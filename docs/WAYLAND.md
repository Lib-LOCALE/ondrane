# Linux and Wayland

Ondrane selects native Wayland by default and uses the `ondrane` app ID. The optional XWayland preference is local to the application’s winit event-loop setup; it does not unset the session’s Wayland environment.

## Desktop integration

- File and folder selection prefers the XDG FileChooser portal; response subscription is established before opening the dialog. Cancellation stays an empty selection, not an error. Legacy zenity/kdialog fallbacks are used when the portal is unavailable.
- Idle inhibition prefers the XDG Inhibit portal, releasing its request on stop. A ScreenSaver D-Bus fallback is available. X11-only tools are not used as a Wayland capture substitute.
- Screen/window capture uses XDG ScreenCast → authorised PipeWire descriptor → GStreamer → a Matroska pipe → libVLC. The desktop asks the user to choose a source. Stop, media replacement and shutdown release the helper and portal session. A late worker reply cannot restart capture after Stop.

Install `gstreamer1.0-tools`, `gstreamer1.0-pipewire`, `gstreamer1.0-plugins-base` and `gstreamer1.0-plugins-good` on Debian/Ubuntu, along with your desktop’s portal implementation and PipeWire. Older PipeWire plugins without `keepalive-time` are supported, but the compositor may only deliver frames when content changes.

Capture audio uses the default PulseAudio/PipeWire input; it does not promise a mix of every application’s audio. Copy recording of the MJPEG/PCM capture stream uses Matroska. Capture descriptors are not written into recent-media history. Reopening/restarting a capture refreshes the pipe and its container header under the existing authorised session.

## Rendering and limitations

libVLC 3 memory callbacks require CPU pixels. Embedded video uses software decoding into a bounded RGBA frame pipeline; it does not support zero-copy GPU video or HDR passthrough. Slint can render the interface through OpenGL or its software backend. Separate-window playback can use native libVLC hardware decoding when its drivers/plugins support it.

Native Wayland drag-and-drop is limited by winit 0.30. Use the file picker, Ctrl+O or optional XWayland mode for that workflow. Absolute window positioning, always-on-top and wallpaper-style windows depend on compositor support. Do not infer full GNOME/KDE/COSMIC parity from one desktop’s test.
