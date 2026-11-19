//! Smithay compositor state: protocols, window bookkeeping and focus.

use std::os::unix::io::OwnedFd;

use smithay::{
    backend::renderer::{
        element::{
            Kind,
            surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
        },
        glow::GlowRenderer,
        utils::on_commit_buffer_handler,
    },
    delegate_compositor, delegate_data_device, delegate_output, delegate_seat, delegate_shm,
    delegate_xdg_shell,
    input::{
        Seat, SeatHandler, SeatState,
        keyboard::{KeyboardHandle, XkbConfig},
        pointer::PointerHandle,
    },
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            Client, DisplayHandle, Resource,
            backend::{ClientData, ClientId, DisconnectReason, protocol::ProtocolError},
            protocol::{wl_buffer, wl_surface::WlSurface},
        },
    },
    utils::{Logical, Point, Rectangle, Serial, Size, Transform},
    wayland::{
        buffer::BufferHandler,
        compositor::{CompositorClientState, CompositorHandler, CompositorState, with_states},
        output::{OutputHandler, OutputManagerState},
        selection::{
            SelectionHandler,
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
            },
        },
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            XdgToplevelSurfaceData,
        },
        shm::{ShmHandler, ShmState},
    },
};
use smithay_egui::EguiState;

/// How many virtual desktops the shell exposes.
pub(crate) const WORKSPACES: usize = 4;
/// Height of the shell's top panel in logical pixels.
pub(crate) const PANEL_H: i32 = 36;

/// One managed xdg toplevel.
#[derive(Debug, Clone)]
pub(crate) struct ManagedWindow {
    pub(crate) surface: ToplevelSurface,
    pub(crate) location: Point<i32, Logical>,
    pub(crate) size: Size<i32, Logical>,
    pub(crate) workspace: usize,
    pub(crate) minimized: bool,
}

impl ManagedWindow {
    pub(crate) fn rect(&self) -> Rectangle<i32, Logical> {
        Rectangle::new(self.location, self.size)
    }

    /// Window title from the xdg role attributes, or a stable fallback.
    pub(crate) fn title(&self) -> String {
        with_states(self.surface.wl_surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().unwrap().title.clone())
                .unwrap_or_else(|| "Window".to_string())
        })
    }
}

/// The compositor's global state.
pub(crate) struct State {
    pub(crate) dh: DisplayHandle,
    pub(crate) compositor_state: CompositorState,
    pub(crate) output: Output,
    pub(crate) output_state: OutputManagerState,
    pub(crate) xdg_shell_state: XdgShellState,
    pub(crate) shm_state: ShmState,
    pub(crate) seat_state: SeatState<Self>,
    pub(crate) data_device_state: DataDeviceState,

    /// Kept alive for the lifetime of the compositor; handles are cloned from it.
    #[allow(dead_code)]
    pub(crate) seat: Seat<Self>,
    pub(crate) keyboard: KeyboardHandle<Self>,
    pub(crate) pointer: PointerHandle<Self>,

    /// Shell UI (egui) rendered above all clients by the compositor itself.
    pub(crate) egui: EguiState,
    pub(crate) windows: Vec<ManagedWindow>,
    pub(crate) focused: Option<WlSurface>,
    pub(crate) workspace: usize,
    /// Output dimensions in compositor logical coordinates.
    pub(crate) output_size: Size<i32, Logical>,
    /// Physical-to-logical output scale reported by winit.
    pub(crate) output_scale: f64,
    pub(crate) pointer_location: Point<f64, Logical>,
    /// App id identifying the fullscreen root shell surface, when mapped.
    pub(crate) shell_app_id: Option<String>,

    // Shell state, all driven from the top panel / apps menu.
    /// Whether the compositor's own egui shell is painted. Turned off in
    /// `HORN_SHELL=gpui` mode so it doesn't duplicate the GPUI app's chrome.
    pub(crate) shell_visible: bool,
    pub(crate) apps_open: bool,
    pub(crate) quick_open: bool,
    pub(crate) wifi: bool,
    pub(crate) bluetooth: bool,
    pub(crate) airplane: bool,
    pub(crate) brightness: f32,
    pub(crate) volume: f32,
    pub(crate) should_exit: bool,
}

impl State {
    pub(crate) fn new(
        dh: &DisplayHandle,
        egui: EguiState,
        output_size: Size<i32, Logical>,
        output_scale: f64,
    ) -> Self {
        let compositor_state = CompositorState::new::<Self>(dh);
        let output_manager = OutputManagerState::new_with_xdg_output::<Self>(dh);

        // Advertise one output so clients can size themselves correctly.
        let output = Output::new(
            "Horn".into(),
            PhysicalProperties {
                size: (
                    (output_size.w as f64 * output_scale).round() as i32,
                    (output_size.h as f64 * output_scale).round() as i32,
                )
                    .into(),
                subpixel: Subpixel::Unknown,
                make: "Horn".into(),
                model: "Nested".into(),
            },
        );
        let mode = Mode {
            size: (output_size.w, output_size.h).into(),
            refresh: 60_000,
        };
        output.create_global::<Self>(dh);
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            Some(Scale::Integer(1)),
            Some((0, 0).into()),
        );
        output.set_preferred(mode);

        let shm_state = ShmState::new::<Self>(dh, vec![]);
        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(dh, "horn");
        let keyboard = seat.add_keyboard(XkbConfig::default(), 200, 25).unwrap();
        let pointer = seat.add_pointer();

        Self {
            dh: dh.clone(),
            compositor_state,
            output,
            output_state: output_manager,
            xdg_shell_state: XdgShellState::new::<Self>(dh),
            shm_state,
            seat_state,
            data_device_state: DataDeviceState::new::<Self>(dh),
            seat,
            keyboard,
            pointer,
            egui,
            windows: Vec::new(),
            focused: None,
            workspace: 0,
            output_size,
            output_scale,
            pointer_location: (0.0, 0.0).into(),
            shell_app_id: None,
            shell_visible: true,
            apps_open: false,
            quick_open: false,
            wifi: true,
            bluetooth: false,
            airplane: false,
            brightness: 1.0,
            volume: 0.7,
            should_exit: false,
        }
    }

    /// Keep the advertised output in sync with the nested window size.
    pub(crate) fn update_output_size(&self, size: Size<i32, Logical>) {
        let mode = Mode {
            size: (size.w, size.h).into(),
            refresh: 60_000,
        };
        self.output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            Some(Scale::Integer(1)),
            Some((0, 0).into()),
        );
        self.output.set_preferred(mode);
    }

    /// Usable area below the shell panel.
    fn work_area(&self) -> Rectangle<i32, Logical> {
        Rectangle::new(
            (0, PANEL_H).into(),
            (self.output_size.w, (self.output_size.h - PANEL_H).max(1)).into(),
        )
    }

    /// Reconfigure every toplevel after an output resize.
    pub(crate) fn reconfigure_all(&mut self) {
        let surfaces: Vec<_> = self.windows.iter().map(|w| w.surface.clone()).collect();
        for surface in surfaces {
            let (size, location) = self.geometry_for(&surface);
            if let Some(window) = self
                .windows
                .iter_mut()
                .find(|w| w.surface.wl_surface() == surface.wl_surface())
            {
                window.size = size;
                window.location = location;
            }
            let active = self.focused.as_ref() == Some(surface.wl_surface());
            self.configure(&surface, active);
        }
    }

    pub(crate) fn is_root_shell(&self, surface: &ToplevelSurface) -> bool {
        self.shell_app_id.as_deref().is_some_and(|app_id| {
            with_states(surface.wl_surface(), |states| {
                states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .and_then(|data| data.lock().unwrap().app_id.clone())
                    .as_deref()
                    == Some(app_id)
            })
        })
    }

    /// Geometry for a toplevel: the root shell fills the whole output, other
    /// clients live in the work area below the shell's own chrome.
    fn geometry_for(&self, surface: &ToplevelSurface) -> (Size<i32, Logical>, Point<i32, Logical>) {
        if self.is_root_shell(surface) {
            return (self.output_size, (0, 0).into());
        }
        let area = self.work_area();
        (
            ((area.size.w - 40).max(100), (area.size.h - 40).max(100)).into(),
            (area.loc.x + 20, area.loc.y + 20).into(),
        )
    }

    /// Configure the shell to the full output; ordinary clients use the work area.
    fn configure(&self, surface: &ToplevelSurface, active: bool) {
        let fullscreen = self.is_root_shell(surface);
        let area = if fullscreen {
            Rectangle::from_size(self.output_size)
        } else {
            self.work_area()
        };
        surface.with_pending_state(|state| {
            let margin = if fullscreen { 0 } else { 40 };
            state.size =
                Some(((area.size.w - margin).max(1), (area.size.h - margin).max(1)).into());
            state.bounds = Some(area.size);
            // The Fullscreen state is what clients act on. Smithay 0.7 does
            // not expose a public WlOutput handle from `Output`, so leave the
            // optional output hint unset.
            state.fullscreen_output = None;
            if fullscreen {
                state.states.set(xdg_toplevel::State::Fullscreen);
            } else {
                state.states.unset(xdg_toplevel::State::Fullscreen);
            }
            if active {
                state.states.set(xdg_toplevel::State::Activated);
            } else {
                state.states.unset(xdg_toplevel::State::Activated);
            }
        });
        surface.send_configure();
    }

    /// Give a surface keyboard focus and update xdg activation states.
    pub(crate) fn focus_window(&mut self, surface: &WlSurface, serial: Serial) {
        let keyboard = self.keyboard.clone();
        keyboard.set_focus(self, Some(surface.clone()), serial);
        self.focused = Some(surface.clone());
        for window in &self.windows {
            let active = window.surface.wl_surface() == surface;
            window.surface.with_pending_state(|state| {
                if active {
                    state.states.set(xdg_toplevel::State::Activated);
                } else {
                    state.states.unset(xdg_toplevel::State::Activated);
                }
            });
            window.surface.send_configure();
        }
        let ws = self
            .windows
            .iter()
            .find(|w| w.surface.wl_surface() == surface)
            .map(|w| w.workspace);
        if let Some(ws) = ws {
            self.workspace = ws;
        }
    }

    /// Focus the topmost visible window on the current workspace.
    pub(crate) fn focus_top(&mut self, serial: Serial) {
        let surface = self
            .windows
            .iter()
            .rev()
            .find(|w| w.workspace == self.workspace && !w.minimized)
            .map(|w| w.surface.wl_surface().clone());
        if let Some(surface) = surface {
            self.focus_window(&surface, serial);
        } else {
            self.focused = None;
            let keyboard = self.keyboard.clone();
            keyboard.set_focus(self, None, serial);
        }
    }

    pub(crate) fn close_window(&mut self, surface: &WlSurface) {
        if let Some(client) = surface.client() {
            client.kill(
                &self.dh,
                ProtocolError {
                    code: 0,
                    object_id: 0,
                    object_interface: "wl_display".to_string(),
                    message: "closed by shell".to_string(),
                },
            );
        }
        self.windows.retain(|w| w.surface.wl_surface() != surface);
        if self.focused.as_ref() == Some(surface) {
            self.focused = None;
        }
    }

    pub(crate) fn minimize_window(&mut self, surface: &WlSurface, serial: Serial) {
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| w.surface.wl_surface() == surface)
        {
            window.minimized = true;
        }
        if self.focused.as_ref() == Some(surface) {
            self.focused = None;
            let keyboard = self.keyboard.clone();
            keyboard.set_focus(self, None, serial);
        }
    }

    pub(crate) fn set_workspace(&mut self, workspace: usize, serial: Serial) {
        if workspace >= WORKSPACES {
            return;
        }
        self.workspace = workspace;
        self.focus_top(serial);
    }

    /// Topmost client window under the pointer (clients never receive input
    /// while the shell's panels own the pointer).
    pub(crate) fn window_at(&self, point: Point<f64, Logical>) -> Option<WlSurface> {
        self.windows
            .iter()
            .rev()
            .find(|w| {
                !w.minimized
                    && w.workspace == self.workspace
                    && w.rect().contains(point.to_i32_floor())
            })
            .map(|w| w.surface.wl_surface().clone())
    }

    /// Render elements for all visible client windows, bottom to top.
    pub(crate) fn surface_elements(
        &self,
        renderer: &mut GlowRenderer,
    ) -> Vec<WaylandSurfaceRenderElement<GlowRenderer>> {
        self.windows
            .iter()
            .filter(|w| !w.minimized && w.workspace == self.workspace)
            .flat_map(|w| {
                render_elements_from_surface_tree(
                    renderer,
                    w.surface.wl_surface(),
                    w.location.to_physical(1),
                    1.0,
                    1.0,
                    Kind::Unspecified,
                )
            })
            .collect()
    }
}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
    }
}

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let on_workspace = self.workspace;
        let (size, mut location) = self.geometry_for(&surface);
        if !self.is_root_shell(&surface) {
            let offset = (self.windows.len() % 6) as i32 * 26;
            location = (location.x + offset, location.y + offset).into();
        }
        self.windows.push(ManagedWindow {
            surface: surface.clone(),
            location,
            size,
            workspace: on_workspace,
            minimized: false,
        });
        let is_first = self
            .windows
            .iter()
            .filter(|w| w.workspace == on_workspace && !w.minimized)
            .count()
            == 1;
        self.configure(&surface, is_first);
        if is_first {
            let serial = smithay::utils::SERIAL_COUNTER.next_serial();
            self.focus_window(surface.wl_surface(), serial);
        }
    }

    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {}

    fn grab(
        &mut self,
        _surface: PopupSurface,
        _seat: smithay::reexports::wayland_server::protocol::wl_seat::WlSeat,
        _serial: Serial,
    ) {
    }

    fn reposition_request(
        &mut self,
        _surface: PopupSurface,
        _positioner: PositionerState,
        _token: u32,
    ) {
    }

    fn minimize_request(&mut self, surface: ToplevelSurface) {
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        self.minimize_window(surface.wl_surface(), serial);
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        // The shell may set its app_id after the toplevel is created. Adopt it
        // as the fullscreen root surface as soon as we recognise it.
        let (size, location) = self.geometry_for(&surface);
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| w.surface.wl_surface() == surface.wl_surface())
        {
            window.size = size;
            window.location = location;
        }
        let active = self.focused.as_ref() == Some(surface.wl_surface());
        self.configure(&surface, active);
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.windows
            .retain(|w| w.surface.wl_surface() != surface.wl_surface());
        if self.focused.as_ref() == Some(surface.wl_surface()) {
            self.focused = None;
        }
    }
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&WlSurface>) {}

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }
}

impl OutputHandler for State {}

impl AsMut<OutputManagerState> for State {
    fn as_mut(&mut self) -> &mut OutputManagerState {
        &mut self.output_state
    }
}

impl SelectionHandler for State {
    type SelectionUserData = ();
}

impl DataDeviceHandler for State {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for State {}

impl ServerDndGrabHandler for State {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {}
}

#[derive(Default)]
pub(crate) struct ClientState {
    pub(crate) compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}

    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

delegate_compositor!(State);
delegate_output!(State);
delegate_shm!(State);
delegate_seat!(State);
delegate_xdg_shell!(State);
delegate_data_device!(State);
