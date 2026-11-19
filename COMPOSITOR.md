# Horn Wayland compositor (Smithay 0.7)

`horn-compositor` is a separate binary built on **Smithay 0.7**. It is a real
Wayland server: it advertises `wl_compositor`, `wl_shm`, `wl_seat`, `xdg_shell`,
`wl_output`/`xdg_output` and data-device globals, accepts clients on its own
socket, focuses and configures toplevels, and renders them with Smithay's GL
renderer.

It currently uses Smithay's **winit backend**, so it runs as a nested compositor
inside your existing Wayland session. That keeps the build free of `libinput`
and DRM system headers. Replace `winit::init` with the udev/DRM backend (and add
those system libraries) to make it a seat-backed hardware compositor.

## Run

```sh
cargo build --bin horn-compositor
./target/debug/horn-compositor
```

It binds `wayland-horn` (override with `HORN_WAYLAND_SOCKET`). Child processes
started from the shell inherit `WAYLAND_DISPLAY=wayland-horn`. A stale socket
from a killed session is cleaned up automatically.

### Which shell do you want?

- **Default (`HORN_SHELL=gpui`)** — the compositor launches only your existing
  Metro/Lumen tile app (`horn`) as its shell and draws **no desktop UI of its
  own**. This is "shell only" mode:

  ```sh
  ./target/debug/horn-compositor
  ```

  Press `F1` if you ever want the compositor's debug overlay.
  The loop is capped at 30 Hz by default; use `HORN_FPS=60` for smoother motion.
  The nested window prefers the X11/Xwayland EGL backend when `DISPLAY` is set,
  because Mesa's Wayland EGL path can fall back to software rendering. Force a
  backend with `HORN_BACKEND=x11` or `HORN_BACKEND=wayland`.
  Intel GPUs are detected from sysfs and Mesa is pointed at `iris` so wgpu
  does not fall back to `llvmpipe`. Override with
  `HORN_MESA_DRIVER=<your-driver>` or set `MESA_LOADER_DRIVER_OVERRIDE`
  yourself. An explicit `HORN_SHELL_APP_ID` (default `horn`) identifies the
  shell toplevel so it is configured fullscreen, underneath other windows.

  If the GPUI shell crashes or is closed, the compositor automatically shows
  its own fallback panel instead of a black screen.
- `HORN_SHELL=egui` — the compositor draws its own egui top panel / apps menu /
  quick settings instead. Intended for debugging the compositor, not as a
  daily shell.

To connect a client manually:

```sh
WAYLAND_DISPLAY=wayland-horn weston-terminal
```

## Controls

In default `gpui` mode, your Metro app owns the UI (status bar, apps menu,
tiles). The controls below belong to the optional compositor overlay
(`HORN_SHELL=egui` or `F1`), not to the normal shell:

- **Top panel**
  - `⊞ Start` opens/closes the apps menu.
  - One button per window: click to focus, `▁` to minimize, `✕` to close
    (kills the client connection).
  - Four workspace buttons; `1`–`4` are also keyboard shortcuts.
  - Clock and a `⚙` quick-settings button.
- **Apps menu**
  - Launcher grid (edit `APPS` in `src/bin/horn-compositor/shell.rs`).
  - Running-window list with focus and close controls.
- **Quick settings**
  - Wi-Fi, Bluetooth and airplane-mode toggles, brightness and volume
    sliders, and a button that exits the compositor.
- **Keyboard**
  - `Esc` closes the apps/quick menus.
  - `1`–`4` switch workspaces.
  - `F1` toggles the compositor shell panel on/off.
  - Other keys go to the focused Wayland client unless an egui field wants them.

Pointer input over the panels is captured by the shell; everything else is
forwarded to the client under the cursor.

## Source layout

```
src/bin/horn-compositor.rs        event loop, winit backend, input routing, rendering
src/bin/horn-compositor/state.rs  Smithay protocol handlers, windows, focus, output
src/bin/horn-compositor/shell.rs  egui shell UI (top panel / apps menu / quick settings)
```

## Current scope

- The original GPUI "Metro" tile UI (`cargo run --bin horn`) is untouched and
  remains a separate application. It is **not** rendered by the compositor.
- The compositor shell uses egui rather than the GPUI tile renderer. Porting the
  7k-line tile UI into a compositor-side renderer is a separate, larger task;
  `shell.rs` is the single place to replace or extend the shell.
- XDG decoration, drag/resize, clipboard/selection transfer, XWayland, and
  hardware (udev/DRM/libinput) backends are not implemented yet.
