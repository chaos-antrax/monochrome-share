# Implementation analysis and initial slice

## Desktop and browser-only remote extension

The browser viewer uses the same logo/name/subtitle and neutral grayscale palette. The stage fills the viewport without header/footer bars. A floating dock contains icon mute, volume, statistics disclosure, and icon-only fullscreen. Inactivity fades the dock while playback is active; pointer and keyboard interaction reveals it, and focus/open statistics prevent hiding. The dock is inside the fullscreen stage. `scripts/test-viewer.mjs` uses installed headless Edge for responsive/layout/control checks and synthetic playback-state tests, not actual media acceptance.

Desktop branding is **Monochrome - Share** with the subtitle **1-to-1 Private screen sharing**. The supplied root `logo.png` is the source for seven Windows icon sizes (16-256px), embedded executable/version resources, the header logo, window/taskbar/tray icons, and the installer icon. `build.rs` invokes `scripts/Build-BrandAssets.ps1`; Windows builds require the SDK's `rc.exe`. Installer identity, installation directory, binary filenames, and the Credential Manager target remain unchanged for upgrade compatibility.

The desktop UI uses a dark Material-inspired two-column layout based on [VS Code Modern Dark's neutral colors](https://github.com/microsoft/vscode/blob/main/extensions/theme-defaults/themes/dark_modern.json): `#181818`/`#1F1F1F` surfaces, `#313131` fields, and `#CCCCCC` text. At the user's request, the upstream blue accent is replaced with entirely grayscale buttons, focus borders, and hover/pressed states. It has inset fields, custom-drawn rounded push buttons and checkboxes, and collapsible diagnostics. Styling lives in `src/desktop_style.rs`; standard Windows control classes preserve input behavior and accessibility text. GDI objects are released on exit, paint uses a backing bitmap, system-DPI scaling applies to layout/fonts, and high-contrast startup uses system colors/native controls. Supported Windows builds receive a matching dark title bar. Streaming, tray lifecycle, and credential persistence are unchanged by the redesign.

The user confirmed LAN playback with audio on multiple Android browsers, reporting 60fps and smooth recovery after a reconnect tap. This is user-reported device evidence, not a latency or overhead measurement.

The implementation uses a native Win32 desktop launcher, a managed capture-host child process, a Windows job object to clean up child processes on desktop exit/crash, and a system-tray menu. The desktop optionally stores provider credentials in current-user Windows Credential Manager with local-PC persistence, restores them at startup, and offers opt-out and Forget controls. No plaintext config file is used. Stop uses stdin shutdown and invalidates the displayed link; restart creates a fresh viewer token.

The user requested remote viewers use only their browser. This supersedes the original Tailscale-first deployment assumption: public-link mode uses Cloudflare Quick Tunnel for HTTPS/WebSocket signaling, plus Cloudflare Realtime TURN credentials generated server-side. Only temporary ICE credentials go to the admitted viewer. Both browser and GStreamer ICE agents receive relay configuration. Quick Tunnels provide a domain-free first test, not a guaranteed production endpoint.

The release package bundles the required GStreamer plugins and native dependency DLLs, cloudflared, and upstream licenses. A per-user Inno Setup installer provides shortcuts and uninstall support. Native build/lint/unit tests, server authentication/secret-handling/shutdown checks, TURN-URI compatibility, and desktop control/tray checks pass locally. Credential Manager roundtrip/update/delete tests use an isolated synthetic entry. The user reports successful public-link playback; fresh-machine install/uninstall, Direct/Relay coverage, iPad playback, latency/overhead measurements, and longer audio validation remain open.

## Decisions

Enabled desktop push buttons and checkboxes use the standard Windows hand cursor on hover, including native high-contrast controls. Disabled controls retain the arrow; editable fields retain their native I-beam. A cursor-only subclass is separate from custom painting and removed when each control is destroyed. Isolated native-control tests verify all three cursor states without interacting with an existing app instance.

Desktop launches are single-instance within the Windows interactive session. An owned `Local\\MonochromeCapture.Desktop.v1` kernel mutex is acquired before UI/host initialization and held for the app lifetime. Duplicate launches restore the existing window (including tray-hidden windows) without changing the active stream. Mutex ownership is released on normal exit; abandoned ownership can be recovered after a crash. A running older desktop build is also recognized by its window class, but older executables themselves cannot enforce the new restriction. Lock acquisition failure is reported instead of launching an unguarded instance.

Installer navigation has equal-width Back/Install buttons at the right, Cancel at the left, a 12-DIP gap within the right-hand group, and symmetric outer margins. The installation directory remains an editable native control inside a subtle gray one-pixel panel outline, replacing the thick native white border. Its text height is measured using the actual font at the current DPI, and the compact borderless edit is vertically centered inside a matching padded surface with 8-DIP horizontal insets. High-contrast/native fallback retains Windows styling.

The installer uses Inno Setup's built-in dark style with a charcoal background, custom high-DPI artwork derived from `logo.png`, and branded welcome/finish text. The flow is Welcome → Installation options (location and optional desktop shortcut together) → Progress → Finish with optional launch. The separate tasks, program-group, and confirmation pages are removed. The per-user AppId, default directory, runtime bundling, credential identity, and uninstall behavior remain unchanged. Build wizard images with `Build-BrandAssets.ps1 -InstallerImages`; `Build-Package.ps1` does this automatically. The style requires Inno Setup 6.6 or newer; native system dialogs and accessibility fallback can follow Windows colors.

The wizard's existing navigation buttons and progress gauge are painted in grayscale without replacing their native behavior. Buttons retain captions, click handling, tab navigation, default/cancel actions, and disabled state; focus receives a gray border. Hooks are removed on setup exit and are not used in high-contrast mode. The bundled Inno Setup 6 compiler produces a 32-bit setup program for the 64-bit app; these window-procedure hooks must be adapted before switching to a 64-bit setup compiler.

`scripts/Test-Installer.ps1` normally launches only its own setup process tree, checks welcome/options controls and shortcut toggling, saves previews, and exits before installation. The ordinary production preview must never click Install. `scripts/Test-InstallerShortcut.ps1` separately compiles small fixtures with unique test AppIds, workspace installation folders, unique temporary desktop shortcut names, and no app launch or Start menu shortcuts. Only verified fixtures can enable the probe's installation mode. Checked and unchecked installations verify actual shortcut existence and target; their uninstallers remove test files/shortcuts afterward. Full-package installation/upgrade, high-contrast behavior, and mixed-DPI acceptance remain separate checks. The built-in Slate style did not render correctly with the local compiler, so the built-in dark style is used with grayscale control painting instead.

Shortcut selection is synchronized in `ShouldSkipPage(wpSelectTasks)`, after Inno rebuilds its task list. Applying it earlier in the directory page's Next event discarded the checkbox choice. Silent installs retain Inno's native `/TASKS` and `/MERGETASKS` selection behavior.

Follow Phases 0 and 1 first. Use `webrtcbin` rather than a new Rust WebRTC stack so capture, codecs, packetization, SRTP, feedback, and clock handling remain in one GStreamer pipeline. Defer the `str0m` versus `webrtc-rs` decision to the optional native rewrite.

Keep the prototype host as a Rust crate with a `media` feature. The server/viewer can be built and tested without native dependencies, while an actual streaming launch requires the media feature and real plugin availability. No fake streaming or implicit software encoder fallback.

Use GStreamer 1.26+ and gstreamer-rs 0.25 / Axum 0.8. Check the resolved dependency versions in Cargo.lock. Runtime startup checks each required element. GPU-memory caps constrain capture and conversion before the D3D11-specific NVENC encoder. This establishes the intended memory path, but GPU device sharing and measured lack of readback still need hardware validation.

The host produces an offer; the viewer returns an answer; both trickle ICE. Remote candidates are queued until the remote description is set. One authenticated WebSocket owns one media pipeline and an admission permit. Cleanup returns the pipeline to NULL and releases admission. Reconnect creates a fresh session. A negotiation timeout prevents an unanswered socket from holding the host indefinitely.

Security starts in the first slice: random token unless explicitly configured, loopback binding by default, single-viewer admission, bounded signaling messages, rate-limited explicit keyframe requests, no request logging of secrets, no third-party viewer resources, and no storage of frames.

## Plan refinements

- Hardware confirmed during initial inspection: **RTX 4070 Laptop GPU**, driver **591.86**. Laptop power/thermal limits may affect overhead measurements relative to a desktop 4070.
- Hybrid laptops may place the primary monitor on an integrated GPU. `--adapter` selects the WGC D3D11 adapter; verify it matches the NVENC adapter. GPU-memory caps do not establish shared-device operation or rule out GPU-to-GPU copies.
- GStreamer `wasapi2src` documents process-tree include/exclude support from 1.22, with Windows build 20348 required. Most Windows 10 desktop installations do not meet that build requirement. Future CLI selection must test the actual build, resolve process trees, and report fallback explicitly.
- This first audio branch shares one GStreamer clock with video, but explicit silence injection, native QPC timestamps, endpoint change recovery, and restart-aware process selection remain Phase 3 work. Do not infer the 80ms sync target from using one pipeline.
- The source requests 60fps directly rather than capturing 165fps and dropping every Nth frame. Verify static refresh and even source pacing experimentally before considering Phase 2 complete.
- Constrained-baseline H.264, CABAC disabled, no B-frames/lookahead, low-latency CBR, repeated headers, five-second safety GOP, and upstream force-key-unit events are the current encoder settings. Main was replaced after the iPad report showed video rejection. Confirm Safari playback and actual PLI/FIR behavior on hardware.
- The viewer requests a 30ms jitter-buffer target only when supported. It is a browser hint, not a latency guarantee. Fullscreen and wake lock are optional capabilities; mobile OS limitations still apply.
- Do not deploy coturn, change firewall policy, install Tailscale, or add startup tasks just to compile the initial prototype. Those belong to explicit deployment and later validation.

## Next implementation work after the hardware spike

1. Validate the native media build and NVENC smoke pipeline with the required GStreamer installation.
2. Test H.264/Opus negotiation on both target browsers, including a static desktop.
3. Measure pacing, glass-to-glass latency, and foreground workload impact.
4. Implement and validate silence continuity and device-change recovery before per-app audio.
5. Add executable-name process selection, restart tracking, and explicit unsupported-build fallback.
6. Verify external network/Tailscale routing and two-hour stability before v1 completion.

## Viewer compatibility and startup safeguards

The temporary viewer diagnostics panel, copy-report tools, user-agent/capability collection, and verbose timeline have been removed after the user confirmed video/audio playback on Android and iOS. The desktop app's separate host/tunnel diagnostics are unchanged.

The iPad report showed Main-profile video (`4d402a`) rejected with no video codecs in the answer, while ICE/DTLS never connected. The encoder now uses constrained-baseline caps and disables CABAC; no SDP profile rewriting or resolution/frame-rate reduction is used. The shared encoder chain is exercised by an opt-in synthetic GPU test, which locally produced 1920x1080@60 constrained-baseline and RTP `profile-level-id=42c02a`, packetization mode 1.

Small shared helpers in `viewer/connection.js` retain BUNDLE-aware answer validation, current receiver-health statistics, and startup error classification without storing raw SDP, browser fingerprints, or report history. A zero-port `bundle-only` section inside a BUNDLE group is not treated as rejected. A genuinely rejected/disabled video answer or one with no codec stops immediately with an actionable error.

Startup keeps a 45-second deadline per attempt, including after an SDP answer. Audio-only playback does not cancel the video deadline. A blocked session with received video can remain connected for a second user gesture; other startup timeouts stop the attempt and show an error. Existing bounded reconnects, mute/volume, fullscreen, and normal stream information are preserved. Browser statistics support both `kind` and the older `mediaType` field; missing decoded-frame counters are not treated as zero. Only recognized browser exception names, not potentially sensitive raw messages, appear in connection errors.

See [viewer troubleshooting](viewer-troubleshooting.md) and `scripts/test-viewer-connection.mjs`. Broader device/network coverage, long-session performance, and glass-to-glass latency remain acceptance work.

## Link-preview branding

The viewer advertises a separate 512x512 PNG through Open Graph image metadata: an RGB-inverted version of the original bird composited onto opaque white. The original `logo.png`, browser favicon, desktop icons, installer artwork, and visible viewer logo are unchanged. `scripts/Build-LinkPreview.ps1` regenerates the checked-in `viewer/link-preview-inverted-v1.png` when the brand source changes; `scripts/Test-LinkPreview.ps1` checks reproducibility, inversion, dimensions, and preservation of the original.

The host embeds the derivative and serves it at `/link-preview-inverted-v1.png`. Crawler-facing HTML uses an absolute asset URL constructed from a validated request Host authority and the application's configured mode (HTTPS for public sharing, HTTP for LAN). Forwarded headers do not control it. Metadata excludes request queries and access tokens; no canonical `og:url` is emitted to replace the original fragment-bearing sharing link. Crawlers fetch only the public page/artwork and do not consume a viewer session. Actual WhatsApp preview selection remains an external-client check; test a fresh sharing link after updating.

## Primary references consulted

- [D3D11 capture and WGC properties](https://gstreamer.freedesktop.org/documentation/d3d11/d3d11screencapturesrc.html)
- [D3D11 NVENC encoder and versioned properties](https://gstreamer.freedesktop.org/documentation/nvcodec/nvd3d11h264enc.html)
- [WASAPI loopback and process-tree support](https://gstreamer.freedesktop.org/documentation/wasapi2/wasapi2src.html)
- [webrtcbin signaling actions](https://gstreamer.freedesktop.org/documentation/webrtc/index.html)
- [gstreamer-rs releases](https://docs.rs/crate/gstreamer/latest)
- [Axum releases](https://docs.rs/crate/axum/latest)
