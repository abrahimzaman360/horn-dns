//! Horn — a real Wayland compositor and shell built on Smithay 0.7.
//!
//! Runs as a nested compositor through Smithay's winit backend so it works
//! inside an existing Wayland session. Swap `winit::init` for the udev/DRM
//! backend and add libinput/DRM system libraries to turn it into a real
//! seat-backed, hardware compositor.
//!
//! By default the compositor launches the existing GPUI "Metro" app as its
//! only shell and draws no UI of its own. An optional `smithay-egui` debug
//! overlay can be enabled with `HORN_SHELL=egui`.

#[path = "horn-compositor/shell.rs"]
mod shell;
#[path = "horn-compositor/state.rs"]
mod state;

use std::{
    fs,
    path::Path,
    process::{Child, Command},
    sync::Arc,
    time::{Duration, Instant},
};

use smithay::{
    backend::{
        input::{
            AbsolutePositionEvent, Axis, AxisSource, Event, InputEvent, KeyboardKeyEvent,
            PointerAxisEvent, PointerButtonEvent,
        },
        renderer::{
            Color32F, Frame, Renderer,
            element::{Element, RenderElement},
            glow::GlowRenderer,
            utils::draw_render_elements,
        },
        winit::{self, WinitEvent},
    },
    input::{
        keyboard::{FilterResult, keysyms},
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    reexports::wayland_server::{ListeningSocket, protocol::wl_surface::WlSurface},
    utils::{Rectangle, SERIAL_COUNTER, Transform},
    wayland::compositor::{SurfaceAttributes, TraversalAction, with_surface_tree_downward},
};
use smithay_egui::EguiState;
use state::{ClientState, State};

/// Target the display's normal 60 Hz cadence by default. The limit prevents
/// winit's zero-timeout event pump from rendering and sending client frame
/// callbacks at an unbounded rate. Override with `HORN_FPS` (1..=240).
fn frame_duration() -> Duration {
    let fps = std::env::var("HORN_FPS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|&fps| (1..=240).contains(&fps))
        .unwrap_or(60);
    Duration::from_nanos(1_000_000_000 / fps)
}

/// Detect an Intel GPU from sysfs (vendor `0x8086`) so we can select Mesa's
/// `iris` driver instead of guessing from the presence of a render node.
fn has_intel_gpu() -> bool {
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        if let Ok(vendor) = fs::read_to_string(entry.path().join("device/vendor")) {
            if vendor.trim() == "0x8086" {
                return true;
            }
        }
    }
    false
}

/// Spawn the GPUI shell next to this binary so it connects to our socket.
fn launch_gpui_shell() -> std::io::Result<Child> {
    let exe = std::env::current_exe().ok();
    let shell = exe
        .as_ref()
        .and_then(|path| path.parent())
        .map(|dir| dir.join("horn"))
        .unwrap_or_else(|| Path::new("horn").to_path_buf());
    let mut command = Command::new(shell);
    // An explicit HORN_MESA_DRIVER still wins. Otherwise the compositor has
    // already selected the right driver globally (see main), and the child
    // inherits it.
    if std::env::var_os("MESA_LOADER_DRIVER_OVERRIDE").is_none() {
        if let Ok(driver) = std::env::var("HORN_MESA_DRIVER") {
            command.env("MESA_LOADER_DRIVER_OVERRIDE", driver);
        }
    }
    command.env("LIBGL_ALWAYS_SOFTWARE", "0");
    command.spawn()
}

/// Bind the compositor's Wayland socket, clearing a stale socket left behind
/// by a previous (killed) session.
fn bind_socket(name: &str) -> Result<ListeningSocket, Box<dyn std::error::Error>> {
    use smithay::reexports::wayland_server::BindError;
    match ListeningSocket::bind(name) {
        Ok(listener) => Ok(listener),
        Err(BindError::AlreadyInUse) => {
            if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
                let path = Path::new(&dir).join(name);
                let _ = std::fs::remove_file(&path);
                let _ = std::fs::remove_file(path.with_extension("lock"));
            }
            Ok(ListeningSocket::bind(name)?)
        }
        Err(err) => Err(Box::new(err)),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = std::env::var("HORN_WAYLAND_SOCKET").unwrap_or_else(|_| "wayland-horn".into());

    let mut display = smithay::reexports::wayland_server::Display::<State>::new()?;
    let mut dh = display.handle();

    // Select Mesa's driver from actual hardware: Intel -> iris. Mesa's Wayland
    // EGL path otherwise often hands wgpu `llvmpipe`, which makes the GPUI
    // shell either crash ("no supported texture formats") or render in
    // software. HORN_MESA_DRIVER overrides detection.
    if std::env::var_os("MESA_LOADER_DRIVER_OVERRIDE").is_none() {
        let driver = std::env::var("HORN_MESA_DRIVER")
            .ok()
            .or_else(|| has_intel_gpu().then(|| "iris".to_string()));
        if let Some(driver) = driver {
            // SAFETY: startup, before EGL init or any threads.
            unsafe {
                std::env::set_var("MESA_LOADER_DRIVER_OVERRIDE", driver);
                std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "0");
            }
        }
    }

    // Prefer the X11/Xwayland EGL path for the nested window when a DISPLAY is
    // available. On some Wayland sessions Mesa cannot obtain a DRM fd from the
    // outer compositor and silently falls back to software rendering, which
    // makes the GPUI shell extremely slow. X11/Xwayland keeps the real GPU.
    let backend_mode = std::env::var("HORN_BACKEND").unwrap_or_else(|_| "auto".into());
    if backend_mode.eq_ignore_ascii_case("x11")
        || (backend_mode.eq_ignore_ascii_case("auto") && std::env::var_os("DISPLAY").is_some())
    {
        // SAFETY: startup, before any winit/EGL initialization or threads.
        unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
    }

    // The nested backend gives us a window, input events and a GL renderer.
    let (mut backend, mut winit) = winit::init::<GlowRenderer>()?;
    let size = backend.window_size().to_logical(1);

    let egui = EguiState::new(Rectangle::from_size(size));
    let mut state = State::new(&dh, egui, size, 1.0);

    let listener = bind_socket(&socket)?;
    // Children launched from the shell connect to *this* compositor. This must
    // happen after winit::init, which connects to the outer session.
    // SAFETY: single-threaded startup; no other thread reads the environment yet.
    unsafe { std::env::set_var("WAYLAND_DISPLAY", &socket) };

    // Default to the user's own shell only. The compositor's egui panel is a
    // debug/control overlay, enabled explicitly with HORN_SHELL=egui or F1.
    let shell_mode = std::env::var("HORN_SHELL").unwrap_or_else(|_| "gpui".into());
    let mut shell_child: Option<Child> = None;
    if shell_mode.eq_ignore_ascii_case("gpui") {
        match launch_gpui_shell() {
            Ok(child) => {
                shell_child = Some(child);
                state.shell_visible = false;
                // Identify the shell's xdg toplevel so it can be configured as
                // a fullscreen root surface underneath all other clients.
                state.shell_app_id =
                    Some(std::env::var("HORN_SHELL_APP_ID").unwrap_or_else(|_| "horn".to_string()));
            }
            Err(err) => {
                eprintln!("horn-compositor: could not launch GPUI shell: {err}");
                // Keep the compositor usable instead of showing a black screen.
                state.shell_visible = true;
            }
        }
    }

    let mut clients = Vec::new();
    let start_time = Instant::now();
    let frame_duration = frame_duration();

    loop {
        if state.should_exit {
            break;
        }
        let frame_start = Instant::now();

        if let smithay::reexports::winit::platform::pump_events::PumpStatus::Exit(_) =
            winit.dispatch_new_events(|event| handle_input(&mut state, event))
        {
            break;
        }

        // If the GPUI shell exits (crash, or the user closed it), fall back to
        // the compositor's own panel so the session never becomes a black
        // rectangle with no controls.
        let mut shell_exited = false;
        if let Some(child) = shell_child.as_mut() {
            if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
                shell_exited = true;
            }
        }
        if shell_exited {
            shell_child = None;
            eprintln!("horn-compositor: GPUI shell exited; showing compositor shell");
            state.shell_visible = true;
            state.apps_open = false;
            state.quick_open = false;
        }

        if let Some(stream) = listener.accept()? {
            let client = dh.insert_client(stream, Arc::new(ClientState::default()))?;
            clients.push(client);
        }
        display.dispatch_clients(&mut state)?;
        display.flush_clients()?;

        let physical = backend.window_size();
        let damage = Rectangle::from_size(physical);
        let scale = 1.0;

        // Draw the shell into a texture first; it is composited on top below.
        // Never ask smithay-egui to render an empty UI: its `used_rect` becomes
        // infinite and the render-target size overflows to a negative Size.
        let shell_element = if state.shell_visible {
            let egui = state.egui.clone();
            Some(egui.render(
                |ctx| shell::draw(ctx, &mut state),
                backend.renderer(),
                Rectangle::from_size(physical.to_logical(1)),
                scale,
                1.0,
            )?)
        } else {
            None
        };
        let client_elements = state.surface_elements(backend.renderer());

        {
            let (renderer, mut framebuffer) = backend.bind()?;
            let mut frame = renderer.render(&mut framebuffer, physical, Transform::Flipped180)?;
            frame.clear(Color32F::new(0.06, 0.06, 0.08, 1.0), &[damage])?;
            draw_render_elements(&mut frame, scale, &client_elements, &[damage])?;
            if let Some(ref shell_element) = shell_element {
                RenderElement::<GlowRenderer>::draw(
                    shell_element,
                    &mut frame,
                    Element::src(shell_element),
                    Element::geometry(shell_element, scale.into()),
                    &[damage],
                    &[],
                )?;
            }
            let _ = frame.finish()?;
        }

        let now = start_time.elapsed().as_millis() as u32;
        for window in &state.windows {
            send_frames_surface_tree(window.surface.wl_surface(), now);
        }

        backend.submit(Some(&[damage]))?;

        // Pace the loop. `pump_events` returns immediately when there is no
        // input, so without this we would render (and send frame callbacks)
        // thousands of times per second.
        if let Some(remaining) = frame_duration.checked_sub(frame_start.elapsed()) {
            std::thread::sleep(remaining);
        }
    }

    Ok(())
}

/// Route winit input between the shell and Wayland clients.
fn handle_input(state: &mut State, event: WinitEvent) {
    match event {
        WinitEvent::Resized { size, scale_factor } => {
            let scale = if scale_factor > 0.0 {
                scale_factor
            } else {
                1.0
            };
            state.output_scale = scale;
            state.output_size = (
                (size.w as f64 / scale).round().max(1.0) as i32,
                (size.h as f64 / scale).round().max(1.0) as i32,
            )
                .into();
            state.update_output_size(state.output_size);
            state.reconfigure_all();
        }
        WinitEvent::Input(event) => match event {
            InputEvent::DeviceAdded { device } => state.egui.handle_device_added(&device),
            InputEvent::DeviceRemoved { device } => state.egui.handle_device_removed(&device),
            InputEvent::Keyboard { event } => {
                let pressed = event.state() == smithay::backend::input::KeyState::Pressed;
                let serial = SERIAL_COUNTER.next_serial();
                let keyboard = state.keyboard.clone();
                let _ = keyboard.input::<(), _>(
                    state,
                    event.key_code(),
                    event.state(),
                    serial,
                    event.time_msec(),
                    |data, modifiers, handle| {
                        data.egui.handle_keyboard(&handle, pressed, *modifiers);
                        if shortcut(data, &handle, pressed) || data.egui.wants_keyboard() {
                            FilterResult::Intercept(())
                        } else {
                            FilterResult::Forward
                        }
                    },
                );
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let pos = event.position_transformed(state.output_size);
                state.pointer_location = pos;
                state.egui.handle_pointer_motion(pos.to_i32_floor());

                let shell_owns =
                    shell::pointer_over_shell(state, pos) || state.egui.wants_pointer();
                let focus = if shell_owns {
                    None
                } else {
                    state
                        .window_at(pos)
                        .map(|surface| (surface, (0.0, 0.0).into()))
                };
                let pointer = state.pointer.clone();
                pointer.motion(
                    state,
                    focus,
                    &MotionEvent {
                        location: pos,
                        serial: SERIAL_COUNTER.next_serial(),
                        time: event.time_msec(),
                    },
                );
            }
            InputEvent::PointerButton { event } => {
                let shell_owns = shell::pointer_over_shell(state, state.pointer_location)
                    || state.egui.wants_pointer();
                if shell_owns {
                    if let Some(button) = mouse_button(event.button_code()) {
                        let pressed =
                            event.state() == smithay::backend::input::ButtonState::Pressed;
                        state.egui.handle_pointer_button(button, pressed);
                    }
                } else {
                    if event.state() == smithay::backend::input::ButtonState::Pressed {
                        if let Some(surface) = state.window_at(state.pointer_location) {
                            state.focus_window(&surface, SERIAL_COUNTER.next_serial());
                        }
                    }
                    let pointer = state.pointer.clone();
                    pointer.button(
                        state,
                        &ButtonEvent {
                            button: event.button_code(),
                            state: event.state().into(),
                            serial: SERIAL_COUNTER.next_serial(),
                            time: event.time_msec(),
                        },
                    );
                }
            }
            InputEvent::PointerAxis { event } => {
                let horizontal = event.amount(Axis::Horizontal).unwrap_or_else(|| {
                    event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.0
                });
                let vertical = event.amount(Axis::Vertical).unwrap_or_else(|| {
                    event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.0
                });
                let shell_owns = shell::pointer_over_shell(state, state.pointer_location)
                    || state.egui.wants_pointer();
                if shell_owns {
                    state.egui.handle_pointer_axis(horizontal, vertical);
                } else {
                    let mut frame = AxisFrame::new(event.time_msec()).source(event.source());
                    if horizontal != 0.0 {
                        frame = frame.value(Axis::Horizontal, horizontal);
                        if let Some(v) = event.amount_v120(Axis::Horizontal) {
                            frame = frame.v120(Axis::Horizontal, v as i32);
                        }
                    } else if event.source() == AxisSource::Finger {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if vertical != 0.0 {
                        frame = frame.value(Axis::Vertical, vertical);
                        if let Some(v) = event.amount_v120(Axis::Vertical) {
                            frame = frame.v120(Axis::Vertical, v as i32);
                        }
                    } else if event.source() == AxisSource::Finger {
                        frame = frame.stop(Axis::Vertical);
                    }
                    let pointer = state.pointer.clone();
                    pointer.axis(state, frame);
                }
            }
            _ => {}
        },
        _ => {}
    }
}

/// Map Linux input-event button codes to Smithay mouse buttons for the shell.
fn mouse_button(code: u32) -> Option<smithay::backend::input::MouseButton> {
    use smithay::backend::input::MouseButton;
    match code {
        0x110 => Some(MouseButton::Left),
        0x111 => Some(MouseButton::Right),
        0x112 => Some(MouseButton::Middle),
        0x113 | 0x114 => Some(MouseButton::Back),
        0x115 | 0x116 => Some(MouseButton::Forward),
        _ => None,
    }
}

/// Compositor-wide keyboard shortcuts. Returning true keeps the key away from clients.
fn shortcut(
    state: &mut State,
    handle: &smithay::input::keyboard::KeysymHandle,
    pressed: bool,
) -> bool {
    if !pressed {
        return false;
    }
    let sym = handle.modified_sym();
    if sym == keysyms::KEY_Escape.into() {
        state.apps_open = false;
        state.quick_open = false;
        return true;
    }
    if sym == keysyms::KEY_F1.into() {
        state.shell_visible = !state.shell_visible;
        if !state.shell_visible {
            state.apps_open = false;
            state.quick_open = false;
        }
        return true;
    }
    for ws in 0..state::WORKSPACES {
        let key: smithay::input::keyboard::Keysym = (keysyms::KEY_1 + ws as u32).into();
        if sym == key {
            state.set_workspace(ws, SERIAL_COUNTER.next_serial());
            return true;
        }
    }
    false
}

/// Send a frame callback to everything in a surface tree.
fn send_frames_surface_tree(surface: &WlSurface, time: u32) {
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_, states, &()| {
            for callback in states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .drain(..)
            {
                callback.done(time);
            }
        },
        |_, _, &()| true,
    );
}
