# Full gifv video frame rate

Correction to `0260a26ce1a81ee0f5dd7860329a69d214758343`, on `devel`, November 15, 2026.
The owner requested local commits, no push and no release/package build. The sticker fix remains
separate and is unchanged. No authenticated Discord session, account messages or audio devices
were used. Public provider files remain in ignored `target/media-regression/` output.

The previous fix selected the linked 20 FPS GIF ahead of the embed's 30 FPS MP4. Its 20 FPS
measurements were accurate for that file but did not establish the desired source or prove
official-client parity. [Tenor provides multiple media formats](https://developers.google.com/tenor/guides/response-objects-and-errors);
their frame counts and timings must not be assumed identical.

| Source | Encoded bytes | Frames / duration | Source frame rate |
| --- | ---: | ---: | ---: |
| [Linked GIF](https://media1.tenor.com/m/rfhdy0TtiAoAAAAC/horse-derp.gif) | 34,766,782 | 361 / 18.05 s | 20 FPS |
| [Embed MP4 rendition](https://media.tenor.com/rfhdy0TtiAoAAAPo/horse-derp.mp4) | 3,284,217 | 599 / 19.967 s | approximately 30 FPS |

Current playback prefers the embed video in both chat and viewer. A bounded worker decodes
every frame on demand using exact presentation timestamps, including sub-20-ms intervals.
It retains a latest frame rather than thinning a whole RGBA loop to fit the cached-frame count.
The decoder seeks to the beginning during the final frame's hold. GIF/poster fallbacks remain.

## Measurements and reproduction

Pinned Rust 1.99.1, locked dependencies, debug/test profiles, debug information and incremental
compilation disabled. Windows 10 IoT Enterprise LTSC 19044, Ryzen 5 4600H, approximately 15.4 GiB
RAM and RTX 2060; native rendering uses WGPU. The decoder measurements below are one run per
size, without sleeping or network I/O. They measure preparation work, not display FPS.

| Full-source decode | Inline, fit 368 | Viewer, fit 720 |
| --- | ---: | ---: |
| Frames decoded / loop | 599 / 19.967 s | 599 / 19.967 s |
| Largest timestamp gap | 33.3334 ms | 33.3334 ms |
| First decoded frame | 45.5865 ms | 36.1632 ms |
| Total decoding work | 984.5323 ms | 1,808.1807 ms |
| Reserved stream bytes | 6,026,553 | 11,578,617 |
| Per-stream byte ceiling | 41,943,040 | 100,663,296 |

The first native presentation sample of that MP4 delivered 600 changed textures in 20.0010
seconds (29.998 FPS), with zero skipped revisions and a longest gap of 35.225 ms, after two
seconds of warmup. The 20-second sample crosses a complete loop boundary. The original cadence
example's historical stdout prefix said `original GIF native`; `SEREIN_MEDIA_CLIP` selected the
MP4 and header sniffing selected the video decoder. The label has been corrected to `local media`.
A repeat on the final source presented 600 revisions in 20.0006 seconds (29.999 FPS), again with
zero skipped revisions; its longest gap was 34.830 ms. Both samples exclude concurrent Cargo work.

```powershell
$env:CARGO_BUILD_JOBS='2'
$env:CARGO_PROFILE_DEV_DEBUG='0'
$env:CARGO_PROFILE_TEST_DEBUG='0'
$env:CARGO_INCREMENTAL='0'
$env:RUST_TEST_THREADS='1'
$env:SEREIN_MEDIA_CLIP='D:/Serein/target/media-regression/horse-derp.mp4'
cargo test --locked -p serein local_video_stream_workload -- --ignored --nocapture
cargo run --locked -p serein --features demo --example gif_stream -- --demo
cargo test --locked -p serein avatars::streaming:: -- --test-threads=8
cargo test --locked -p ui --lib avatars::
```

The standalone example starts no account/network/audio adapters. Native counters measure texture
presentation, not GPU scanout or a live official-client comparison. Still screenshots cannot
establish frame rate; no before/after stills are claimed for this motion-only correction. Previous
20 FPS measurements are retained and explicitly superseded in the original evidence.

Regression coverage exercises video preference and GIF/poster fallback, every frame of a native
24 FPS fixture despite a two-frame cache budget, native rewind, 60 FPS timestamp precision,
nonzero timestamp origins, rejection of nonfinite/backwards/out-of-range times, cancellation,
and initial loading leases that cannot depend on a UI refresh before first delivery. The local
HTTP test withholds the last byte while requiring first-frame delivery. Its native-video fixture
has a valid inert `free` box so its body exceeds native read-ahead; a tiny movie otherwise fits
entirely in that first read. Five-second test timeouts are deadlock guards, not performance asserts.

Eight stream workers share the existing quota and remain independent of short artwork decodes.
Video input stays capped at 16 MiB; original GIF input retains its separate 40 MiB allowance.
Encoded input and modeled frame working sets share existing 40/96 MiB per-stream and 128 MiB
pool limits; native backend/driver allocations and bounded download bodies for cache writes are
additional. No whole-process RSS, CPU utilization, network latency, release/package size, GPU
scanout or authenticated official-client comparison was measured.

## Verification

Formatting, workspace/all-target Clippy, example Clippy, all 240 desktop tests, all 34 focused
UI media tests, the seven streaming regressions with eight test threads, the no-default-features
check, policy checks and ordinary debug desktop/example builds passed. Ignored local workloads
were run explicitly for the provider asset and are separate from normal synthetic regression tests.

`cargo xtask check` passed formatting and Clippy, then stopped on the same pre-existing
`typing::tests::bounded_flood_refresh_and_out_of_order_signals_have_fixed_storage` assertion
(16 entries versus 8). No typing or voice code changed. Previously recorded synthetic voice
timeouts were not re-run for this correction; the full workspace is not claimed green.
Release and package checks were omitted at the owner's request. Nothing was pushed.
