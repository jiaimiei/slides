use std::{fs, ops::Deref, path::PathBuf};

use anyhow::{Context, Result};
use arc_swap::ArcSwap;
use fn_error_context::context;
use macros::tauri_command;
use serde_json::to_string;
use tauri::{AppHandle, Manager};
use tryvial::try_fn;

use crate::AppSettings;

#[tauri_command]
#[try_fn]
#[context("Failed to get settings")]
fn get_settings(app: &AppHandle) -> Result<AppSettings> {
	app.state::<ArcSwap<AppSettings>>().load().deref().deref().to_owned()
}

#[tauri_command]
#[try_fn]
#[context("Failed to save settings")]
fn save_settings(app: &AppHandle, settings: AppSettings) -> Result<()> {
	fs::write(
		app.path_resolver()
			.app_data_dir()
			.context("Couldn't get app data dir")?
			.join("settings.json"),
		to_string(&settings)?
	)?;

	app.state::<ArcSwap<AppSettings>>().store(settings.into());
}

#[tauri_command]
#[try_fn]
#[context("Failed to save current time")]
fn save_current_time(data_path: PathBuf, time: f64) -> Result<()> {
	fs::write(data_path.join("current_time.txt"), time.to_string())?;
}
