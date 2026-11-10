//! Native animation cadence measurement. No account, network or audio adapters.
#[allow(dead_code)]
#[path = "../src/avatars.rs"]
mod avatars;
use eframe::egui;
use std::io::Read;
use std::time::{Duration, Instant};

struct Demo {
	stream: ui::AnimationStream,
	ready: tokio::sync::oneshot::Receiver<egui::ColorImage>,
	texture: Option<egui::TextureHandle>,
	started: Instant,
	measured: Option<Instant>,
	sequence: u64,
	presented: u64,
	skipped: u64,
	finished: bool,
	original: bool,
	last_frame: Option<Instant>,
	longest_gap: Duration,
}
impl eframe::App for Demo {
	fn persist_egui_memory(&self) -> bool {
		false
	}
	fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
		if self.finished {
			ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
			return;
		}
		let _ = self.ready.try_recv();
		if let Some((sequence, image)) = self.stream.frame()
			&& sequence != self.sequence
		{
			if self.measured.is_some() {
				self.presented += 1;
				self.skipped += sequence.saturating_sub(self.sequence + 1);
				let now = Instant::now();
				if let Some(last) = self.last_frame.replace(now) {
					self.longest_gap = self.longest_gap.max(now - last);
				}
			}
			self.sequence = sequence;
			if let Some(texture) = &mut self.texture {
				texture.set(image, egui::TextureOptions::LINEAR);
			} else {
				self.texture = Some(ui.ctx().load_texture(
					"media-stream",
					image,
					egui::TextureOptions::LINEAR,
				));
			}
		}
		if self.measured.is_none() && self.started.elapsed() >= Duration::from_secs(2) {
			self.measured = Some(Instant::now());
			self.presented = 0;
			self.skipped = 0;
		}
		ui.heading("Provider media cadence (offline)");
		ui.label(if self.original {
			"Local GIF/video rendition, fitted to the inline playback size."
		} else {
			"40 synthetic authored frames at 20 fps, looping every 2 seconds."
		});
		ui.label("Native texture presentation after a two-second warmup; no network or audio.");
		if let Some(texture) = &self.texture {
			ui.add(egui::Image::new(texture).max_size(egui::Vec2::splat(240.0)));
		}
		if let Some(measured) = self.measured {
			let elapsed = measured.elapsed().as_secs_f64();
			ui.label(format!(
				"Presented: {}   Skipped: {}   {:.2} frames/s",
				self.presented,
				self.skipped,
				self.presented as f64 / elapsed.max(0.001)
			));
			if elapsed >= if self.original { 20.0 } else { 5.0 } {
				self.finished = true;
				println!(
					"{} media native: presented={} skipped={} seconds={elapsed:.4} fps={:.3} longest_gap_ms={:.3} reserved_bytes={}",
					if self.original { "local" } else { "synthetic" },
					self.presented,
					self.skipped,
					self.presented as f64 / elapsed,
					self.longest_gap.as_secs_f64() * 1000.0,
					self.stream.reserved_bytes()
				);
				ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
			}
		}
		ui.ctx().request_repaint_after(Duration::from_millis(100));
	}
}
fn main() -> eframe::Result {
	if !std::env::args().any(|arg| arg == "--demo") {
		eprintln!("gif_stream requires --demo; no application startup attempted");
		std::process::exit(2);
	}
	let original = std::env::var_os("SEREIN_MEDIA_CLIP")
		.or_else(|| std::env::var_os("SEREIN_ORIGINAL_GIF"))
		.map(|path| {
			let mut bytes = Vec::new();
			std::fs::File::open(path)
				.expect("local original GIF")
				.take(40 * 1024 * 1024 + 1)
				.read_to_end(&mut bytes)
				.expect("bounded local GIF read");
			assert!(
				bytes.len() <= 40 * 1024 * 1024,
				"local GIF exceeds input limit"
			);
			bytes
		});
	let original_source = original.is_some();
	eframe::run_native(
		"Serein media cadence (offline)",
		eframe::NativeOptions {
			viewport: egui::ViewportBuilder::default().with_inner_size([560.0, 420.0]),
			renderer: eframe::Renderer::Wgpu,
			persist_window: false,
			..Default::default()
		},
		Box::new(move |cc| {
			let (stream, ready) = avatars::streaming::demo_stream(cc.egui_ctx.clone(), original);
			Ok(Box::new(Demo {
				stream,
				ready,
				texture: None,
				started: Instant::now(),
				measured: None,
				sequence: 0,
				presented: 0,
				skipped: 0,
				finished: false,
				original: original_source,
				last_frame: None,
				longest_gap: Duration::ZERO,
			}))
		}),
	)
}
