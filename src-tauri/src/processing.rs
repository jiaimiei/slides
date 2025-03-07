use std::{
	fs::{self, File},
	io::Write,
	path::PathBuf,
	sync::{LazyLock, Mutex},
	time::{Duration, Instant}
};

use crate::{transcode::transcode, AppSettings, BasicProgress, ExtendedProgress, Progress};
use crate::{whisper::transcribe, WHISPER_PROGRESS_SENDER};

use anyhow::{Context, Result};
use arc_swap::ArcSwap;
use delta_e::DE2000;
use fn_error_context::context;
use futures::StreamExt;
use image::{ImageBuffer, Rgb};
use itertools::Itertools;
use macros::async_tauri_command;
use openai_dive::v1::{
	api::Client,
	resources::chat::{ChatCompletionParametersBuilder, ChatMessage, ChatMessageContent}
};
use rand::{thread_rng, Rng};
use rayon::{
	iter::{IndexedParallelIterator, ParallelIterator},
	slice::ParallelSlice
};
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, from_value, to_string, Value};
use tauri::{
	async_runtime::{self, JoinHandle},
	AppHandle, Manager
};
use tempfile::tempdir;
use tryvial::try_fn;
use video_rs::Frame;
use warp::Filter;

static SERVER_SECRET: LazyLock<String> =
	LazyLock::new(|| thread_rng().gen::<[u64; 4]>().map(|x| x.to_string()).join("-"));

static SERVER_HANDLE: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

static MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin?download=true";
static MODEL_HASH: &str = "43806203079b34211f185a19116492944db21f9ee14aa0f3c5d6cfe7e81a7861";

// Average similarity of colours based on Oklab
fn similarity(x: &[u8], y: &[u8]) -> f32 {
	(100.0
		- (x.par_chunks_exact(3)
			.zip(y.par_chunks_exact(3))
			.map(|(one, two)| DE2000::from_rgb(one.try_into().unwrap(), two.try_into().unwrap()))
			.sum::<f32>()
			/ x.len() as f32))
		/ 100.0
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Region {
	pub segments: Vec<Segment>,
	pub words: Option<Vec<Segment>>,
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

#[async_tauri_command]
#[try_fn]
#[context("Couldn't process regions")]
async fn process_regions(app: &AppHandle, video_path: &PathBuf) -> Result<()> {
	let temp = tempdir().context("Couldn't get temporary folder")?;

	let model_path = app
		.path_resolver()
		.app_data_dir()
		.context("Couldn't get app data folder")?
		.join("model.bin");

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
		drop(SERVER_HANDLE.lock().unwrap().take().inspect(|x| x.abort()));

		SERVER_HANDLE.lock().unwrap().replace(async_runtime::spawn({
			warp::serve(
				warp::path(SERVER_SECRET.to_string())
					.and(warp::path::end())
					.and(warp::fs::file(video_path.to_owned()))
			)
			.run(([127, 0, 0, 1], 52937))
		}));

		app.emit_all("complete", (SERVER_SECRET.to_owned(), output_path))?;

		return Ok(());
	}

	fs::create_dir_all(&output_path).context("Couldn't ensure output folder")?;

	let (a, b) = rayon::join(
		|| {
			anyhow::Ok({
				let json_path = {
					let mut x = video_path.to_owned();
					x.pop();
					x.join(format!("{}.json", video_path.file_stem().unwrap().to_str().unwrap()))
				};

				if json_path.exists() {
					app.emit_all("progress", Progress::Transcribing(ExtendedProgress::Preparing))?;

					let transcript =
						from_slice::<Value>(&fs::read(&json_path).context("Couldn't read transcription JSON")?)
							.context("Couldn't deserialise transcription JSON")?;

					let segments = from_value::<Vec<Segment>>(
						transcript.get("segments").context("Couldn't get segments")?.to_owned()
					)
					.context("Couldn't deserialise segments")?;

					app.emit_all("progress", Progress::Transcribing(ExtendedProgress::Done))?;

					(
						segments
							.into_iter()
							.map(|Segment { text, start, end }| {
								(text, (start * 100.0).round() as i64, (end * 100.0).round() as i64)
							})
							.collect(),
						None
					)
				} else {
					app.emit_all("progress", Progress::Transcoding(BasicProgress::Started))?;
					transcode(video_path, temp.path().join("audio.wav")).context("Couldn't transcode video to WAV")?;
					app.emit_all("progress", Progress::Transcoding(BasicProgress::Done))?;

					if !model_path.exists()
						|| blake3::Hasher::new()
							.update_rayon(&fs::read(&model_path).context("Couldn't read video")?)
							.finalize()
							.to_string() != MODEL_HASH
					{
						app.emit_all("progress", Progress::Downloading(ExtendedProgress::Preparing))?;

						async_runtime::block_on(async {
							let res = reqwest::get(MODEL_URL).await?.error_for_status()?;

							let total_size = res.content_length().context("Couldn't get content length")?;

							let start_time = Instant::now();

							let mut file = File::create(&model_path).context("Couldn't create model file")?;
							let mut downloaded = 0;
							let mut stream = res.bytes_stream();

							while let Some(item) = stream.next().await {
								let chunk = item.context("Error while downloading file")?;

								file.write_all(&chunk).context("Error while writing to file")?;

								let new = total_size.min(downloaded + (chunk.len() as u64));
								downloaded = new;

								app.emit_all(
									"progress",
									Progress::Downloading(ExtendedProgress::Progress(
										downloaded as f32 / total_size as f32,
										(Instant::now() - start_time).as_secs_f32() / downloaded as f32
											* (total_size as f32 - downloaded as f32)
									))
								)?;
							}

							anyhow::Ok(())
						})?;

						app.emit_all("progress", Progress::Downloading(ExtendedProgress::Done))?;
					}

					app.emit_all("progress", Progress::Transcribing(ExtendedProgress::Preparing))?;

					let (segments, words) = transcribe(
						model_path
							.as_os_str()
							.to_str()
							.context("Couldn't interpret model path as string")?,
						temp.path().join("audio.wav"),
						move |progress| {
							WHISPER_PROGRESS_SENDER
								.lock()
								.unwrap()
								.as_ref()
								.unwrap()
								.send(progress.clamp(0, 100))
								.unwrap();
						}
					)
					.context("Couldn't transcribe audio")?;

					app.emit_all("progress", Progress::Transcribing(ExtendedProgress::Done))?;

					(segments, Some(words))
				}
			})
		},
		|| {
			anyhow::Ok({
				app.emit_all("progress", Progress::Processing(ExtendedProgress::Preparing))?;

				let mut video = video_rs::Decoder::new(video_path.to_owned()).context("Couldn't open video")?;

				let (width, height) = video.size();

				let frame_rate = video.frame_rate();

				let mut splits = vec![];

				let mut naive_splits = vec![];

				let mut last_frame: Option<Frame> = None;

				let total_secs = video.duration()?.as_secs();

				let start_time = Instant::now();
				let mut last_report = Instant::now();

				while let Ok((time, frame)) = video.decode() {
					if Instant::now() - last_report > Duration::from_millis(100) {
						last_report = Instant::now();

						app.emit_all(
							"progress",
							Progress::Processing(ExtendedProgress::Progress(
								time.as_secs() / total_secs,
								(Instant::now() - start_time).as_secs_f32() / time.as_secs()
									* (total_secs - time.as_secs())
							))
						)?;
					}

					if let Some(last_frame) = last_frame {
						let sim = similarity(last_frame.as_slice().unwrap(), frame.as_slice().unwrap());

						if sim < 0.99 {
							if time.as_secs() - naive_splits.last().unwrap() > 2.0 {
								splits.push(time.as_secs());
							}

							naive_splits.push(time.as_secs());
						} else {
							// Skip one second
							for _ in 0..(frame_rate) as usize {
								let _ = video.decode_raw();
							}
						}
					} else {
						splits.push(time.as_secs());
						naive_splits.push(time.as_secs());
					}

					last_frame = Some(frame);
				}

				splits.push(total_secs);

				app.emit_all("progress", Progress::Processing(ExtendedProgress::Done))?;

				app.emit_all("progress", Progress::GatheringPreviews(ExtendedProgress::Preparing))?;

				video.seek_to_start()?;

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
					let x: Result<_> = try {
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
					};

					if let Err(e) = x {
						eprintln!("Error in saving preview image {idx}: {e}");
					}
				}

				app.emit_all("progress", Progress::GatheringPreviews(ExtendedProgress::Done))?;

				splits
			})
		}
	);

	let ((segments, words), splits) = (a?, b?);

	app.emit_all("progress", Progress::Summarising(ExtendedProgress::Preparing))?;

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

		let included_words = words.as_ref().map(|words| {
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

			if included_words
				.last()
				.map(|Segment { text, .. }| text)
				.unwrap_or(&String::new())
				.trim()
				.starts_with('[')
			{
				included_words.pop();
			}

			included_words
		});

		split_segments.push(Region {
			start: *split_start,
			end: *split_end,
			summary: included_segments
				.iter()
				.map(|Segment { text, .. }| text.to_owned())
				.collect::<Vec<_>>()
				.join(" "),
			segments: included_segments,
			words: included_words
		});
	}

	let settings = app.state::<ArcSwap<AppSettings>>();
	let settings = settings.load();

	if settings.ai.use_ai {
		let client = Client::new_with_base(&settings.ai.base_url, settings.ai.key.to_owned());

		let start_time = Instant::now();

		let total_segments = split_segments.len() as f32;

		for (idx, region) in split_segments.iter_mut().enumerate() {
			region.summary = region.summary.trim().to_owned();

			if !region.summary.is_empty() {
				if let Ok(res) = client
					.chat()
					.create(
						ChatCompletionParametersBuilder::default()
							.model(&settings.ai.model)
							.messages(vec![ChatMessage::User {
								content: ChatMessageContent::Text(
									settings.ai.prompt_template.replace("##text##", &region.summary)
								),
								name: None
							}])
							.build()?
					)
					.await
				{
					if let ChatMessage::Assistant { content, .. } = &res.choices[0].message {
						if let ChatMessageContent::Text(text) = content.as_ref().context("No response content")? {
							if text.split("\n\n").next().context("No response content")?.ends_with(":") {
								region.summary =
									text.split("\n\n").skip(1).collect_vec().join("\n\n").trim().to_owned();
							} else {
								region.summary = text.trim().to_owned();
							}
						}
					}

					tokio::time::sleep(std::time::Duration::from_secs(1)).await;
				}
			}

			app.emit_all(
				"progress",
				Progress::Summarising(ExtendedProgress::Progress(
					(idx + 1) as f32 / total_segments,
					(Instant::now() - start_time).as_secs_f32() / (idx + 1) as f32
						* (total_segments - (idx + 1) as f32)
				))
			)?;
		}
	}

	fs::write(output_path.join("regions.json"), to_string(&split_segments)?)?;

	app.emit_all("progress", Progress::Summarising(ExtendedProgress::Done))?;

	drop(SERVER_HANDLE.lock().unwrap().take().inspect(|x| x.abort()));

	SERVER_HANDLE.lock().unwrap().replace(async_runtime::spawn({
		warp::serve(
			warp::path(SERVER_SECRET.to_string())
				.and(warp::path::end())
				.and(warp::fs::file(video_path.to_owned()))
		)
		.run(([127, 0, 0, 1], 52937))
	}));

	app.emit_all("complete", (SERVER_SECRET.to_owned(), output_path))?;
}
