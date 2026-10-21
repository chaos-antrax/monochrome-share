# Validation record

Do not mark device-dependent acceptance criteria passed without measurements.

| Check | Status | Evidence / required measurement |
| --- | --- | --- |
| NVIDIA GPU and driver present | Confirmed | RTX 4070 Laptop GPU, driver 591.86 from nvidia-smi |
| Rust toolchain | Passed | Project-local Rust 1.99 MSVC; rustfmt and clippy installed |
| GStreamer runtime/development | Passed, user-reported | GStreamer 1.28.7 runtime/development; all required elements and pkg-config verified in supplied logs |
| Server/protocol build and tests | Passed | Unit tests, native-media clippy with warnings denied, release build, authenticated server smoke test, graceful managed-host shutdown |
| Media Rust API/type check | Passed | cargo check --features media with DOCS_RS=1 and separate target/api-check; excludes native discovery/linking |
| Native media build | Passed locally and user-reported | Release binaries and native-media clippy pass against GStreamer 1.28.7 |
| GPU-only 1080p60 spike | Pending | scripts/Test-Nvenc.ps1; inspect caps and GPU traces |
| Authentication and session cleanup | Passed locally | scripts/test-server.mjs: missing/wrong token → 401; authorized viewer-only sessions; admission released after disconnect |
| Concurrent one-viewer admission | Pending runtime check | Connect second browser while an actual media session is active |
| Android playback | Passed, user-reported | Multiple Android browsers; video and audio reported smooth at 60fps on LAN |
| Branded floating viewer controls | Passed local headless browser checks | Installed Edge: 1280x800, 390x844, 320x568, 844x390; logo/name, no header/footer bars, icon-only fullscreen, volume/mute, fullscreen labels, auto-hide/reveal and open statistics. Android/Safari media playback with the redesigned controls remains a device acceptance check |
| Inverted link-preview branding | Passed local automated checks; WhatsApp check pending | Separate 512x512 dark-on-white PNG and explicit Open Graph image metadata; original logo/favicon and visible app branding retained. scripts/Test-LinkPreview.ps1 verifies deterministic RGB inversion and original asset preservation. Unit/server/browser checks cover URL scheme, safe host handling, no token/query leakage, PNG delivery, and unchanged viewer branding. WhatsApp preview refresh remains a user check |
| iPad/iOS playback | Passed, user-reported after codec fix | User confirms video and audio now work on both Android and iOS after switching Main to constrained-baseline. Prior iPad Air 5 report showed video rejection. Per-browser/link-path coverage, measured latency, and long-session results were not separately supplied |
| Constrained-baseline GPU output | Passed synthetic hardware test | Opt-in nvenc_outputs_real_constrained_baseline_and_matching_rtp_caps: 60 synthetic black frames, D3D11/NVENC, encoded caps 1920x1080@60 constrained-baseline; RTP profile-level-id=42c02a, packetization-mode=1. No desktop capture/recording. Does not establish Safari playback, real-time pacing, or latency |
| Viewer startup safeguards and diagnostics removal | Passed local automated checks | scripts/test-viewer-connection.mjs passes BUNDLE-only audio/video vs rejection, Safari mediaType statistics, 45-second startup timeout/cleanup, safe negotiation errors, retries, normal statistics, blocked playback recovery, and stale-answer guards. Temporary panel, report export, browser fingerprint, verbose timeline, and diagnostics.js route removed. Headless Edge verifies absence and retained controls at four sizes; authenticated server tests verify old endpoint is 404. Native-media tests, release build, formatting, and Clippy pass. Clean-viewer physical-device regression remains recommended |
| Even 165Hz → 60Hz pacing | Pending | Scroll/text/video/game; frame-time measurements |
| Static refresh and keyframe handling | Pending | Static screen, resume, PLI/FIR, explicit keyframe |
| LAN/Tailscale latency | Pending | Film desktop + phone timer; target under 300ms |
| CPU/GPU/encoder overhead | Pending | Task Manager/PresentMon baseline and stream comparisons |
| Silence continuity and A/V sync | Pending implementation | Silence/resume + clap test, offset under ~80ms |
| Per-app include/exclude | Pending implementation | Process trees, unsupported OS, process restart |
| Default output-device change | Pending implementation | Change/unplug output during stream |
| Android lock/background recovery | Passed with reconnect tap, user-reported | Stream resumes smoothly after prolonged minimizing and screen lock; automatic recovery still to be measured |
| Native desktop/tray controls | Passed local automation | Packaged app: native controls, host/link startup, minimize/close to tray, Stop and Exit; clipboard copy/manual UX still need user check |
| Material-inspired desktop redesign | Checks passed locally | Dark palette contrast tests; neutral-surface color test; custom checkbox click/Space behavior, diagnostics expand/collapse, host startup, tray and shutdown automation. High-contrast and mixed-DPI manual checks remain pending |
| Browser-only remote connectivity | User reports working | Public-link workflow reported successful; GStreamer accepts converted UDP/TCP/TLS TURN URLs. Direct/Relay coverage, measured performance, and longer sessions remain pending |
| Local credential persistence | Automated store tests pass | Windows Credential Manager synthetic save/load/update/delete test; Remember opt-out and Forget controls implemented; real-token restart check remains a user acceptance check |
| Portable package and Windows installer | Built; bundled runtime checked locally | Per-user setup executable and ZIP, GStreamer/plugins/cloudflared/licenses; packaged server and desktop tests pass without system GStreamer on PATH. Fresh-machine install/uninstall and signing remain pending |
| Dark branded installer redesign | Welcome/options checks passed locally | scripts/Test-Installer.ps1: branding, folder/shortcut screen, checkbox toggle, Install caption, dark navigation colors, left Cancel/right Back+Install, equal widths/alignment/corner margins, subdued directory outline, centered editable path text and horizontal padding. Setup-only screenshots inspected. No installation performed. Progress/finish, upgrade/uninstall, high-contrast and mixed-DPI acceptance remain pending |
| Single-instance desktop | Passed locally | Mutex contention/release tests; scripts/Test-Desktop.ps1: second process exits, restores visible/tray-hidden first window, preserves host health and viewer link. Existing user instances must be exited before running this isolated test |
| Desktop pointer cursor | Passed isolated native-control tests | Enabled push buttons/checkboxes use IDC_HAND; disabled controls use IDC_ARROW; editable fields use IDC_IBEAM. Existing user app and stream were left untouched |
| Installer desktop shortcut selection | Passed actual isolated install/uninstall tests | scripts/Test-InstallerShortcut.ps1: checked creates a desktop .lnk targeting the test desktop.exe; unchecked creates none. Unique test AppIds and shortcut names protect the real installation; fixtures uninstalled and temporary shortcut removed. Full-package upgrade acceptance remains pending |
| Two-hour soak | Pending | Memory, frame times, audio drift, device behavior |
