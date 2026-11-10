//! Decode every gifv frame on demand, retaining its presentation timestamp rather than a
//! sampled whole-loop RGBA cache. The shared stream owns cancellation and UI admission.
use super::{FrameBudget, Input, Reader};
use eframe::egui;
use std::{
	io::BufReader,
	sync::Arc,
	task::Poll,
	time::{Duration, Instant},
};

pub(super) fn open(input: &Input, budget: &FrameBudget) -> Option<platform::video::Decoder> {
	let source = Box::new(BufReader::new(Reader {
		input: input.clone(),
		position: 0,
	}));
	let decoder = super::super::open_preview_source_until(
		source,
		Instant::now() + Duration::from_secs(15),
		budget.fit,
	)?;
	let info = decoder.info();
	if info.width == 0
		|| info.height == 0
		|| info.width.max(info.height) > super::ANIMATION_CANVAS
		|| !info.duration.is_finite()
		|| info.duration <= 0.0
	{
		return None;
	}
	input.budget(info.width, info.height, budget)?;
	Some(decoder)
}

pub(super) fn cycle(
	decoder: &mut platform::video::Decoder,
	input: &Input,
	budget: &FrameBudget,
	mut present: impl FnMut(Duration, Arc<egui::ColorImage>) -> bool,
) -> Option<(usize, Duration)> {
	let duration = Duration::try_from_secs_f64(decoder.info().duration).ok()?;
	let mut origin = None;
	let mut previous = None;
	let mut count = 0;
	let mut progress = Instant::now();
	loop {
		if input.producer.activity(Instant::now()).is_none()
			|| progress.elapsed() > Duration::from_secs(15)
		{
			return None;
		}
		match decoder.poll_video().ok()? {
			Poll::Pending => {
				let _ = decoder.poll_audio();
				input.producer.wait(Duration::from_millis(2));
			}
			Poll::Ready(None) => break,
			Poll::Ready(Some(platform::video::Sample::Video { pts, picture })) => {
				if count >= super::MAX_FRAMES {
					return None;
				}
				// Preserve sub-20-ms timing (e.g. 60 fps), normalize composition offsets,
				// and reject backwards/out-of-range timestamps before scheduling a wait.
				let at = frame_time(pts, &mut origin, previous, duration)?;
				let (width, height) = picture.display_size();
				if width.max(height) > super::ANIMATION_CANVAS {
					return None;
				}
				let (width, height, rgba) = picture.to_rgba()?;
				let image =
					super::resize_to(image::RgbaImage::from_raw(width, height, rgba)?, budget.fit);
				let image = Arc::new(egui::ColorImage::from_rgba_unmultiplied(
					[image.width() as usize, image.height() as usize],
					image.as_raw(),
				));
				if !present(at, image) {
					return None;
				}
				previous = Some(at);
				count += 1;
				progress = Instant::now();
				// No audio is played. Drain the sibling track so bounded demuxer queues
				// cannot hold up video on backends that read both tracks together.
				let _ = decoder.poll_audio();
			}
			Poll::Ready(Some(_)) => {}
		}
	}
	(count >= 2).then_some((count, duration))
}

fn frame_time(
	pts: f64,
	origin: &mut Option<f64>,
	previous: Option<Duration>,
	duration: Duration,
) -> Option<Duration> {
	if !pts.is_finite() {
		return None;
	}
	let at = Duration::try_from_secs_f64(pts - *origin.get_or_insert(pts)).ok()?;
	(previous.is_none_or(|previous| at > previous) && at < duration).then_some(at)
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn timestamps_preserve_sixty_fps_and_offsets_and_reject_invalid_waits() {
		let mut origin = None;
		let duration = Duration::from_secs(1);
		let first = frame_time(7.0, &mut origin, None, duration).unwrap();
		let second = frame_time(7.0 + 1.0 / 60.0, &mut origin, Some(first), duration).unwrap();
		assert_eq!(first, Duration::ZERO);
		assert_eq!(second.as_micros(), 16_666);
		for invalid in [f64::NAN, f64::INFINITY, 6.0, 7.0, 8.0] {
			assert!(frame_time(invalid, &mut origin, Some(second), duration).is_none());
		}
	}

	#[test]
	fn native_video_keeps_all_frames_with_a_tiny_cache_budget_and_rewinds() {
		let bytes = include_bytes!("../../tests/fixtures/video.mov");
		let (stream, producer) = ui::AnimationStream::channel(128, 2 * 1024 * 1024);
		let input = Input::new(producer, 2 * 1024 * 1024);
		assert!(input.cached(bytes.to_vec()));
		let budget = FrameBudget {
			fit: 128,
			bytes: 2 * 1024 * 1024,
			count: 2,
			shrinks: 0,
		};
		let decoder = open(&input, &budget);
		#[cfg(not(target_os = "windows"))]
		if decoder.is_none() {
			return;
		} // Installed native codecs are optional off Windows.
		let mut decoder = decoder.expect("native H.264 fixture");
		for _ in 0..2 {
			let mut times = Vec::new();
			let (frames, duration) = cycle(&mut decoder, &input, &budget, |at, _| {
				times.push(at);
				let _ = stream.frame();
				true
			})
			.expect("whole native loop");
			assert_eq!(
				frames, 72,
				"never thin the 24 fps fixture down to the cached-frame count"
			);
			assert_eq!(times[0], Duration::ZERO);
			assert!((2.99..=3.01).contains(&duration.as_secs_f64()));
			assert!(
				times
					.windows(2)
					.all(|pair| (41_600..=41_700).contains(&(pair[1] - pair[0]).as_micros()))
			);
			assert!(stream.reserved_bytes() <= budget.bytes);
			decoder.seek(0.0).unwrap();
		}
	}

	#[test]
	#[ignore = "set SEREIN_MEDIA_CLIP to a local MP4 for full-source cadence measurements"]
	fn local_video_stream_workload() {
		let bytes =
			std::fs::read(std::env::var_os("SEREIN_MEDIA_CLIP").expect("local clip path")).unwrap();
		for (name, fit, limit) in [("inline", 368, 40), ("viewer", 720, 96)] {
			let (stream, producer) = ui::AnimationStream::channel(fit, limit * 1024 * 1024);
			let input = Input::new(producer, 16 * 1024 * 1024);
			assert!(input.cached(bytes.clone()));
			let budget = FrameBudget {
				fit,
				bytes: limit * 1024 * 1024,
				count: 240,
				shrinks: 0,
			};
			let started = Instant::now();
			let mut decoder = open(&input, &budget).expect("native clip");
			let mut first = None;
			let mut previous = Duration::ZERO;
			let mut largest_gap = Duration::ZERO;
			let (count, duration) = cycle(&mut decoder, &input, &budget, |at, _| {
				first.get_or_insert_with(|| started.elapsed());
				largest_gap = largest_gap.max(at - previous);
				previous = at;
				let _ = stream.frame();
				true
			})
			.expect("full source loop");
			eprintln!(
				"{name}: {count} frames, {duration:?}, {:.3} fps, largest timestamp gap {largest_gap:?}, first {first:?}, total {:?}, reserved {} bytes",
				count as f64 / duration.as_secs_f64(),
				started.elapsed(),
				stream.reserved_bytes()
			);
		}
	}
}
