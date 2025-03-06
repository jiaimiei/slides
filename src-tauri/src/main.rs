#![feature(try_blocks)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Prevents additional console window on Windows in release, DO NOT REMOVE!!

mod commands;
mod processing;
mod transcode;
mod whisper;

use std::{
	fs,
	sync::{
		mpsc::{channel, Sender},
		Arc, Mutex
	},
	time::Instant
};

use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, to_string};
use specta::{
	collect_types,
	ts::{BigIntExportBehavior, ExportConfiguration},
	Type
};
use tauri::{async_runtime, Manager};
use tauri_specta::ts;

use crate::{
	commands::{rs_get_settings, rs_save_current_time, rs_save_settings},
	processing::rs_process_regions
};

// #[global_allocator]
// static ALLOC: snmalloc_rs::SnMalloc = snmalloc_rs::SnMalloc;

static WHISPER_PROGRESS_SENDER: Mutex<Option<Sender<i32>>> = Mutex::new(None);

static DEFAULT_PROMPT_TEMPLATE: &str = r"The following is an excerpt from a lecture transcript:

##text##

Reformat this excerpt in paragraphed, readable form. Correct any spelling or grammar issues. Give only the reformatted text in your response.";

#[derive(Serialize, Deserialize, Clone, Type)]
pub struct AISettings {
	use_ai: bool,
	base_url: String,
	key: String,
	model: String,
	prompt_template: String
}

#[derive(Serialize, Deserialize, Clone, Type)]
pub struct AppSettings {
	ai: AISettings
}

fn main() {
	#[cfg(debug_assertions)]
	ts::export_with_cfg(
		collect_types![
			rs_process_regions,
			rs_save_current_time,
			rs_get_settings,
			rs_save_settings
		]
		.unwrap(),
		ExportConfiguration::new().bigint(BigIntExportBehavior::Number),
		"../src/lib/bindings.ts"
	)
	.unwrap();

	tauri::Builder::default()
		.invoke_handler(tauri::generate_handler![
			rs_process_regions,
			rs_save_current_time,
			rs_get_settings,
			rs_save_settings
		])
		.setup(|app| {
			let (tx, rx) = channel();

			let _ = WHISPER_PROGRESS_SENDER.lock().unwrap().insert(tx);

			if !app
				.path_resolver()
				.app_data_dir()
				.unwrap()
				.join("settings.json")
				.exists() || from_slice::<AppSettings>(
				&fs::read(app.path_resolver().app_data_dir().unwrap().join("settings.json")).unwrap()
			)
			.is_err()
			{
				fs::write(
					app.path_resolver().app_data_dir().unwrap().join("settings.json"),
					to_string(&AppSettings {
						ai: AISettings {
							use_ai: false,
							base_url: "https://api.mistral.ai/v1".into(),
							key: "".into(),
							model: "mistral-large-latest".into(),
							prompt_template: DEFAULT_PROMPT_TEMPLATE.into()
						}
					})
					.unwrap()
				)
				.unwrap();
			}

			app.manage::<ArcSwap<AppSettings>>(
				Arc::new(
					from_slice::<AppSettings>(
						&fs::read(app.path_resolver().app_data_dir().unwrap().join("settings.json")).unwrap()
					)
					.unwrap()
				)
				.into()
			);

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

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "camelCase")]
pub enum Progress {
	Transcoding(BasicProgress),
	Downloading(ExtendedProgress),
	Transcribing(ExtendedProgress),
	Processing(ExtendedProgress),
	GatheringPreviews(ExtendedProgress),
	Summarising(ExtendedProgress)
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
