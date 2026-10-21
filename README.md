# Monochrome - Share

1-to-1 Private screen sharing.

Share your Windows desktop and audio with one viewer through a browser link. Viewers do not need to install an app.

## Features

- NVIDIA NVENC hardware encoding, targeting 1080p at 60fps.
- Optional desktop audio with viewer mute, volume, fullscreen, and reconnect controls.
- Token-protected access with one viewer at a time.
- Local-network sharing or browser-only remote access through a public HTTPS link.
- Lightweight native Windows app with a dark interface and system-tray controls.
- Optional credential storage in Windows Credential Manager.

## Setup

### Windows app

Requires Windows 10/11 and an NVIDIA GPU with a working NVENC driver. Packaged builds include GStreamer; Rust and developer tools are not required.

1. Install using the setup executable, or extract the portable ZIP and run `desktop.exe`.
2. Choose whether to include desktop audio.
3. Configure local or remote sharing below, then click **Start sharing**.
4. Copy the viewer link and send it privately. Open it in a browser and tap to play.

Minimizing or closing the window keeps sharing active in the system tray. Use **Stop** to end sharing or **Exit** to close the app.

### Local-network sharing

Leave **Create public HTTPS link** unchecked and enter your PC's LAN IP and port, for example `192.168.1.12:8000`. Connect the viewer to the same network.

Allow the host through Windows Firewall on your private network if needed. Use local HTTP sharing only on a trusted network.

### Remote sharing

1. Obtain a Cloudflare Realtime TURN key ID and API token.
2. Enter them in the desktop app and enable **Create public HTTPS link**.
3. Start sharing and send the generated link to the viewer.

No domain, VPS, or viewer installation is required. **Remember credentials on this PC** saves credentials for future use. TURN usage may incur provider charges.

See the [remote-sharing guide](docs/remote-sharing.md) for credential setup and troubleshooting.

Keep viewer links private. Stopping sharing ends access; restarting creates a new token.

## Build from source

Install Rust stable (MSVC), Visual Studio C++ Build Tools with a Windows SDK, and matching **GStreamer MSVC x86_64 runtime and development packages, version 1.26 or newer**.

Set `GSTREAMER_1_0_ROOT_MSVC_X86_64` to your GStreamer installation directory, then run:

```powershell
. .\scripts\Enter-DevEnvironment.ps1
.\scripts\Test-Environment.ps1
cargo build --release --features media --bins
.\target\release\desktop.exe
```

Create a portable ZIP and installer with:

```powershell
.\scripts\Build-Package.ps1
```

Installer builds require Inno Setup. Remote sharing requires `cloudflared`; see the remote-sharing guide.

## More information

Video and audio playback have been reported working on Android and iOS; public-link sharing has also been reported working. Broader device, long-session, and fresh-machine installation testing remain pending.

- [Remote-sharing setup](docs/remote-sharing.md)
- [Implementation notes](docs/implementation.md)
- [Validation checklist](docs/validation.md)
