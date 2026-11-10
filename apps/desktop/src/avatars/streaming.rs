//! Bounded original-GIF playback. Network bytes can arrive while the first frames play.
use super::{ANIMATION_ALLOC, ANIMATION_CANVAS, FrameBudget, resize_to};
use eframe::egui;
use image::{AnimationDecoder, ImageDecoder};
use std::{
	io::{self, BufReader, Read, Seek, SeekFrom},
	sync::{Arc, Condvar, LazyLock, Mutex},
	time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};
#[path = "video_stream.rs"]
mod video;

#[derive(Clone, Copy)]
pub(super) enum Format {
	Gif,
	Video,
}

pub(super) const MAX_ENCODED: usize = 40 * 1024 * 1024;
pub(super) static SLOTS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(8)));
const MAX_FRAMES: usize = 3600;

struct Bytes {
	data: Vec<u8>,
	limit: usize,
	complete: Option<bool>,
	expected: Option<usize>,
}
struct Source {
	bytes: Mutex<Bytes>,
	wake: Condvar,
}
#[derive(Clone)]
pub(super) struct Input {
	source: Arc<Source>,
	producer: ui::AnimationProducer,
}

impl Input {
	pub(super) fn new(producer: ui::AnimationProducer, bytes: usize) -> Self {
		Self {
			source: Arc::new(Source {
				bytes: Mutex::new(Bytes {
					data: Vec::new(),
					limit: bytes.min(MAX_ENCODED),
					complete: None,
					expected: None,
				}),
				wake: Condvar::new(),
			}),
			producer,
		}
	}
	pub(super) fn cached(&self, mut data: Vec<u8>) -> bool {
		let Ok(mut bytes) = self.source.bytes.lock() else {
			return false;
		};
		if data.len() > bytes.limit {
			return false;
		}
		data.shrink_to_fit();
		bytes.expected = Some(data.len());
		bytes.data = data;
		bytes.complete = Some(true);
		self.source.wake.notify_all();
		true
	}
	pub(super) fn reserve(&self, length: usize) -> bool {
		let Ok(mut bytes) = self.source.bytes.lock() else {
			return false;
		};
		if length > bytes.limit {
			return false;
		}
		bytes.expected = Some(length);
		let additional = length.saturating_sub(bytes.data.len());
		bytes.data.try_reserve_exact(additional).is_ok()
	}
	pub(super) fn append(&self, chunk: &[u8]) -> bool {
		if self.producer.activity(Instant::now()).is_none() {
			return false;
		}
		let Ok(mut bytes) = self.source.bytes.lock() else {
			return false;
		};
		if bytes.complete.is_some()
			|| bytes
				.data
				.len()
				.checked_add(chunk.len())
				.is_none_or(|end| end > bytes.limit)
		{
			return false;
		}
		if bytes.data.try_reserve_exact(chunk.len()).is_err() {
			return false;
		}
		bytes.data.extend_from_slice(chunk);
		self.source.wake.notify_all();
		true
	}
	pub(super) fn finish(&self, success: bool) {
		if let Ok(mut bytes) = self.source.bytes.lock()
			&& bytes.complete.is_none()
		{
			bytes.complete = Some(success);
		}
		self.source.wake.notify_all();
	}
	fn budget(&self, width: u32, height: u32, budget: &FrameBudget) -> Option<()> {
		let canvas = width as usize * height as usize * 4;
		let (fit_width, fit_height) = ui::fit_edge(width, height, budget.fit);
		let fitted = fit_width as usize * fit_height as usize * 4;
		// The GIF decoder keeps three source canvases. Reserve fitted conversion/handoff,
		// first-frame delivery and playback textures too; no complete RGBA loop is retained.
		let work = 3 * canvas + 6 * fitted;
		let limit = budget.bytes.checked_sub(work)?.min(MAX_ENCODED);
		let mut bytes = self.source.bytes.lock().ok()?;
		if bytes.expected.is_some_and(|expected| expected > limit) {
			return None;
		}
		let limit = limit.min(bytes.limit).min(bytes.expected.unwrap_or(limit));
		if bytes.data.len() > limit {
			return None;
		}
		if bytes.data.capacity() > limit {
			bytes.data.shrink_to_fit();
		}
		if bytes.data.capacity() > limit || !self.producer.constrain(limit + work) {
			return None;
		}
		bytes.limit = limit;
		Some(())
	}
}

/// Unblocks the synchronous decoder if an async load is cancelled or its download fails.
pub(super) struct DownloadGuard(pub Input);
impl Drop for DownloadGuard {
	fn drop(&mut self) {
		self.0.finish(false);
	}
}

struct Reader {
	input: Input,
	position: usize,
}
impl Seek for Reader {
	fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
		let base = match from {
			SeekFrom::Start(at) => {
				let limit = self
					.input
					.source
					.bytes
					.lock()
					.map_err(|_| io::ErrorKind::Other)?
					.limit;
				if at > limit as u64 {
					return Err(io::ErrorKind::InvalidInput.into());
				}
				self.position = at as usize;
				return Ok(at);
			}
			SeekFrom::Current(_) => self.position,
			SeekFrom::End(_) => loop {
				if self.input.producer.activity(Instant::now()).is_none() {
					return Err(io::ErrorKind::ConnectionAborted.into());
				}
				let bytes = self
					.input
					.source
					.bytes
					.lock()
					.map_err(|_| io::ErrorKind::Other)?;
				match bytes.complete {
					Some(true) => break bytes.data.len(),
					Some(false) => return Err(io::ErrorKind::UnexpectedEof.into()),
					None => {
						if let Some(expected) = bytes.expected {
							break expected;
						}
					}
				}
				let _ = self
					.input
					.source
					.wake
					.wait_timeout(bytes, Duration::from_millis(100));
			},
		};
		let offset = match from {
			SeekFrom::Current(offset) | SeekFrom::End(offset) => offset,
			_ => unreachable!(),
		};
		let at = (base as i128) + i128::from(offset);
		let limit = self
			.input
			.source
			.bytes
			.lock()
			.map_err(|_| io::ErrorKind::Other)?
			.limit;
		if at < 0 || at > limit as i128 {
			return Err(io::ErrorKind::InvalidInput.into());
		}
		self.position = at as usize;
		Ok(self.position as u64)
	}
}
impl Read for Reader {
	fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
		if output.is_empty() {
			return Ok(0);
		}
		loop {
			if self.input.producer.activity(Instant::now()).is_none() {
				return Err(io::ErrorKind::ConnectionAborted.into());
			}
			let mut bytes = self
				.input
				.source
				.bytes
				.lock()
				.map_err(|_| io::ErrorKind::Other)?;
			if self.position < bytes.data.len() {
				let count = output.len().min(bytes.data.len() - self.position);
				output[..count].copy_from_slice(&bytes.data[self.position..self.position + count]);
				self.position += count;
				return Ok(count);
			}
			if let Some(complete) = bytes.complete {
				return if complete {
					Ok(0)
				} else {
					Err(io::ErrorKind::UnexpectedEof.into())
				};
			}
			bytes = self
				.input
				.source
				.wake
				.wait_timeout(bytes, Duration::from_millis(100))
				.map_err(|_| io::ErrorKind::Other)?
				.0;
			drop(bytes);
		}
	}
}

/// Decode one full loop without temporal sampling. Scheduling is supplied separately so tests
/// inspect authored timing and every frame without wall-clock sleeps.
pub(super) fn cycle(
	input: &Input,
	budget: &FrameBudget,
	mut present: impl FnMut(Duration, Duration, Arc<egui::ColorImage>) -> bool,
) -> Option<(usize, Duration)> {
	let reader = Reader {
		input: input.clone(),
		position: 0,
	};
	let mut decoder = image::codecs::gif::GifDecoder::new(BufReader::new(reader)).ok()?;
	let (width, height) = decoder.dimensions();
	let mut limits = image::Limits::default();
	limits.max_image_width = Some(ANIMATION_CANVAS);
	limits.max_image_height = Some(ANIMATION_CANVAS);
	limits.max_alloc = Some(ANIMATION_ALLOC);
	decoder.set_limits(limits).ok()?;
	input.budget(width, height, budget)?;
	let mut elapsed = Duration::ZERO;
	let mut count = 0;
	for frame in decoder.into_frames() {
		if count >= MAX_FRAMES {
			return None;
		}
		let frame = frame.ok()?;
		let (num, den) = frame.delay().numer_denom_ms();
		let delay = Duration::from_nanos(u64::from(num) * 1_000_000 / u64::from(den.max(1)))
			.clamp(Duration::from_millis(20), Duration::from_secs(10));
		let image = resize_to(frame.into_buffer(), budget.fit);
		let image = Arc::new(egui::ColorImage::from_rgba_unmultiplied(
			[image.width() as usize, image.height() as usize],
			image.as_raw(),
		));
		if !present(elapsed, delay, image) {
			return None;
		}
		elapsed += delay;
		count += 1;
	}
	(count >= 2).then_some((count, elapsed))
}

fn wait_for_frame(producer: &ui::AnimationProducer, anchor: &mut Instant, at: Duration) -> bool {
	loop {
		match producer.activity(Instant::now()) {
			None => return false,
			Some(ui::StreamActivity::Paused) => {
				let paused = Instant::now();
				while producer.activity(Instant::now()) == Some(ui::StreamActivity::Paused) {
					producer.wait(Duration::from_millis(100));
				}
				*anchor += paused.elapsed();
			}
			Some(ui::StreamActivity::Playing) => {
				let due = *anchor + at;
				let now = Instant::now();
				if now >= due {
					return true;
				}
				producer.wait(due - now);
			}
		}
	}
}

pub(super) fn spawn(
	input: Input,
	budget: FrameBudget,
	producer: ui::AnimationProducer,
	permit: OwnedSemaphorePermit,
	ctx: egui::Context,
	format: Format,
) -> io::Result<oneshot::Receiver<egui::ColorImage>> {
	let (send, receive) = oneshot::channel();
	std::thread::Builder::new()
		.name("serein-gif-stream".into())
		.spawn(move || {
			let _permit = permit;
			let _ = run(input, budget, &producer, send, &ctx, format);
			producer.finish(producer.activity(Instant::now()).is_some());
			ctx.request_repaint();
		})?;
	Ok(receive)
}

fn run(
	input: Input,
	budget: FrameBudget,
	producer: &ui::AnimationProducer,
	send: oneshot::Sender<egui::ColorImage>,
	ctx: &egui::Context,
	format: Format,
) -> Option<()> {
	let mut decoder = match format {
		Format::Gif => None,
		Format::Video => Some(video::open(&input, &budget)?),
	};
	let mut ready = Some(send);
	let mut anchor = Instant::now();
	loop {
		let mut present = |at, image: Arc<egui::ColorImage>| {
			if ready.is_some() {
				anchor = Instant::now();
			}
			if !wait_for_frame(producer, &mut anchor, at) || !producer.publish(image.clone()) {
				return false;
			}
			if let Some(ready) = ready.take() {
				let _ = ready.send(image.as_ref().clone());
			}
			ctx.request_repaint();
			true
		};
		let (_, duration) = match &mut decoder {
			Some(decoder) => video::cycle(decoder, &input, &budget, present),
			None => cycle(&input, &budget, |at, _, image| present(at, image)),
		}?;
		// Decode the next loop's opening frame during the final frame's hold.
		// Its callback still waits for the authored boundary, avoiding a decode-sized
		// hitch from waiting until that boundary before rewinding the source.
		anchor += duration;
		if let Some(decoder) = &mut decoder {
			decoder.seek(0.0).ok()?;
		}
	}
}

#[cfg(feature = "demo")]
#[allow(dead_code)] // Used by the standalone native cadence example, not the normal desktop.
pub(crate) fn demo_stream(
	ctx: egui::Context,
	original: Option<Vec<u8>>,
) -> (ui::AnimationStream, oneshot::Receiver<egui::ColorImage>) {
	let original_source = original.is_some();
	let mut bytes = Vec::new();
	if let Some(original) = original {
		bytes = original;
	} else {
		let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
		for frame in 0..40 {
			let mut image = image::RgbaImage::from_pixel(80, 80, image::Rgba([35, 39, 48, 255]));
			for y in 12..68 {
				for x in frame..frame + 16 {
					image.put_pixel(x, y, image::Rgba([103, 192, 177, 255]));
				}
			}
			encoder
				.encode_frame(image::Frame::from_parts(
					image,
					0,
					0,
					image::Delay::from_numer_denom_ms(50, 1),
				))
				.unwrap();
		}
	}
	let budget = FrameBudget {
		fit: if original_source { 368 } else { 160 },
		bytes: if original_source { 40 } else { 4 } * 1024 * 1024,
		count: 240,
		shrinks: 0,
	};
	let (stream, producer) = ui::AnimationStream::channel(budget.fit, budget.bytes);
	let input = Input::new(producer.clone(), budget.bytes);
	let format = if super::is_isobmff(&bytes) {
		Format::Video
	} else {
		Format::Gif
	};
	assert!(input.cached(bytes));
	let permit = SLOTS
		.clone()
		.try_acquire_owned()
		.expect("synthetic stream slot");
	let ready =
		spawn(input, budget, producer, permit, ctx, format).expect("synthetic decoder thread");
	(stream, ready)
}

#[cfg(test)]
mod tests {
	use super::*;
	// These two loader tests exercise the same process-wide decoder quota.
	static LOADS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
	fn fixture() -> Vec<u8> {
		let mut bytes = Vec::new();
		{
			let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
			for shade in [20, 80, 140] {
				encoder
					.encode_frame(image::Frame::from_parts(
						image::RgbaImage::from_pixel(8, 8, image::Rgba([shade, 0, 0, 255])),
						0,
						0,
						image::Delay::from_numer_denom_ms(50, 1),
					))
					.unwrap();
			}
		}
		bytes
	}
	#[test]
	fn streaming_retains_every_authored_frame_and_rewinds_without_cache_thinning() {
		let (_stream, producer) = ui::AnimationStream::channel(8, 4096);
		let input = Input::new(producer, 4096);
		assert!(input.cached(fixture()));
		let budget = FrameBudget {
			fit: 8,
			bytes: 4096,
			count: 2,
			shrinks: 0,
		};
		for _ in 0..2 {
			let mut frames = Vec::new();
			assert_eq!(
				cycle(&input, &budget, |at, delay, image| {
					frames.push((at, delay, image.pixels()[0].r()));
					true
				}),
				Some((3, Duration::from_millis(150)))
			);
			assert_eq!(
				frames,
				vec![
					(Duration::ZERO, Duration::from_millis(50), 20),
					(Duration::from_millis(50), Duration::from_millis(50), 80),
					(Duration::from_millis(100), Duration::from_millis(50), 140),
				]
			);
		}
		// A declared body must fit alongside decoded canvases, even if only a short
		// prefix has arrived. Reject before delivering frames instead of exceeding the lane.
		let (_stream, producer) = ui::AnimationStream::channel(8, 8192);
		let oversized = Input::new(producer, 8192);
		assert!(oversized.reserve(5000));
		assert!(oversized.append(&fixture()));
		oversized.finish(true);
		assert!(
			cycle(&oversized, &budget, |_, _, _| panic!(
				"oversized body admitted"
			))
			.is_none()
		);
	}
	#[test]
	fn source_bounds_truncation_and_consumer_cancellation_stop_reads() {
		let (stream, producer) = ui::AnimationStream::channel(8, 4096);
		let input = Input::new(producer, 4);
		assert!(!input.reserve(5));
		assert!(input.append(&[1, 2, 3, 4]));
		assert!(!input.append(&[5]));
		input.finish(false);
		let mut reader = Reader { input, position: 0 };
		let mut bytes = [0; 4];
		assert_eq!(reader.read(&mut bytes).unwrap(), 4);
		assert_eq!(
			reader.read(&mut bytes).unwrap_err().kind(),
			io::ErrorKind::UnexpectedEof
		);
		drop(stream);
		assert_eq!(
			reader.read(&mut bytes).unwrap_err().kind(),
			io::ErrorKind::ConnectionAborted
		);
	}

	#[test]
	fn first_frame_decodes_before_the_body_finishes() {
		let (_stream, producer) = ui::AnimationStream::channel(8, 4096);
		let input = Input::new(producer, 4096);
		let bytes = fixture();
		// Withhold the trailer: a whole-body buffering decoder would block instead of
		// presenting its first complete frame. The timeout is only a deadlock guard.
		assert!(input.append(&bytes[..bytes.len() - 1]));
		let (send, receive) = std::sync::mpsc::sync_channel(1);
		let thread = std::thread::spawn(move || {
			let budget = FrameBudget {
				fit: 8,
				bytes: 4096,
				count: 2,
				shrinks: 0,
			};
			cycle(&input, &budget, |at, delay, _| {
				let _ = send.send((at, delay));
				false
			})
		});
		assert_eq!(
			receive.recv_timeout(Duration::from_secs(5)).unwrap(),
			(Duration::ZERO, Duration::from_millis(50))
		);
		assert!(thread.join().unwrap().is_none());
	}

	#[tokio::test]
	async fn network_load_delivers_playback_before_download_completion_and_caches_the_source() {
		let _gate = LOADS.lock().await;
		use tokio::io::{AsyncReadExt, AsyncWriteExt};
		#[cfg(target_os = "windows")]
		let video = {
			let mut bytes = include_bytes!("../../tests/fixtures/video.mov").to_vec();
			// A valid inert box makes the source larger than native read-ahead. Withholding
			// the last byte of a tiny movie would just block that initial read-ahead buffer.
			let padding = 4 * 1024 * 1024u32;
			bytes.extend_from_slice(&padding.to_be_bytes());
			bytes.extend_from_slice(b"free");
			bytes.resize(bytes.len() + padding as usize - 8, 0);
			bytes
		};
		let cases = [
			("gif", fixture(), 8, 4096),
			#[cfg(target_os = "windows")]
			("mp4", video, 128, 8 * 1024 * 1024),
		];
		for (extension, bytes, fit, frame_bytes) in cases {
			let length = bytes.len();
			let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
			let url = format!(
				"http://{}/original.{extension}",
				listener.local_addr().unwrap()
			);
			let (release, released) = tokio::sync::oneshot::channel();
			let server = tokio::spawn(async move {
				let (mut socket, _) = listener.accept().await.unwrap();
				let mut request = [0; 4096];
				let _ = socket.read(&mut request).await.unwrap();
				socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
				socket.write_all(&bytes[..length - 1]).await.unwrap();
				released.await.unwrap();
				socket.write_all(&bytes[length - 1..]).await.unwrap();
			});
			let (sender, mut results) = super::super::result_channel(egui::Context::default());
			let budget = super::super::Budget {
				fit,
				encoded: MAX_ENCODED,
				canvas: super::ANIMATION_CANVAS,
				alloc: super::ANIMATION_ALLOC,
				frames: Some(FrameBudget {
					fit,
					bytes: frame_bytes,
					count: 2,
					shrinks: 0,
				}),
			};
			let client = wreq::Client::builder().no_proxy().build().unwrap();
			let loading = tokio::spawn(super::super::load_stream(
				format!("media:ia:e{fit}:https://media.tenor.com/synthetic/original.{extension}"),
				url,
				budget,
				None,
				None,
				Some(client),
				Instant::now(),
				sender,
			));
			let first = tokio::time::timeout(Duration::from_secs(5), results.recv())
				.await
				.unwrap()
				.unwrap();
			assert_eq!(first.stage, super::super::DecodeStage::Streaming);
			assert!(first.image.is_some() && first.stream.is_some());
			assert!(
				!loading.is_finished(),
				"playback must precede the withheld response trailer"
			);
			release.send(()).unwrap();
			let loaded = tokio::time::timeout(Duration::from_secs(5), loading)
				.await
				.unwrap()
				.unwrap();
			assert!(loaded.streamed && loaded.image.is_none() && loaded.frames.is_empty());
			assert_eq!(loaded.fetched.unwrap().len(), length);
			server.await.unwrap();
		}
	}

	#[tokio::test]
	async fn decoder_pressure_defers_instead_of_blocking_other_media_jobs() {
		let _gate = LOADS.lock().await;
		let _permits = SLOTS.clone().acquire_many_owned(8).await.unwrap();
		let (sender, _) = super::super::result_channel(egui::Context::default());
		let budget = super::super::Budget {
			fit: 8,
			encoded: MAX_ENCODED,
			canvas: 8,
			alloc: 4096,
			frames: Some(FrameBudget {
				fit: 8,
				bytes: 4096,
				count: 2,
				shrinks: 0,
			}),
		};
		let loaded = tokio::time::timeout(
			Duration::from_secs(5),
			super::super::load_stream(
				"media:ia:e8:https://media1.tenor.com/synthetic/original.gif".into(),
				"https://media1.tenor.com/synthetic/original.gif".into(),
				budget,
				Some(fixture()),
				None,
				None,
				Instant::now(),
				sender,
			),
		)
		.await
		.unwrap();
		assert!(loaded.retry && !loaded.streamed && loaded.image.is_none());
	}

	#[test]
	#[ignore = "set SEREIN_ORIGINAL_GIF to a local original GIF for full-cadence verification"]
	fn local_original_gif_workload() {
		let bytes = std::fs::read(std::env::var_os("SEREIN_ORIGINAL_GIF").expect("local GIF path"))
			.unwrap();
		for (name, fit, limit) in [("inline", 368, 40), ("viewer", 720, 96)] {
			let (stream, producer) = ui::AnimationStream::channel(fit, limit * 1024 * 1024);
			let input = Input::new(producer, limit * 1024 * 1024);
			assert!(input.cached(bytes.clone()));
			let budget = FrameBudget {
				fit,
				bytes: limit * 1024 * 1024,
				count: 240,
				shrinks: 0,
			};
			let started = Instant::now();
			let mut first = None;
			let mut largest_gap = Duration::ZERO;
			let (frames, duration) = cycle(&input, &budget, |_, delay, _| {
				first.get_or_insert_with(|| started.elapsed());
				largest_gap = largest_gap.max(delay);
				let _ = stream.frame(); // Mark synthetic consumer activity; no sleeps or rendering.
				true
			})
			.expect("complete original loop");
			eprintln!(
				"{name}: {frames} frames, {duration:?}, {:.2} fps, maximum hold {largest_gap:?}, first {first:?}, full decode {:?}, reserved {} bytes",
				frames as f64 / duration.as_secs_f64(),
				started.elapsed(),
				stream.reserved_bytes()
			);
			assert!(stream.reserved_bytes() <= budget.bytes);
		}
	}
}
