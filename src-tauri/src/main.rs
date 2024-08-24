// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod transcode;
mod whisper;

use std::{
	fs,
	path::{Path, PathBuf},
	sync::{
		mpsc::{channel, Receiver, Sender},
		Arc, LazyLock, Mutex
	},
	time::Instant
};

use anyhow::{Context, Result};
use delta_e::DE2000;
use fn_error_context::context;
use image::{ImageBuffer, Rgb};
use itertools::Itertools;
use ndarray::Axis;
use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use serde_json::to_string;
use tauri::{async_runtime, AppHandle, Manager};
use tempfile::tempdir;
use transcode::transcode;
use tryvial::try_fn;
use video_rs::Frame;
use whisper::transcribe;

static WHISPER_PROGRESS_SENDER: Mutex<Option<Sender<i32>>> = Mutex::new(None);

fn main() {
	tauri::Builder::default()
		.invoke_handler(tauri::generate_handler![rs_process_regions])
		.setup(|app| {
			let (tx, rx) = channel();

			let _ = WHISPER_PROGRESS_SENDER.lock().unwrap().insert(tx);

			let app = app.handle();

			async_runtime::spawn(async move {
				let mut first_time = None;

				while let Ok(progress) = rx.recv() {
					app.emit_all(
						"progress",
						Progress::Transcribing(ExtendedProgress::Progress(
							(progress as f32 / 100.0).clamp(0.0, 1.0),
							(Instant::now() - *first_time.get_or_insert(Instant::now())).as_secs_f32()
								/ progress as f32 * (100 - progress) as f32
						))
					)
					.unwrap();
				}
			});

			Ok(())
		})
		.run(tauri::generate_context!())
		.expect("error while running tauri application");
}

// Proportion of same pixels between two frames
// fn similarity(x: &[u8], y: &[u8]) -> f64 {
// 	let mut same = 0;

// 	for i in 0..x.len() {
// 		if x[i] == y[i] {
// 			same += 1;
// 		}
// 	}

// 	same as f64 / x.len() as f64
// }

// Average similarity of colours based on Oklab
fn similarity(x: &[(u8, u8, u8)], y: &[(u8, u8, u8)]) -> f64 {
	(100.0
		- (x.par_iter()
			.zip(y)
			.map(|((r1, g1, b1), (r2, g2, b2))| DE2000::from_rgb(&[*r1, *g1, *b1], &[*r2, *g2, *b2]))
			.sum::<f32>() as f64
			/ x.len() as f64))
		/ 100.0
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "camelCase")]
pub enum Progress {
	Transcoding(BasicProgress),
	Transcribing(ExtendedProgress),
	Processing(ExtendedProgress),
	GatheringPreviews(ExtendedProgress),
	Finalising(BasicProgress)
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum BasicProgress {
	Started,
	Done
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "camelCase")]
pub enum ExtendedProgress {
	Preparing,
	Progress(f32, f32),
	Done
}

#[tauri::command]
async fn rs_process_regions(app: AppHandle, video_path: PathBuf) -> Result<(), String> {
	process_regions(&app, &video_path).map_err(|x| format!("{x:?}"))
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Region {
	pub segments: Vec<Segment>,
	pub words: Vec<Segment>,
	pub start: f32,
	pub end: f32,
	pub summary: String
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
	pub text: String,
	pub start: f32,
	pub end: f32
}

#[try_fn]
#[context("Couldn't process regions")]
fn process_regions(app: &AppHandle, video_path: &Path) -> Result<()> {
	let temp = tempdir().context("Couldn't get temporary folder")?;

	let output_path = app
		.path_resolver()
		.app_data_dir()
		.context("Couldn't get app data folder")?
		.join("videos")
		.join({
			blake3::Hasher::new()
				.update_rayon(&fs::read(video_path).context("Couldn't read video")?)
				.finalize()
				.to_string()
		});

	// We've already processed this video
	if output_path.join("regions.json").exists() {
		app.emit_all("complete", output_path)?;
		return Ok(());
	}

	fs::create_dir_all(&output_path).context("Couldn't ensure output folder")?;

	let (a, b) = rayon::join(
		|| {
			anyhow::Ok({
				app.emit_all("progress", Progress::Transcoding(BasicProgress::Started))?;
				transcode(video_path, temp.path().join("audio.wav")).context("Couldn't transcode video to WAV")?;
				app.emit_all("progress", Progress::Transcoding(BasicProgress::Done))?;

				app.emit_all("progress", Progress::Transcribing(ExtendedProgress::Preparing))?;

				let (mut segments, mut words) = transcribe(temp.path().join("audio.wav"), move |progress| {
					WHISPER_PROGRESS_SENDER
						.lock()
						.unwrap()
						.as_ref()
						.unwrap()
						.send(progress.clamp(0, 100))
						.unwrap();
				})
				.context("Couldn't transcribe audio")?;

				app.emit_all("progress", Progress::Transcribing(ExtendedProgress::Done))?;

				(segments, words)
			})
		},
		|| {
			anyhow::Ok({
				app.emit_all("progress", Progress::Processing(ExtendedProgress::Preparing))?;

				let mut video = video_rs::Decoder::new(video_path).context("Couldn't open video")?;

				let (width, height) = video.size();

				let mut splits = vec![];

				let mut last_frame: Option<Frame> = None;

				let mut skipping = 0;

				let total_secs = video.duration()?.as_secs();

				let start_time = Instant::now();

				'l: while let Ok((time, frame)) = {
					if skipping > 0 {
						skipping -= 1;
						let _ = video.decode_raw();
						continue 'l;
					} else {
						video.decode()
					}
				} {
					app.emit_all(
						"progress",
						Progress::Processing(ExtendedProgress::Progress(
							time.as_secs() / total_secs,
							(Instant::now() - start_time).as_secs_f32() / time.as_secs()
								* (total_secs - time.as_secs())
						))
					)?;

					if let Some(last_frame) = last_frame {
						let sim = similarity(
							&last_frame
								.slice(ndarray::s![.., .., 0..3])
								.to_slice()
								.unwrap()
								.iter()
								.copied()
								.tuples()
								.collect_vec(),
							&frame
								.slice(ndarray::s![.., .., 0..3])
								.to_slice()
								.unwrap()
								.iter()
								.copied()
								.tuples()
								.collect_vec()
						);

						if sim < 0.99 {
							if time.as_secs() - splits.last().unwrap() > 1.0 {
								splits.push(time.as_secs());
							}
						} else {
							skipping = 15;
						}
					} else {
						splits.push(time.as_secs());
					}

					last_frame = Some(frame);
				}

				splits.push(total_secs);

				app.emit_all("progress", Progress::Processing(ExtendedProgress::Done))?;

				app.emit_all("progress", Progress::GatheringPreviews(ExtendedProgress::Preparing))?;

				video.seek_to_start()?;

				let frame_rate = video.frame_rate();

				let middle_frames = splits
					.iter()
					.tuple_windows()
					.map(|(start, end)| (start + end) / 2.0)
					.map(|x| (x * frame_rate).round() as usize)
					.collect_vec();

				let frames_to_decode = *middle_frames.last().unwrap_or(&0) as f32;

				let start_time = Instant::now();

				let mut frame = 0;
				for (idx, middle_frame) in middle_frames.into_iter().enumerate() {
					while frame != middle_frame {
						video.decode_raw()?;
						frame += 1;

						app.emit_all(
							"progress",
							Progress::GatheringPreviews(ExtendedProgress::Progress(
								frame as f32 / frames_to_decode,
								(Instant::now() - start_time).as_secs_f32() / frame as f32
									* (frames_to_decode - frame as f32)
							))
						)?;
					}

					let frame = video.decode()?.1;

					let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_raw(
						width,
						height,
						frame
							.slice(ndarray::s![.., .., 0..3])
							.to_slice()
							.context("Couldn't slice frame as image")?
							.to_vec()
					)
					.unwrap();

					img.save(output_path.join(format!("{}.png", idx)))?;
				}

				app.emit_all("progress", Progress::GatheringPreviews(ExtendedProgress::Done))?;

				splits
			})
		}
	);

	let ((segments, words), splits) = (a?, b?);

	app.emit_all("progress", Progress::Finalising(BasicProgress::Started))?;

	let mut split_segments = vec![];

	for (split_start, split_end) in splits.iter().tuple_windows() {
		let mut included_segments = segments
			.iter()
			.filter(|(_, start, end)| {
				((*start as f32 / 100.0) >= *split_start && (*start as f32 / 100.0) <= *split_end)
					|| ((*end as f32 / 100.0) >= *split_start && (*end as f32 / 100.0) <= *split_end)
			})
			.map(|(text, start, end)| Segment {
				text: text.to_owned(),
				start: *start as f32 / 100.0,
				end: *end as f32 / 100.0
			})
			.collect_vec();

		let mut included_words = words
			.iter()
			.filter(|(_, start, end)| {
				((*start as f32 / 100.0) >= *split_start && (*start as f32 / 100.0) <= *split_end)
					|| ((*end as f32 / 100.0) >= *split_start && (*end as f32 / 100.0) <= *split_end)
			})
			.map(|(text, start, end)| Segment {
				text: text.to_owned(),
				start: *start as f32 / 100.0,
				end: *end as f32 / 100.0
			})
			.collect_vec();

		// Remove [BLANK_AUDIO] tokens

		if included_segments
			.last()
			.map(|Segment { text, .. }| text)
			.unwrap_or(&String::new())
			.trim()
			.starts_with('[')
		{
			included_segments.pop();
		}

		if included_words
			.last()
			.map(|Segment { text, .. }| text)
			.unwrap_or(&String::new())
			.trim()
			.starts_with('[')
		{
			included_words.pop();
		}

		split_segments.push(Region {
			start: *split_start,
			end: *split_end,
			summary: included_segments
				.iter()
				.map(|Segment { text, .. }| text.to_owned())
				.collect::<Vec<_>>()
				.join(""),
			segments: included_segments,
			words: included_words
		});
	}

	fs::write(output_path.join("regions.json"), to_string(&split_segments)?)?;

	app.emit_all("progress", Progress::Finalising(BasicProgress::Done))?;

	app.emit_all("complete", output_path)?;
}
