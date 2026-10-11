# Bundled components

The package includes a subset of the official GStreamer runtime plugins, their DLL dependencies, and upstream license texts in `runtime/share/licenses`.

GStreamer and many runtime libraries use LGPL or other open-source licenses. Their binaries remain separate, replaceable DLLs. Upstream sources and build instructions are available at:

- https://gstreamer.freedesktop.org/download/
- https://gitlab.freedesktop.org/gstreamer/gstreamer
- https://gitlab.freedesktop.org/gstreamer/cerbero

The copied upstream license directory identifies individual bundled dependencies. Keep these notices and licenses with redistributed packages and fulfill the applicable source-provision requirements before public distribution.

cloudflared is distributed under the Apache License 2.0. Source and license: https://github.com/cloudflare/cloudflared. The package contains its license at `cloudflared-LICENSE` when the binary is bundled.

The project is currently an unsigned personal-use prototype, not a signed public software release.
