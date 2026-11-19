//! The compositor's own shell UI, rendered by egui above every Wayland client.
//!
//! Everything here is interactive: the top panel switches workspaces and
//! focuses/minimizes/closes windows, the apps menu launches programs and manages
//! running windows, and the quick panel toggles settings or exits the session.

use egui::{Align2, Color32, Context, Frame, Vec2};
use smithay::utils::{Logical, Point, SERIAL_COUNTER};

use crate::state::{State, WORKSPACES};

const ACCENT: Color32 = Color32::from_rgb(0xA4, 0xC4, 0x00);
const PANEL_BG: Color32 = Color32::from_rgb(0x1B, 0x1B, 0x1B);

/// Programs offered by the apps menu. If an executable is missing, the click is
/// simply ignored; drop your own entries in here to extend the launcher.
const APPS: &[(&str, &str)] = &[
    ("Terminal", "weston-terminal"),
    ("Text Editor", "kate"),
    ("Browser", "firefox"),
    ("Files", "nautilus"),
    ("Calculator", "gnome-calculator"),
    ("Image Viewer", "eog"),
];

/// True when the shell's own surfaces own the pointer at this location.
/// Used by the input router so client windows never steal panel clicks.
pub(crate) fn pointer_over_shell(state: &State, point: Point<f64, Logical>) -> bool {
    if !state.shell_visible {
        return false;
    }
    let (x, y) = (point.x, point.y);
    if y < 36.0 {
        return true;
    }
    if state.apps_open && x < 360.0 && y < 560.0 {
        return true;
    }
    if state.quick_open && x > state.output_size.w as f64 - 260.0 && y < 320.0 {
        return true;
    }
    false
}

/// Draw one frame of the shell.
pub(crate) fn draw(ctx: &Context, state: &mut State) {
    if !state.shell_visible {
        return;
    }
    top_panel(ctx, state);
    if state.apps_open {
        apps_menu(ctx, state);
    }
    if state.quick_open {
        quick_settings(ctx, state);
    }
}

fn launch(exe: &str) {
    if let Err(err) = std::process::Command::new(exe).spawn() {
        eprintln!("horn-compositor: could not launch {exe}: {err}");
    }
}

fn clock() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60)
}

fn top_panel(ctx: &Context, state: &mut State) {
    egui::Area::new(egui::Id::new("horn-top-panel"))
        .anchor(Align2::LEFT_TOP, Vec2::ZERO)
        .movable(false)
        .show(ctx, |ui| {
            Frame::new()
                .fill(ACCENT)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.style_mut().visuals.button_frame = false;
                        ui.style_mut().visuals.widgets.inactive.fg_stroke.color =
                            Color32::from_rgb(0x10, 0x10, 0x10);

                        if ui.button("⊞ Start").clicked() {
                            state.apps_open = !state.apps_open;
                            state.quick_open = false;
                        }
                        ui.separator();

                        // Window list: click the title to focus, ✕ to close, ▁ to minimize.
                        let windows: Vec<_> = state
                            .windows
                            .iter()
                            .filter(|w| {
                                !state.is_root_shell(&w.surface)
                                    && w.workspace == state.workspace
                                    && !w.minimized
                            })
                            .map(|w| {
                                (
                                    truncate(&w.title(), 18),
                                    state.focused.as_ref() == Some(w.surface.wl_surface()),
                                    w.surface.wl_surface().clone(),
                                )
                            })
                            .collect();
                        let mut closed = None;
                        for (title, focused, surface) in windows {
                            let label = if focused {
                                format!("▣ {title}")
                            } else {
                                format!("▢ {title}")
                            };
                            if ui.button(label).clicked() {
                                state.focus_window(&surface, SERIAL_COUNTER.next_serial());
                            }
                            if ui.button("▁").clicked() {
                                state.minimize_window(&surface, SERIAL_COUNTER.next_serial());
                            }
                            if ui.button("✕").clicked() {
                                closed = Some(surface);
                            }
                        }
                        if let Some(surface) = closed {
                            state.close_window(&surface);
                        }

                        ui.separator();
                        for ws in 0..WORKSPACES {
                            let selected = state.workspace == ws;
                            if ui
                                .selectable_label(selected, format!("{}", ws + 1))
                                .clicked()
                            {
                                state.set_workspace(ws, SERIAL_COUNTER.next_serial());
                            }
                        }

                        ui.separator();
                        ui.label(clock());
                        if ui.button("⚙").clicked() {
                            state.quick_open = !state.quick_open;
                            state.apps_open = false;
                        }
                    });
                });
        });
}

fn truncate(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        text.to_string()
    } else {
        format!("{}…", chars[..max].iter().collect::<String>())
    }
}

fn apps_menu(ctx: &Context, state: &mut State) {
    egui::Area::new(egui::Id::new("horn-apps-menu"))
        .anchor(Align2::LEFT_TOP, Vec2::new(0.0, 36.0))
        .movable(false)
        .show(ctx, |ui| {
            Frame::new()
                .fill(PANEL_BG)
                .stroke(egui::Stroke::new(1.0_f32, ACCENT))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(336.0_f32);
                    ui.horizontal(|ui| {
                        ui.heading("All apps");
                        if ui.button("✕").clicked() {
                            state.apps_open = false;
                        }
                    });
                    ui.separator();

                    ui.label("Launch");
                    egui::Grid::new("horn-app-grid")
                        .num_columns(2)
                        .spacing(Vec2::new(8.0, 8.0))
                        .show(ui, |ui| {
                            for (name, exe) in APPS {
                                if ui.button(*name).clicked() {
                                    launch(exe);
                                    state.apps_open = false;
                                }
                            }
                        });

                    ui.separator();
                    ui.label("Running windows");
                    let running: Vec<_> = state
                        .windows
                        .iter()
                        .filter(|w| !state.is_root_shell(&w.surface))
                        .map(|w| (truncate(&w.title(), 24), w.surface.wl_surface().clone()))
                        .collect();
                    let mut closed = None;
                    for (title, surface) in running {
                        ui.horizontal(|ui| {
                            if ui.button(format!("▣ {title}")).clicked() {
                                state.focus_window(&surface, SERIAL_COUNTER.next_serial());
                                state.apps_open = false;
                            }
                            if ui.button("✕").clicked() {
                                closed = Some(surface.clone());
                            }
                        });
                    }
                    if let Some(surface) = closed {
                        state.close_window(&surface);
                    }
                });
        });
}

fn quick_settings(ctx: &Context, state: &mut State) {
    egui::Area::new(egui::Id::new("horn-quick-settings"))
        .anchor(Align2::RIGHT_TOP, Vec2::new(0.0, 36.0))
        .movable(false)
        .show(ctx, |ui| {
            Frame::new()
                .fill(PANEL_BG)
                .stroke(egui::Stroke::new(1.0_f32, ACCENT))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(236.0_f32);
                    ui.horizontal(|ui| {
                        ui.heading("Quick settings");
                        if ui.button("✕").clicked() {
                            state.quick_open = false;
                        }
                    });
                    ui.separator();

                    ui.horizontal(|ui| {
                        if ui.selectable_label(state.wifi, "Wi-Fi").clicked() {
                            state.wifi = !state.wifi;
                        }
                        if ui.selectable_label(state.bluetooth, "Bluetooth").clicked() {
                            state.bluetooth = !state.bluetooth;
                        }
                    });
                    if ui
                        .selectable_label(state.airplane, "Airplane mode")
                        .clicked()
                    {
                        state.airplane = !state.airplane;
                        if state.airplane {
                            state.wifi = false;
                            state.bluetooth = false;
                        }
                    }
                    ui.add(
                        egui::Slider::new(&mut state.brightness, 0.05_f32..=1.0_f32)
                            .text("Brightness"),
                    );
                    ui.add(egui::Slider::new(&mut state.volume, 0.0_f32..=1.0_f32).text("Volume"));
                    ui.separator();
                    if ui.button("⏻  Shut down compositor").clicked() {
                        state.should_exit = true;
                    }
                });
        });
}
