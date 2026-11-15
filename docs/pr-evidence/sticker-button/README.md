# Composer sticker button

The measurements below describe the original `devel` implementation. When cherry-picked
onto the newer `devel2` base, the atlas also retains main's stop-sharing glyph: the combined
113 cells need fifteen rows (512×960), and the regenerated PNG is 55,754 bytes. The Korean
catalog added on main also receives the new labels. Original size comparisons and locale
counts below are historical, not measurements of the integrated build.

The dedicated sticker button sits between GIF and emoji and opens the shared picker on
Stickers. Clicking another media button switches tabs; clicking the active button closes
the picker. Opening it preserves the draft and does not send an item. The tooltip and
accessible label are translated in all thirteen bundled locales.

## Baseline and reproduction

Baseline `9739a9ca` has the same source tree as `08afff426ee8b21171d79465e77fe0e3cb4cc146`.
The preceding history cleanup combined the published sticker and GIF commits with their
respective local follow-ups; unrelated replayed commits have identical stable patch IDs.
The baseline working tree was clean. Work remains local on `devel`, as requested.

On Windows, build with pinned Rust 1.99.1 and locked dependencies:

```powershell
$env:CARGO_BUILD_JOBS='2'
$env:CARGO_PROFILE_DEV_DEBUG='0'
$env:CARGO_PROFILE_TEST_DEBUG='0'
$env:CARGO_INCREMENTAL='0'
$env:RUST_TEST_THREADS='1'
cargo build --locked -p serein --features demo
target/debug/serein.exe --demo --demo-stickers
```

In the synthetic OFFLINE PREVIEW, open Stickers using the new composer button, switch
between GIF, emoji and stickers, then press the active button again to close. Repeat with
a typed draft. Sticker selection continues through the existing picker/send flow.

Native before/after screenshots were not captured: window capture timed out, then the
owner pressed Escape to stop desktop automation. No further desktop input was attempted.
The owner closed the preview, and its verified remaining `--demo --demo-stickers` process
was stopped to release the debug executable for rebuilding. Native visual acceptance,
including narrow layouts and light/dark appearance, remains unmeasured. No authenticated
Discord session, message, call or microphone was used.

## Verification

- `cargo test --locked -p ui --lib`: 513 passed, 5 ignored, no failures.
- The new AccessKit/pointer regression locates buttons by accessible label and checks
  switching from both emoji and GIF, preserving the popup during mouse-down, opening
  Stickers, toggling closed, retaining the draft and producing no selected item.
- `cargo xtask check`: formatting and workspace/all-target Clippy passed. Workspace tests
  stopped at the previously reproduced unrelated client-core failure
  `typing::tests::bounded_flood_refresh_and_out_of_order_signals_have_fixed_storage`
  (`typing.rs:517`, expected 8, actual 16; 227 other client-core tests passed).
- `cargo check --locked -p serein --no-default-features`, `cargo xtask policy` and the
  debug demo build passed. The vendored egui deprecation warning remains.
- Icon regeneration verified pinned upstream hashes and reproduced identical decoded
  atlas pixels and a byte-identical canonical LF index in an isolated output tree.

## Resource comparison

One baseline and one changed debug build used the same toolchain, feature flags and
profile settings above. These are artifact sizes, not runtime performance measurements.

| Metric | Baseline | After | Delta |
| --- | ---: | ---: | ---: |
| Debug demo executable bytes | 165,783,552 | 165,841,408 | +57,856 (+0.035%) |
| Encoded icon atlas bytes | 27,327 | 54,792 | +27,465 (+100.5%) |
| Atlas dimensions | 512 × 896 | 512 × 896 | Unchanged |
| Populated atlas cells | 111 | 112 | +1 |

The atlas was regenerated with resvg 0.45.1 and losslessly recompressed with zlib level 9;
the baseline used oxipng. Encoded-size growth includes this compression difference.
The texture dimensions are unchanged. The button uses the existing picker and texture
atlas, adds no runtime dependency, and does not alter animation decoding, caching or
playback. Native idle CPU, process RSS, frame latency and startup time were not measured.
Release/package builds and publishing were excluded by the owner's instructions.
