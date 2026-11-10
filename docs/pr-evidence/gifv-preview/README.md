# Windows gifv preview regression evidence

Baseline: `0ec7a773`, which includes both reported regression commits. The owner's subsequent
`6cab6b0a` changes release LTO only; these checks use debug/test profiles. No release build,
package, push or live Discord session was used.

## Local Tenor reproduction

The reported GIF's associated 3,284,217-byte MP4 was downloaded once into ignored local test output:

```powershell
curl.exe --fail --location --max-time 40 --max-filesize 16777216 --output target/media-regression/horse-derp.mp4 https://media.tenor.com/rfhdy0TtiAoAAAPo/horse-derp.mp4
$env:SEREIN_MEDIA_CLIP = 'D:/Serein/target/media-regression/horse-derp.mp4'
cargo test --locked -p serein local_clip_decode_workload -- --ignored --nocapture
```

This explicit local workload exercises full inline and viewer decoding, preserving the clip's
approximately 19.967-second loop under the existing frame/byte limits. It performs no network I/O.
The downloaded third-party artwork is not committed. The ordinary regression suite uses only
existing synthetic MOV/GIF/APNG/Lottie fixtures.

Native MOV regression coverage exercises normal hardware-first playback and scaled software-first
previews, including audio, seeking and frame validity. The long-clip test also checks a synthetic MOV
with a nonzero initial composition timestamp. Preview sampling normalizes timestamps to the first
decoded frame to preserve loop duration and avoid unnecessary frame-budget compaction. Successful
Windows decoding is required instead of silently passing a failed decode.

The initial build exhausted the Windows paging-file allowance. Checks use reduced debug information
and one or two build jobs; runtime media optimization settings are unchanged. Release executable,
package size, live Discord loading, GPU frame latency, native idle CPU and process RSS were not
measured. See `docs/performance.md` and `measurements.json` for the recorded comparisons.

## Validation - November 13, 2026

Validation ran against the combined local fixes. Formatting, workspace Clippy with warnings
denied, the no-default-features check and repository policy check passed. All 233 desktop tests
passed (5 ignored), including the long-clip regression. Native MOV playback/scaled-preview
coverage and the explicit local Tenor workload passed.

The full workspace gate is not green because two unchanged tests fail on baseline `0ec7a773` too:

- `typing::tests::bounded_flood_refresh_and_out_of_order_signals_have_fixed_storage`: expected 8 entries, received 16.
- `transport::test_stream::established_streams_bound_missing_rekey_execution_and_welcome`: exceeds its 45-second test timeout.

Both baseline failures were reproduced separately. Their code is unchanged by these fixes.
Other workspace suites passed; the final desktop suite was rerun after fixing timestamp sampling.
