//! A single-frame handoff for long animations. Decoding and scheduling belong to the desktop.
use egui::ColorImage;
use std::{
	sync::{
		Arc, Condvar, Mutex, Weak,
		atomic::{AtomicUsize, Ordering},
	},
	time::{Duration, Instant},
};

const VISIBLE: Duration = Duration::from_millis(500);

struct Frame {
	image: Option<Arc<ColorImage>>,
	sequence: u64,
	playing: bool,
	touched: Instant,
	closed: Option<bool>,
}

struct Shared {
	edge: u32,
	bytes: AtomicUsize,
	frame: Mutex<Frame>,
	wake: Condvar,
}

/// The UI owns this lease; dropping it cancels its decoder. Only the latest frame is retained.
pub struct AnimationStream(Arc<Shared>);

/// A weak producer cannot keep a discarded UI animation alive.
#[derive(Clone)]
pub struct AnimationProducer(Weak<Shared>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamActivity {
	Playing,
	Paused,
}

impl AnimationStream {
	/// `bytes` reserves the encoded source, decoder canvases, handoff and playback texture.
	pub fn channel(edge: u32, bytes: usize) -> (Self, AnimationProducer) {
		let shared = Arc::new(Shared {
			edge,
			bytes: AtomicUsize::new(bytes),
			frame: Mutex::new(Frame {
				image: None,
				sequence: 0,
				playing: true,
				touched: Instant::now(),
				closed: None,
			}),
			wake: Condvar::new(),
		});
		let producer = AnimationProducer(Arc::downgrade(&shared));
		(Self(shared), producer)
	}
	pub fn reserved_bytes(&self) -> usize {
		self.0.bytes.load(Ordering::Acquire)
	}
	pub fn edge(&self) -> u32 {
		self.0.edge
	}
	/// Reading a visible frame refreshes playback; hidden surfaces never call this.
	pub fn frame(&self) -> Option<(u64, Arc<ColorImage>)> {
		let mut frame = self.0.frame.lock().ok()?;
		let resume = !frame.playing || frame.touched.elapsed() >= VISIBLE;
		frame.playing = true;
		frame.touched = Instant::now();
		if resume {
			self.0.wake.notify_all();
		}
		Some((frame.sequence, frame.image.clone()?))
	}
	pub fn pause(&self) {
		if let Ok(mut frame) = self.0.frame.lock() {
			frame.playing = false;
		}
		self.0.wake.notify_all();
	}
	/// `Some(true)` means decode failure; `Some(false)` means idle expiry/cancellation.
	pub fn closed(&self) -> Option<bool> {
		self.0.frame.lock().ok()?.closed
	}
}

impl Drop for AnimationStream {
	fn drop(&mut self) {
		self.0.wake.notify_all();
	}
}

impl AnimationProducer {
	/// Header admission can tighten the original reservation before the first frame is sent.
	pub fn constrain(&self, bytes: usize) -> bool {
		let Some(shared) = self.0.upgrade() else {
			return false;
		};
		let Ok(frame) = shared.frame.lock() else {
			return false;
		};
		if bytes == 0
			|| frame
				.image
				.as_ref()
				.is_some_and(|image| image.pixels().len().saturating_mul(4) > bytes / 4)
		{
			return false;
		}
		bytes <= shared.bytes.fetch_min(bytes, Ordering::AcqRel)
	}
	pub fn activity(&self, now: Instant) -> Option<StreamActivity> {
		let shared = self.0.upgrade()?;
		if Arc::strong_count(&shared) == 1 {
			return None;
		}
		let frame = shared.frame.lock().ok()?;
		let idle = now.saturating_duration_since(frame.touched);
		// Before first delivery the loader owns the lease, so no visible UI can refresh it.
		// Network deadlines and dropping that loader still cancel a pending first frame.
		if frame.closed.is_some() || (frame.image.is_some() && idle >= super::IDLE_FRAMES) {
			None
		} else if frame.playing && (frame.image.is_none() || idle < VISIBLE) {
			Some(StreamActivity::Playing)
		} else {
			Some(StreamActivity::Paused)
		}
	}
	pub fn publish(&self, image: Arc<ColorImage>) -> bool {
		let Some(shared) = self.0.upgrade() else {
			return false;
		};
		let [width, height] = image.resolution();
		if width == 0
			|| height == 0
			|| width.max(height) > shared.edge as usize
			|| image.pixels().len() != width * height
			|| image.pixels().len().saturating_mul(4) > shared.bytes.load(Ordering::Acquire) / 4
		{
			return false;
		}
		let Ok(mut frame) = shared.frame.lock() else {
			return false;
		};
		if frame.closed.is_some()
			|| frame
				.image
				.as_ref()
				.is_some_and(|old| old.resolution() != image.resolution())
		{
			return false;
		}
		if frame.image.is_none() {
			// First-frame handoff gets its own visibility grace after network/decoder startup.
			frame.touched = Instant::now();
		}
		frame.sequence = frame.sequence.wrapping_add(1);
		frame.image = Some(image);
		true
	}
	pub fn finish(&self, failed: bool) {
		if let Some(shared) = self.0.upgrade()
			&& let Ok(mut frame) = shared.frame.lock()
		{
			frame.closed = Some(failed);
			shared.wake.notify_all();
		}
	}
	/// Decoder-side cancellable wait; never called by rendering or audio callbacks.
	pub fn wait(&self, timeout: Duration) {
		if let Some(shared) = self.0.upgrade()
			&& let Ok(frame) = shared.frame.lock()
		{
			let _ = shared
				.wake
				.wait_timeout(frame, timeout.min(Duration::from_millis(100)));
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn latest_frame_is_bounded_and_dropping_the_consumer_cancels() {
		let (stream, producer) = AnimationStream::channel(16, 4096);
		let red = Arc::new(ColorImage::filled([8, 8], egui::Color32::RED));
		assert!(producer.publish(red.clone()));
		assert!(!producer.constrain(1));
		assert!(!producer.constrain(8192));
		assert_eq!(stream.reserved_bytes(), 4096);
		assert!(Arc::ptr_eq(&stream.frame().unwrap().1, &red));
		assert!(!producer.publish(Arc::new(ColorImage::filled([17, 8], egui::Color32::RED))));
		assert!(!producer.publish(Arc::new(ColorImage::filled([8, 9], egui::Color32::RED))));
		assert!(producer.publish(Arc::new(ColorImage::filled([8, 8], egui::Color32::BLUE))));
		assert_eq!(stream.frame().unwrap().0, 2);
		drop(stream);
		assert_eq!(producer.activity(Instant::now()), None);
		assert!(!producer.publish(red));
	}
	#[test]
	fn hidden_and_unfocused_streams_pause_then_expire_without_sleeping() {
		let (stream, producer) = AnimationStream::channel(16, 4096);
		let before = stream.0.frame.lock().unwrap().touched;
		assert_eq!(
			producer.activity(before + super::super::IDLE_FRAMES),
			Some(StreamActivity::Playing)
		);
		assert!(producer.publish(Arc::new(ColorImage::filled([8, 8], egui::Color32::RED))));
		let now = stream.0.frame.lock().unwrap().touched;
		assert_eq!(producer.activity(now), Some(StreamActivity::Playing));
		assert_eq!(
			producer.activity(now + VISIBLE),
			Some(StreamActivity::Paused)
		);
		assert_eq!(producer.activity(now + super::super::IDLE_FRAMES), None);
		stream.pause();
		assert_eq!(producer.activity(now), Some(StreamActivity::Paused));
		stream.frame();
		assert_eq!(
			producer.activity(Instant::now()),
			Some(StreamActivity::Playing)
		);
	}
}
