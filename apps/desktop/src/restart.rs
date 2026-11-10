//! Restart Serein (Ctrl+R by default): the running app exits through its normal close path,
//! then `main` starts a replacement process once the window, workers and single-instance claim
//! are released. Nothing is killed; a close the user cancels never restarts.
use std::{ffi::OsString, time::Duration};

/// Marks the replacement process, so it waits for the exiting one's single-instance claim
/// instead of handing its launch to an app that is shutting down.
const RESTARTED: &str = "--restarted";
/// How long a replacement waits for the previous process to release the claim.
const CLAIM_WAIT: Duration = Duration::from_secs(10);

/// Claim the single instance, or `None` when another running Serein took this launch.
pub fn claim() -> Option<platform::single_instance::Instance> {
	let restarted = std::env::args_os().any(|arg| arg == RESTARTED);
	let deadline = std::time::Instant::now() + CLAIM_WAIT;
	loop {
		match platform::single_instance::claim() {
			platform::single_instance::Launch::Primary(instance) => return Some(instance),
			platform::single_instance::Launch::Forwarded
				if restarted && std::time::Instant::now() < deadline =>
			{
				std::thread::sleep(Duration::from_millis(100));
			}
			platform::single_instance::Launch::Forwarded => return None,
		}
	}
}

/// Start the replacement process with this launch's arguments.
pub fn relaunch() {
	let Some(program) = program() else {
		eprintln!("Serein could not locate its executable to restart.");
		return;
	};
	if std::process::Command::new(program)
		.args(arguments(std::env::args_os().skip(1)))
		.spawn()
		.is_err()
	{
		eprintln!("Serein could not start again after closing.");
	}
}

/// The executable to start. An AppImage's mount disappears with this process, so the image
/// itself is started instead of the binary inside it.
fn program() -> Option<std::path::PathBuf> {
	#[cfg(target_os = "linux")]
	if std::env::var_os("APPDIR").is_some()
		&& let Some(image) = std::env::var_os("APPIMAGE")
	{
		return Some(image.into());
	}
	std::env::current_exe().ok()
}

/// The previous launch's arguments, without starting minimized (a restart is explicit and
/// shows the window) and with exactly one restart marker.
fn arguments(previous: impl Iterator<Item = OsString>) -> Vec<OsString> {
	previous
		.filter(|arg| arg != "--start-minimized" && arg != RESTARTED)
		.chain(std::iter::once(RESTARTED.into()))
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	/// A restart keeps the launch options, shows the window and marks itself only once.
	#[test]
	fn relaunch_arguments_drop_minimized_start_and_mark_one_restart() {
		let previous = ["--start-minimized", "--restarted", "--profile=work"].map(OsString::from);
		assert_eq!(
			arguments(previous.into_iter()),
			["--profile=work", RESTARTED].map(OsString::from)
		);
	}
}
