# Original Tenor GIF cadence

**Source preference superseded November 15, 2026.** This workload correctly measured the 20 FPS
linked GIF, but that is lower than the embed MP4's 30 FPS. Current gifv playback prefers the
video and preserves every frame; see [the correction](../gifv-framerate/README.md).

Local follow-up to `f19193d9` on `devel`, preserving the separate media and sticker fixes.
The owner requested local commits, no push and no release build/package. Measurements use
pinned Rust 1.99.1, locked dependencies, debug/test profiles with debug information and
incremental compilation disabled. The host is Windows 10 IoT Enterprise LTSC 19044,
Ryzen 5 4600H, approximately 15.4 GiB RAM and RTX 2060.

The [reported original GIF](https://media1.tenor.com/m/rfhdy0TtiAoAAAAC/horse-derp.gif)
was downloaded from its public, credential-free HTTPS URL into ignored
`target/media-regression/horse-derp-original.gif`, with a 64 MiB download cap. It is not a runtime
fixture or committed asset. It contains 34,766,782 encoded bytes, 280×498 pixels and 361 frames,
each authored for 50 ms: 20 FPS and an 18.05-second loop. The previous MP4 rendition contains
599 frames over 19.967 seconds. Its different duration explains the speed difference; retaining
only 136 inline / 108 viewer frames under whole-loop pixel limits also reduced smoothness.

The new original-GIF path keeps bounded encoded input, three source canvases and six fitted-frame
equivalents under the existing per-animation byte limits, instead of caching a thinned RGBA loop.
The UI uploads only the latest changed frame. Decoding and scheduling happen on cancellable
workers; visible consumers refresh their leases during long holds and network buffering.

## Reproduction

Set these environment variables in PowerShell before debug commands:

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_PROFILE_DEV_DEBUG='0'
$env:CARGO_PROFILE_TEST_DEBUG='0'
$env:CARGO_INCREMENTAL='0'
$env:RUST_TEST_THREADS='1'
$env:SEREIN_ORIGINAL_GIF='D:/Serein/target/media-regression/horse-derp-original.gif'
cargo test --locked -p serein local_original_gif_workload -- --ignored --nocapture
cargo test --locked -p serein avatars::streaming::tests
cargo test --locked -p ui --lib avatars::
# Original asset: 2-second warmup, 20-second presentation sample (crosses a loop boundary).
cargo run --locked -p serein --features demo --example gif_stream -- --demo
# Generated fixture: 2-second warmup, 5-second presentation sample.
Remove-Item Env:SEREIN_ORIGINAL_GIF
cargo run --locked -p serein --features demo --example gif_stream -- --demo
```

The ignored local workload decodes every frame without sleeping or displaying it. It reports
authored timing, first decoded-frame latency, total decode work and reserved bytes. This is a
single measurement per size, excluding network transfer and actual display presentation.
The baseline values are from the unchanged MP4 workload recorded in
[the earlier evidence](../gifv-preview/measurements.json); they are not a new native benchmark.

The standalone native example generates an offline 40-frame GIF at 20 FPS, loops it through the
same stream scheduler and texture handoff, warms up for two seconds, then counts changed
textures and skipped revisions for five seconds. It starts no account, network or audio adapters.
These counters measure native texture presentation, not GPU scanout timing or the official client.
One native sample presented 100 revisions in 5.0032 seconds (19.987 FPS), with no skipped revisions
after the two-second warmup; its reserved stream working set was 238,533 bytes.
A still screenshot cannot establish motion cadence, so no before/after stills are claimed for this
follow-up. The original-GIF fixture and synthetic native workload are identified separately.
When `SEREIN_ORIGINAL_GIF` names a local file, the same example reads at most 40 MiB and uses
the inline original-GIF budget instead, measuring for 20 seconds after the same warmup.
The reported horse GIF presented 400 revisions over 20.0005 seconds (20.000 FPS rounded),
with zero skipped revisions and a longest measured gap of 51.569 ms. That sample crosses its
18.05-second loop boundary; the next loop's first frame is decoded during the final frame's
hold, before the authored boundary. Reserved stream bytes were 38,259,454.

Regression coverage includes first-frame delivery before HTTP body completion (a local server
withholds the trailer), full authored frame retention and rewind, encoded-byte limits, truncated
input, consumer cancellation, viewer closure, stable texture reuse, admission under both item and
byte pressure, retry backoff, decoder saturation without blocking other image jobs, and original
GIF → video → poster fallback. Five-second test timeouts are deadlock guards, not performance asserts.

At most eight stream decoders run. They do not consume the short artwork-decode permits.
An inline stream reserves at most 40 MiB, a viewer stream 96 MiB; shared pools remain 128 MiB
and the inline animation count remains 96. Network jobs may additionally hold one encoded body
of at most 40 MiB each for the existing disk cache. These are bounded working sets, not a
whole-process memory limit. CPU utilization, OS/driver allocations, network latency, release/package size
and a live authenticated Discord session were not measured.

The original is substantially larger than its MP4 rendition, so streamed requests allow up to
60 seconds for their bounded body instead of inheriting the ordinary image worker's 15-second
deadline. Startup still waits only for the first complete frame. Slow networks may buffer and
use more bandwidth than the MP4 fallback.

## Verification limits

`cargo xtask check` passed formatting and workspace/all-target Clippy, then stopped on the
previously recorded `typing::tests::bounded_flood_refresh_and_out_of_order_signals_have_fixed_storage`
failure (16 entries versus 8). The broader workspace run used `--no-fail-fast` and excluded that
test and the previously baseline-verified
`transport::test_stream::established_streams_bound_missing_rekey_execution_and_welcome` timeout.
All targets except `discord-voice --lib` passed, including 512 UI tests, 238 desktop tests and
the model URL-security tests. The voice target additionally timed out in
`transport::test_stream::local_stream_sender_and_viewer_deliver_audio_and_video`
at `test_stream.rs:102`; an isolated retry also timed out. No voice code changed in this follow-up;
this additional failure is reported rather than presented as a baseline-verified result.

The locked no-default-features check, policy check, ordinary debug desktop build, native example
build and example Clippy passed. Release and packaging checks were omitted at the owner's request.
Final workspace/all-target Clippy and formatting passed again after the scheduling adjustments.
All 238 desktop tests passed on the final source, and the five streaming regressions also passed
with eight test threads, including serialized access to the process-wide decoder quota.
