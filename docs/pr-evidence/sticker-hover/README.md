# Sticker hover regression evidence

Baseline: `0ec7a773`, which includes both reported regression commits. The owner's subsequent
`6cab6b0a` changes release LTO only; these checks use debug/test profiles. No release build,
package, push or live Discord session was used.

## Native picker captures

Both images use the same modified `profile_preview` fixture, copied into the disposable baseline
worktree without changing its production rendering code. The fixture injects original checkerboard
frame sequences at the same post-decode boundary as animated stickers. It exercises the shared
sticker playback cache, not Lottie rasterization or service compatibility. The same sticker appears
in Recently Used, its server group and the hovered footer.

Build with pinned Rust 1.99.1 and the lockfile:

```powershell
$env:CARGO_BUILD_JOBS = '2'
$env:CARGO_PROFILE_DEV_DEBUG = '0'
$env:CARGO_PROFILE_TEST_DEBUG = '0'
$env:CARGO_INCREMENTAL = '0'
cargo build --locked -p serein --features demo --example profile_preview
target/debug/examples/profile_preview.exe --demo --page=stickers --animated-stickers --sticker-hover=540,425 --output=after.png
```

The screenshots are exported by the native egui/WGPU framebuffer callback and visually inspected.
Both use dark appearance, 1120×760 logical/physical pixels, 100% scale and the same injected pointer
position. `before.png` shows the hovered sticker blurred in both grid locations; `after.png` keeps
both grid copies sharp while the smaller footer animates independently. A still image does not
measure motion or frame latency; the texture regression test verifies independent advancing frames.

## CPU preparation workload

```powershell
cargo test --locked -p ui --lib sticker_hover_workload -- --ignored --nocapture
```

The same workload was copied into the baseline test module. Its upload deadline reset accesses
the original field there and each independent playback field after the fix. Thirty synthetic
160×120 animations each have sixteen 20-ms frames. Each simulated frame paints a 96-point grid
and one 32-point hovered footer; source frames advance every third UI tick. Deadlines are advanced
explicitly without sleeps. Each sample has 160 ticks; sample 0 warms the fitted caches and samples
1–5 are measured. This measures CPU preparation and texture deltas without a native window/GPU,
not rendered frame latency or actual Lottie decode performance. All thirty animations remain held.

The new simultaneous-size regression test was also copied to the baseline. It fails there because
grid and footer return the same texture ID, and passes with independent playback textures.

The initial build exhausted the Windows paging-file allowance. Checks use reduced debug information
and one or two build jobs; runtime media optimization settings are unchanged. Release executable,
package size, live Discord loading, GPU frame latency, native idle CPU and process RSS were not
measured. See `docs/performance.md` and `measurements.json` for the recorded comparisons.

## Validation - November 13, 2026

Validation ran against the combined local fixes. Formatting, workspace Clippy with warnings
denied, the no-default-features check and repository policy check passed. All 508 UI unit tests
passed (5 ignored), including independent playback textures, cache reuse and byte/item bounds.
The new texture regression fails on the baseline and passes after the fix.

The full workspace gate is not green because two unchanged tests fail on baseline `0ec7a773` too:

- `typing::tests::bounded_flood_refresh_and_out_of_order_signals_have_fixed_storage`: expected 8 entries, received 16.
- `transport::test_stream::established_streams_bound_missing_rekey_execution_and_welcome`: exceeds its 45-second test timeout.

Both baseline failures were reproduced separately. Their code is unchanged by these fixes.
Other workspace suites passed; the final desktop suite was rerun after fixing timestamp sampling.
