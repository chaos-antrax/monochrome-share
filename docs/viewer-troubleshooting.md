# Viewer troubleshooting

The temporary connection-report panel has been removed following user-confirmed video/audio playback on Android and iOS. Normal stream information remains available from the floating dock's information icon.

## If a stream does not start

- Use a fresh link from the running host and disconnect other viewers first.
- Allow up to 45 seconds for startup. A failed attempt now shows a connection, format, or media error instead of waiting indefinitely.
- If asked to tap again, do so to allow playback with sound. A usable connection is preserved for this second gesture.
- For a video-format error, update the host app and browser. The host encodes constrained-baseline H.264 at 1080p60 rather than Main; it does not simply rewrite SDP.
- Check the desktop app's existing host/tunnel diagnostics for host-side failures. For public links, check [remote-sharing setup](remote-sharing.md).
- The floating stream information shows received resolution, frame rate, bitrate, RTT, and Direct/Relay route. RTT is not glass-to-glass latency.

No viewer report, browser fingerprint, candidate history, or verbose event timeline is collected or exported.

## Regression checks

```powershell
node scripts/test-viewer-connection.mjs
. .\scripts\Enter-DevEnvironment.ps1
cargo test --features media --locked
# Optional GPU check: synthetic black frames only, no desktop capture.
cargo test --features media --locked nvenc_outputs_real_constrained_baseline_and_matching_rtp_caps -- --ignored --nocapture
cargo build --release --features media --bins --locked
$env:SCREEN_SHARE_TEST_HOST = "$PWD\target\release\monochrome-capture.exe"
node scripts/test-server.mjs
node scripts/test-viewer.mjs
```

Tests retain BUNDLE-only audio/video acceptance, genuine video rejection, Safari statistics compatibility, startup timeouts, safe connection errors, normal statistics, playback recovery, and the actual NVENC profile check. Automated desktop/synthetic checks do not replace physical device acceptance.
