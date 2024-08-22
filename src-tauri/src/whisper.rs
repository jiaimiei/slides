#![allow(clippy::uninlined_format_args)]

use anyhow::{Context, Result};
use hound::{SampleFormat, WavReader};
use std::path::Path;
use tryvial::try_fn;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

fn parse_wav_file(path: &Path) -> Vec<i16> {
	let reader = WavReader::open(path).expect("failed to read file");

	if reader.spec().channels != 1 {
		panic!("expected mono audio file");
	}
	if reader.spec().sample_format != SampleFormat::Int {
		panic!("expected integer sample format");
	}
	if reader.spec().sample_rate != 16000 {
		panic!("expected 16KHz sample rate");
	}
	if reader.spec().bits_per_sample != 16 {
		panic!("expected 16 bits per sample");
	}

	reader
		.into_samples::<i16>()
		.map(|x| x.expect("sample"))
		.collect::<Vec<_>>()
}

#[try_fn]
pub fn transcribe(
	wav_path: impl AsRef<Path>,
	progress_callback: impl FnMut(i32) + 'static
) -> Result<(Vec<(String, i64, i64)>, Vec<(String, i64, i64)>)> {
	let original_samples = parse_wav_file(wav_path.as_ref());
	let mut samples = vec![0.0f32; original_samples.len()];
	whisper_rs::convert_integer_to_float_audio(&original_samples, &mut samples).context("failed to convert samples")?;

	let ctx = WhisperContext::new_with_params(
		r"C:\Users\User\Documents\Github\slides\model.bin",
		WhisperContextParameters::default()
	)
	.context("failed to open model")?;

	let mut params = FullParams::new(SamplingStrategy::BeamSearch {
		beam_size: 5,
		patience: 1.0
	});

	params.set_print_special(false);
	params.set_print_progress(false);
	params.set_print_realtime(false);
	params.set_print_timestamps(false);
	params.set_token_timestamps(true);
	params.set_split_on_word(true);
	params.set_entropy_thold(2.8);

	params.set_n_threads(num_cpus::get() as i32);

	params.set_progress_callback_safe(progress_callback);

	let mut state = ctx.create_state().context("failed to create key")?;

	state.full(params, &samples).context("failed to convert samples")?;

	let num_segments = state.full_n_segments().expect("failed to get segments");

	let mut words = Vec::new();
	let mut utterances = Vec::new();
	for segment_idx in 0..num_segments {
		let text = state.full_get_segment_text(segment_idx)?;
		let start = state.full_get_segment_t0(segment_idx)?;
		let stop = state.full_get_segment_t1(segment_idx)?;

		utterances.push((text, start, stop));

		let num_tokens = state.full_n_tokens(segment_idx)?;

		for t in 0..num_tokens {
			let text = state.full_get_token_text(segment_idx, t)?;
			let token_data = state.full_get_token_data(segment_idx, t)?;

			if text.starts_with("[_") {
				continue;
			}

			words.push((text, token_data.t0, token_data.t1));
		}
	}

	(utterances, words)
}
