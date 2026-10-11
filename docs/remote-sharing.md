# Browser-only remote sharing

The Windows app keeps capture and NVENC encoding on your PC. A public HTTPS tunnel carries the viewer page and WebSocket signaling. WebRTC tries direct media and uses managed TURN when needed. Viewers open a private browser link and do not install Tailscale.

## One-time setup

1. Create a Cloudflare account and open **Realtime → TURN** in the dashboard. Create a TURN key and obtain its key ID and API token. Use the [official credential guide](https://developers.cloudflare.com/realtime/turn/generate-credentials/) for the current dashboard/API workflow. Check the provider's current billing terms before enabling a service.
2. Obtain `cloudflared.exe` from the [official Windows download](https://developers.cloudflare.com/tunnel/downloads/). The packaged build includes it when supplied to the package script.
3. Open `desktop.exe`. Enter the TURN key ID and API token. **Remember credentials on this PC** is checked by default: starting public sharing saves them in Windows Credential Manager, and subsequent app launches restore them. Uncheck it for memory-only use; doing so also removes any previously saved credentials. Credentials are passed to the host via its process environment, never the viewer assets or installer.
4. Check **Create public HTTPS link**, then click **Start sharing**.
5. Wait for the viewer link, copy it, and open it on a phone using **mobile data**, not the home Wi-Fi. The viewer requires the secret in the link.

No domain is needed for the first test. A Quick Tunnel supplies a temporary `https://...trycloudflare.com` address. Its hostname changes on restart and it has no uptime guarantee. Cloudflare describes this as a testing/development feature, not production hosting: [Quick Tunnel documentation](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/). If a local cloudflared config interferes with Quick Tunnels, follow that guide rather than deleting your existing configuration.

## Controls and lifecycle

- **Forget saved credentials** removes the stored key/token and clears both fields when sharing is stopped. It does not revoke the Cloudflare token; revoke it in the Cloudflare dashboard if needed. Saved credentials belong to your Windows user on this PC, persist across app upgrades, and are not portable with the ZIP. Uninstall does not remove them: use Forget first, or remove `MonochromeCapture/CloudflareTURN/v1` under Windows Credential Manager's Windows Credentials. Windows-protected storage is not protection against malware running as your own user. See [Microsoft's credential storage documentation](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentialw).
- Remote mode listens on `127.0.0.1:8000`; it does not expose the HTTP service through your router.
- The local server mints temporary TURN credentials only after a viewer passes token authentication and one-viewer admission. Long-lived provider API credentials never go to the browser. Generated relay credentials last four hours and are cached for five minutes to avoid repeated API calls during reconnects. Reconnect to begin a new session if the four-hour limit is reached.
- The viewer's **Stream information** icon opens statistics including **Direct** or **Relay**. A public page loading successfully is not proof that media crossed the network.
- The browser viewer uses the Monochrome - Share logo and grayscale theme. A floating dock replaces the top/bottom bars: speaker icon for mute, slider for volume, information icon for statistics, and icon-only fullscreen. During playback it fades after inactivity; tap the screen, move the pointer, or tab into the controls to reveal it. Controls remain inside stage fullscreen; Safari's native video fullscreen uses the browser's own controls.
- **Stop** closes the tunnel and stops capture. **Exit** stops managed processes and exits. Closing/minimizing the window keeps sharing enabled in the tray. Double-click the tray icon to reopen it; right-click for Stop or Exit.
- **Diagnostics** expands the local host/tunnel log panel; click **Hide diagnostics** to return to the compact view. **Copy link** and **Open** become available once the viewer URL is ready. The UI uses a Material-inspired native layout without an embedded browser.
- Every new sharing launch gets a fresh viewer token. A copied link grants access until sharing is stopped. Do not post it publicly.
- For LAN mode, leave public sharing unchecked and enter the host's LAN IP and port. Allow the installed host executable through Windows Firewall on the intended private network if required.

## CLI alternative

```powershell
. .\scripts\Enter-DevEnvironment.ps1
$env:SCREEN_SHARE_TURN_KEY_ID = 'YOUR_TURN_KEY_ID'
# Enter this locally; never paste the API token into chat or source control.
$credential = Get-Credential -UserName 'TURN' -Message 'Enter the TURN API token as the password'
$env:SCREEN_SHARE_TURN_API_TOKEN = $credential.GetNetworkCredential().Password
cargo run --features media -- --remote --bind 127.0.0.1:8000
```

In a second terminal, start `cloudflared tunnel --url http://127.0.0.1:8000`. Replace the localhost portion of the printed Viewer URL with the generated HTTPS address, keeping `/#token=...`. Stop both processes when done.

Existing self-hosted TURN works using `SCREEN_SHARE_TURN=turn://user:password@host:3478`. Use URI-escaped credentials. The host passes the corresponding ICE configuration to the browser. A public HTTPS endpoint is still needed.

## Remote acceptance checks

- Android browser on mobile data: page, video, audio, displayed route, FPS, bitrate, RTT.
- Test Direct and Relay paths on different networks. A relay adds bandwidth cost and can increase latency; measure actual glass-to-glass latency.
- Verify lock/unlock and longer backgrounding recover, then test a two-hour session.
- Stop sharing: the old URL stops working and no host/tunnel process remains.
- Never put the TURN API token in the browser or a public tunnel URL.

A stable named tunnel/domain, adaptive bitrate for weak uplinks, signed installer, and fresh-machine installation testing remain release milestones.
