use super::{App, config, input::Action};
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Menu,
    Settings,
    Bindings,
    Rebind(usize),
}
pub struct Preferences {
    pub mode: config::WindowMode,
    original_mode: config::WindowMode,
    pub scale: f32,
    pub audio: config::AudioConfig,
    pub controls: config::ControllerConfig,
}
const BINDINGS: [&str; 7] = [
    "Confirm",
    "Back",
    "Refresh",
    "Menu",
    "Play / pause",
    "Fullscreen",
    "Mute",
];

fn apply_window(ctx: &egui::Context, mode: config::WindowMode, size: [f32; 2]) {
    // Fullscreen receives no subsequent windowed geometry requests.
    match mode {
        config::WindowMode::Fullscreen => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true))
        }
        config::WindowMode::Maximized => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        }
        config::WindowMode::Windowed => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size.into()));
        }
    }
}
impl App {
    pub(super) fn cancel_preferences(&mut self, ctx: &egui::Context) {
        if let Some(previous) = self.preferences.take() {
            ctx.set_zoom_factor(self.settings.config.ui_scale);
            apply_window(
                ctx,
                previous.original_mode,
                self.settings.config.window.size,
            );
            self.apply_audio();
        }
        self.menu_error = None;
    }
    fn open_preferences(&mut self, ctx: &egui::Context) {
        let mode = ctx.input(|input| {
            if input.viewport().fullscreen.unwrap_or(false) {
                config::WindowMode::Fullscreen
            } else if input.viewport().maximized.unwrap_or(false) {
                config::WindowMode::Maximized
            } else {
                config::WindowMode::Windowed
            }
        });
        self.preferences = Some(Preferences {
            mode,
            original_mode: mode,
            scale: self.settings.config.ui_scale,
            audio: self.settings.config.audio.clone(),
            controls: self.settings.config.controls.clone(),
        });
        self.panel = Some(Panel::Settings);
        self.menu_selected = 0;
        self.input.block();
    }
    fn panel_len(&self) -> usize {
        match self.panel {
            Some(Panel::Menu) => {
                if self.player.active() {
                    4
                } else {
                    3
                }
            }
            Some(Panel::Settings) => 7,
            Some(Panel::Bindings) => 9,
            _ => 0,
        }
    }
    pub(super) fn panel_action(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Up => self.menu_selected = self.menu_selected.saturating_sub(1),
            Action::Down => {
                self.menu_selected =
                    (self.menu_selected + 1).min(self.panel_len().saturating_sub(1))
            }
            Action::Back => {
                if self.panel == Some(Panel::Bindings) {
                    self.panel = Some(Panel::Settings);
                    self.menu_selected = 4;
                } else if self.panel == Some(Panel::Settings) {
                    self.cancel_preferences(ctx);
                    self.panel = Some(Panel::Menu);
                    self.menu_selected = 1;
                } else {
                    self.panel = None;
                }
                self.input.block();
            }
            Action::Left => self.adjust(-1, ctx),
            Action::Right => self.adjust(1, ctx),
            Action::Confirm => self.activate(ctx),
            _ => {}
        }
    }
    fn adjust(&mut self, direction: i32, ctx: &egui::Context) {
        if self.panel != Some(Panel::Settings) {
            return;
        }
        let Some(draft) = self.preferences.as_mut() else {
            return;
        };
        match self.menu_selected {
            0 => {
                let modes = [
                    config::WindowMode::Fullscreen,
                    config::WindowMode::Windowed,
                    config::WindowMode::Maximized,
                ];
                let index = modes.iter().position(|m| *m == draft.mode).unwrap() as i32;
                draft.mode = modes[(index + direction).rem_euclid(3) as usize];
                apply_window(ctx, draft.mode, self.settings.config.window.size);
            }
            1 => {
                draft.scale =
                    ((draft.scale * 10.0 + direction as f32).round() / 10.0).clamp(0.5, 4.0);
                ctx.set_zoom_factor(draft.scale);
            }
            2 => {
                draft.audio.volume =
                    (draft.audio.volume + direction as f64 * 5.0).clamp(0.0, 100.0);
                self.apply_audio();
            }
            3 => {
                draft.audio.muted = !draft.audio.muted;
                self.apply_audio();
            }
            _ => {}
        }
    }
    fn activate(&mut self, ctx: &egui::Context) {
        match self.panel {
            Some(Panel::Menu) => match self.menu_selected {
                0 => {
                    self.panel = None;
                    self.input.block();
                }
                1 => self.open_preferences(ctx),
                2 if self.player.active() => {
                    self.cancel_preferences(ctx);
                    self.action(Action::Stop, ctx);
                }
                _ => self.action(Action::Quit, ctx),
            },
            Some(Panel::Settings) => match self.menu_selected {
                0..=3 => self.adjust(1, ctx),
                4 => {
                    self.panel = Some(Panel::Bindings);
                    self.menu_selected = 0;
                    self.input.block();
                }
                5 => {
                    let draft = self.preferences.as_ref().unwrap();
                    match self.settings.save_preferences(
                        draft.mode,
                        draft.scale,
                        &draft.audio,
                        &draft.controls,
                    ) {
                        Ok(()) => {
                            let draft = self.preferences.take().unwrap();
                            self.settings.config.window.mode = draft.mode;
                            self.settings.config.ui_scale = draft.scale;
                            self.settings.config.audio = draft.audio;
                            self.settings.config.controls = draft.controls;
                            self.panel = Some(Panel::Menu);
                            self.menu_selected = 1;
                            self.menu_error = None;
                            self.input.block();
                        }
                        Err(error) => {
                            self.menu_error = Some(format!("Could not save settings: {error}"))
                        }
                    }
                }
                _ => self.panel_action(Action::Back, ctx),
            },
            Some(Panel::Bindings) => match self.menu_selected {
                0..=6 => {
                    self.panel = Some(Panel::Rebind(self.menu_selected));
                    self.rebind_armed = false;
                    self.input.block();
                }
                7 => {
                    self.preferences.as_mut().unwrap().controls =
                        config::ControllerConfig::default();
                    self.input.block();
                }
                _ => self.panel_action(Action::Back, ctx),
            },
            _ => {}
        }
    }
    pub(super) fn capture_binding(
        &mut self,
        index: usize,
        buttons: [bool; 10],
        ctx: &egui::Context,
    ) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) || (self.rebind_armed && buttons[1]) {
            self.panel = Some(Panel::Bindings);
            self.input.block();
            return;
        }
        if !self.rebind_armed {
            self.rebind_armed = !buttons.iter().any(|b| *b);
            return;
        }
        if let Some(pressed) = buttons.iter().position(|b| *b) {
            let button = config::ControllerButton::ALL[pressed];
            let bindings = &mut self.preferences.as_mut().unwrap().controls.buttons;
            rebind(bindings, index, button);
            self.panel = Some(Panel::Bindings);
            self.input.block();
        }
    }
    pub(super) fn draw_panel(&mut self, ctx: &egui::Context) {
        let Some(panel) = self.panel else {
            return;
        };
        let title = match panel {
            Panel::Menu => "Menu",
            Panel::Settings => "Settings",
            Panel::Bindings => "Controller buttons",
            Panel::Rebind(_) => "Assign button",
        };
        let rows: Vec<String> = match panel {
            Panel::Menu => if self.player.active() {
                vec!["Resume", "Settings", "Stop playback", "Exit application"]
            } else {
                vec!["Back to launcher", "Settings", "Exit application"]
            }
            .into_iter()
            .map(str::to_owned)
            .collect(),
            Panel::Settings => {
                let p = self.preferences.as_ref().unwrap();
                vec![
                    format!("Window: {:?}", p.mode),
                    format!("UI scale: {:.1}", p.scale),
                    format!("Volume: {:.0}%", p.audio.volume),
                    format!("Mute: {}", if p.audio.muted { "On" } else { "Off" }),
                    "Controller buttons".into(),
                    "Save settings".into(),
                    "Cancel".into(),
                ]
            }
            Panel::Bindings => {
                let mut rows: Vec<_> = BINDINGS
                    .iter()
                    .zip(self.preferences.as_ref().unwrap().controls.buttons)
                    .map(|(name, button)| format!("{name}: {}", button.label()))
                    .collect();
                rows.extend(["Reset buttons to defaults".into(), "Back".into()]);
                rows
            }
            Panel::Rebind(_) => vec![],
        };
        if !matches!(panel, Panel::Rebind(_)) {
            self.menu_selected = self.menu_selected.min(rows.len().saturating_sub(1));
        }
        let mut clicked = None;
        egui::Window::new(title).collapsible(false).resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).default_width(480.0)
            .show(ctx, |ui| {
                if let Panel::Rebind(index) = panel {
                    ui.label(format!("Release buttons, then press a button for {}.", BINDINGS[index]));
                    ui.label("Escape or physical B cancels. An assigned button swaps with its previous action.");
                    if ui.button("Cancel").clicked_by(egui::PointerButton::Primary) { clicked = Some(usize::MAX); }
                } else {
                    egui::ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.65).show(ui, |ui| {
                        for (index, label) in rows.iter().enumerate() {
                            let response = ui.add_sized([ui.available_width(), 48.0], egui::Button::new(label).selected(index == self.menu_selected));
                            if index == self.menu_selected { response.scroll_to_me(None); }
                            if response.clicked_by(egui::PointerButton::Primary) { clicked = Some(index); }
                        }
                    });
                    ui.label("Up / Down: Navigate · Left / Right: Adjust");
                    let controls = self.preferences.as_ref().map_or(&self.settings.config.controls, |p| &p.controls);
                    ui.label(format!("{} / Enter: Select · {} / Esc: Back", controls.buttons[0].label(), controls.buttons[1].label()));
                    if panel == Panel::Settings { ui.small("Save keeps these preferences. Cancel restores previous settings."); }
                }
                if let Some(error) = &self.menu_error { ui.colored_label(egui::Color32::LIGHT_RED, error); }
            });
        if let Some(index) = clicked {
            if index == usize::MAX {
                self.panel = Some(Panel::Bindings);
                self.input.block();
            } else {
                self.menu_selected = index;
                self.activate(ctx);
            }
        }
    }
}
fn rebind(
    bindings: &mut [config::ControllerButton; 7],
    index: usize,
    button: config::ControllerButton,
) {
    if let Some(previous) = bindings.iter().position(|b| *b == button) {
        bindings.swap(index, previous);
    } else {
        bindings[index] = button;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rebinding_swaps_to_keep_every_action_reachable() {
        let mut bindings = config::ControllerConfig::default().buttons;
        rebind(&mut bindings, 0, config::ControllerButton::Start);
        assert_eq!(bindings[0], config::ControllerButton::Start);
        assert_eq!(bindings[3], config::ControllerButton::South);
        for (index, button) in bindings.iter().enumerate() {
            assert!(!bindings[..index].contains(button));
        }
    }
}
