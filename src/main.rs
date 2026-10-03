mod browser;
mod browser_ui;
mod input;
mod menu;
mod phone;
mod web;

use eframe::egui;
use input::{Action, Controls, Input};
use media_launcher::{config, player};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct PadState {
    controls: [bool; 4],
    buttons: [bool; 10],
    pressed_controls: [bool; 4],
    pressed_buttons: [bool; 10],
    error: Option<String>,
}
struct Gamepads {
    state: Arc<Mutex<PadState>>,
    stop: Arc<AtomicBool>,
}
impl Gamepads {
    fn new(ctx: egui::Context) -> Self {
        let state = Arc::new(Mutex::new(PadState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (shared, stopped) = (state.clone(), stop.clone());
        std::thread::spawn(move || {
            let mut pads = match gilrs::Gilrs::new() {
                Ok(pads) => pads,
                Err(e) => {
                    shared.lock().unwrap().error = Some(format!(
                        "Controller input unavailable: {e}. Keyboard controls remain available."
                    ));
                    ctx.request_repaint();
                    return;
                }
            };
            while !stopped.load(Ordering::Relaxed) {
                while pads.next_event().is_some() {}
                let mut controls = [false; 4];
                let mut buttons = [false; 10];
                for (_, pad) in pads.gamepads() {
                    use gilrs::{Axis, Button};
                    let y = pad.value(Axis::LeftStickY);
                    let x = pad.value(Axis::LeftStickX);
                    let stick = input::stick_direction(x, y);
                    for (index, button) in [
                        Button::DPadUp,
                        Button::DPadDown,
                        Button::DPadLeft,
                        Button::DPadRight,
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        controls[index] |= pad.is_pressed(button) || stick == Some(index);
                    }
                    let physical = [
                        Button::South,
                        Button::East,
                        Button::West,
                        Button::North,
                        Button::Start,
                        Button::Select,
                        Button::LeftTrigger,
                        Button::RightTrigger,
                        Button::LeftThumb,
                        Button::RightThumb,
                    ];
                    for (index, button) in physical.into_iter().enumerate() {
                        buttons[index] |= pad.is_pressed(button);
                    }
                }
                let mut state = shared.lock().unwrap();
                if controls != state.controls || buttons != state.buttons {
                    for (index, pressed) in controls.iter().enumerate() {
                        state.pressed_controls[index] |= *pressed && !state.controls[index];
                    }
                    for (index, pressed) in buttons.iter().enumerate() {
                        state.pressed_buttons[index] |= *pressed && !state.buttons[index];
                    }
                    state.controls = controls;
                    state.buttons = buttons;
                    ctx.request_repaint();
                }
                drop(state);
                std::thread::sleep(Duration::from_millis(8));
            }
        });
        Self { state, stop }
    }
}
impl Drop for Gamepads {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

struct App {
    browser: browser::Browser,
    settings: config::Settings,
    player: player::Player,
    input: Input,
    pads: Gamepads,
    ensure_visible: bool,
    camera: browser_ui::Camera,
    web: web::Server,
    phone: phone::PhoneLink,
    panel: Option<menu::Panel>,
    menu_selected: usize,
    preferences: Option<menu::Preferences>,
    rebind_armed: bool,
    menu_error: Option<String>,
    overlay_activity: Instant,
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>, settings: config::Settings) -> Self {
        cc.egui_ctx.set_zoom_factor(settings.config.ui_scale);
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut style = (*cc.egui_ctx.style()).clone();
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(30.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(30.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(36.0));
        style.spacing.item_spacing = egui::vec2(12.0, 12.0);
        cc.egui_ctx.set_style(style);
        let player = player::Player::new(&settings, cc);
        let mut browser = browser::Browser::new(settings.config.root.clone());
        if let Some(error) = player.startup_error() {
            browser.error = Some(error.into());
        }
        let mut app = Self {
            browser,
            player,
            input: Input::new(Instant::now()),
            pads: Gamepads::new(cc.egui_ctx.clone()),
            ensure_visible: true,
            camera: browser_ui::Camera::default(),
            web: web::Server::new(settings.config.listen, cc.egui_ctx.clone()),
            settings,
            panel: None,
            menu_selected: 0,
            preferences: None,
            rebind_armed: false,
            menu_error: None,
            overlay_activity: Instant::now(),
            phone: phone::PhoneLink::default(),
        };
        app.apply_audio();
        app
    }
    fn action(&mut self, action: Action, ctx: &egui::Context) {
        self.overlay_activity = Instant::now();
        if action == Action::Quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if action == Action::Menu {
            if self.panel.is_some() {
                self.cancel_preferences(ctx);
                self.panel = None;
            } else {
                self.panel = Some(menu::Panel::Menu);
                self.menu_selected = 0;
            }
            self.input.block();
            return;
        }
        // Explicit playback actions from remote/UI are independent of navigation.
        match action {
            Action::PlayPause if self.player.active() => {
                self.playback_command(&["cycle", "pause"]);
                return;
            }
            Action::Stop => {
                self.cancel_preferences(ctx);
                self.playback_command(&["stop"]);
                self.panel = None;
                self.input.block();
                return;
            }
            Action::Seek(seconds) => {
                self.playback_command(&["seek", &seconds.to_string(), "relative"]);
                return;
            }
            Action::Volume(amount) => {
                let audio = self
                    .preferences
                    .as_mut()
                    .map_or(&mut self.settings.config.audio, |p| &mut p.audio);
                audio.volume = (audio.volume + amount as f64).clamp(0.0, 100.0);
                self.apply_audio();
                return;
            }
            Action::Mute => {
                let audio = self
                    .preferences
                    .as_mut()
                    .map_or(&mut self.settings.config.audio, |p| &mut p.audio);
                audio.muted = !audio.muted;
                self.apply_audio();
                return;
            }
            _ => {}
        }
        if self.panel.is_some() {
            self.panel_action(action, ctx);
            return;
        }
        if self.player.active() {
            match action {
                Action::Back => self.action(Action::Stop, ctx),
                Action::Confirm => self.action(Action::PlayPause, ctx),
                Action::Left => self.action(Action::Seek(-10), ctx),
                Action::Right => self.action(Action::Seek(10), ctx),
                Action::Up => self.action(Action::Volume(5), ctx),
                Action::Down => self.action(Action::Volume(-5), ctx),
                Action::Fullscreen => toggle_fullscreen(ctx, &self.settings.config.window),
                _ => {}
            }
            return;
        }

        match action {
            Action::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Action::Fullscreen => toggle_fullscreen(ctx, &self.settings.config.window),
            Action::PlayPause
            | Action::Menu
            | Action::Stop
            | Action::Seek(_)
            | Action::Volume(_)
            | Action::Mute => {}
            Action::Back | Action::Left => {
                self.browser.back();
                self.ensure_visible = true;
            }
            Action::Refresh => {
                self.browser.refresh();
                self.ensure_visible = true;
            }
            Action::Up => {
                self.browser
                    .select(self.browser.selected().saturating_sub(1));
                self.ensure_visible = true;
            }
            Action::Down => {
                self.browser
                    .select(self.browser.selected().saturating_add(1));
                self.ensure_visible = true;
            }
            Action::Confirm | Action::Right => {
                if self.browser.error.is_some() {
                    self.browser.refresh();
                    return;
                }
                let Some(entry) = self.browser.selected_entry().cloned() else {
                    return;
                };
                if entry.directory {
                    self.browser.navigate(entry.path);
                    self.ensure_visible = true;
                } else {
                    let result = self
                        .browser
                        .media_path(&entry)
                        .and_then(|path| self.player.launch(&self.settings, path.as_os_str()));
                    match result {
                        Ok(()) => {
                            self.overlay_activity = Instant::now();
                            self.apply_audio();
                            self.input.block();
                        }
                        Err(e) => {
                            eprintln!("{e}");
                            self.browser.error = Some(e);
                        }
                    }
                }
            }
        }
    }
}
impl App {
    fn play_url(&mut self, url: &str, ctx: &egui::Context) -> Result<(), String> {
        if self.player.active() {
            return Err("Playback is already active".into());
        }
        self.player
            .launch(&self.settings, std::ffi::OsStr::new(url))?;
        self.overlay_activity = Instant::now();
        self.cancel_preferences(ctx);
        self.panel = None;
        self.browser.error = None;
        self.apply_audio();
        self.input.block();
        Ok(())
    }
    fn apply_audio(&mut self) {
        if self.player.is_embedded() {
            let audio = self
                .preferences
                .as_ref()
                .map_or(&self.settings.config.audio, |p| &p.audio);
            let volume = audio.volume.to_string();
            let mute = if audio.muted { "yes" } else { "no" };
            self.playback_command(&["set", "volume", &volume]);
            self.playback_command(&["set", "mute", mute]);
        }
    }
    fn paint_video(&self, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_millis(100));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                self.player.paint(ui);
            });
    }
    fn playback_command(&mut self, args: &[&str]) {
        if let Err(error) = self.player.command(args) {
            eprintln!("{error}");
            self.browser.error = Some(error);
        }
    }
    fn playback_ui(&mut self, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_millis(100));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                self.player.paint(ui);
            });
        let status = self.player.status();
        if !status.paused
            && !status.loading
            && self.overlay_activity.elapsed() >= Duration::from_secs(3)
        {
            ctx.set_cursor_icon(egui::CursorIcon::None);
            return;
        }
        egui::Area::new("player_controls".into()).order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -12.0]).show(ctx, |ui| {
                egui::Frame::new().fill(egui::Color32::from_black_alpha(210)).inner_margin(12.0).show(ui, |ui| {
                    if status.loading { ui.label("Loading media…"); }
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("-10s").clicked_by(egui::PointerButton::Primary) { self.action(Action::Seek(-10), ctx); }
                        if ui.button(if status.paused { "Play" } else { "Pause" }).clicked_by(egui::PointerButton::Primary) { self.action(Action::PlayPause, ctx); }
                        if ui.button("+10s").clicked_by(egui::PointerButton::Primary) { self.action(Action::Seek(10), ctx); }
                        if ui.button("Back").clicked_by(egui::PointerButton::Primary) { self.action(Action::Stop, ctx); }
                        if ui.button("Menu").clicked_by(egui::PointerButton::Primary) { self.action(Action::Menu, ctx); }
                        if let Some(position) = status.position {
                            let duration = status.duration.map_or_else(|| "Live".into(), elapsed);
                            ui.label(format!("{} / {duration}", elapsed(position)));
                        }
                    });
                    ui.label(format!("Volume {:.0}%{}", status.volume, if status.muted { " · Muted" } else { "" }));
                    ui.label("A / Enter / Space: Pause · B / Esc: Back · D-pad Up/Down: Volume · D-pad Left/Right: Seek · F: Fullscreen · Start / M: Menu · Q: Quit");
                });
            });
    }
}
fn keyboard_controls(ctx: &egui::Context, controls: &mut Controls, pressed: &mut Controls) {
    let keys = [
        egui::Key::ArrowUp,
        egui::Key::ArrowDown,
        egui::Key::Enter,
        egui::Key::Escape,
        egui::Key::R,
        egui::Key::M,
        egui::Key::ArrowLeft,
        egui::Key::ArrowRight,
        egui::Key::Space,
        egui::Key::F,
        egui::Key::Q,
        egui::Key::V,
    ];
    ctx.input(|input| {
        for (index, key) in keys.into_iter().enumerate() {
            // Preserve a tap whose down/up events both arrived between video frames.
            let edge = input.events.iter().any(|event| matches!(event,
                egui::Event::Key { key: event_key, pressed: true, repeat: false, .. } if *event_key == key));
            pressed.0[index] |= edge;
            controls.0[index] |= input.key_down(key) || edge;
        }
    });
}
fn toggle_fullscreen(ctx: &egui::Context, window: &config::WindowConfig) {
    let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
    if fullscreen {
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(window.size.into()));
    }
}
fn elapsed(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.player.close();
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        while let Some(request) = self.web.try_recv() {
            let result = match request.command {
                web::Command::Play(url) => self.play_url(&url, ctx),
                web::Command::Action(action) => {
                    if self.player.active() && self.player.is_embedded() {
                        self.action(action, ctx);
                        Ok(())
                    } else {
                        Err("Embedded playback is not active".into())
                    }
                }
            };
            let _ = request.reply.send(result);
        }

        if let Some(result) = self.player.poll() {
            if self.panel == Some(menu::Panel::Menu) {
                self.menu_selected = 0;
            }
            if let Err(error) = result {
                eprintln!("{error}");
                self.browser.error = Some(error);
            }
            self.input.block();
            self.ensure_visible = true;
        }
        let status = self.player.status();
        if self.player.active()
            && self.player.is_embedded()
            && self.preferences.is_none()
            && !status.loading
        {
            self.settings.config.audio.volume = status.volume.clamp(0.0, 100.0);
            self.settings.config.audio.muted = status.muted;
        }
        self.web.publish(web::Status {
            active: self.player.active(),
            embedded: self.player.is_embedded(),
            loading: status.loading,
            paused: status.paused,
            position: status.position,
            duration: status.duration,
            volume: status.volume,
            muted: status.muted,
            error: self.browser.error.is_some(),
        });
        if self.player.active() && !self.player.is_embedded() {
            if ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            ctx.request_repaint_after(Duration::from_millis(100));
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Playing in external mpv");
                ui.label("The browser will resume when playback ends.");
            });
            return;
        }
        let mut state = self.pads.state.lock().unwrap();
        let pressed = std::mem::take(&mut state.pressed_buttons);
        let raw_buttons = std::array::from_fn(|index| state.buttons[index] || pressed[index]);
        let directions = std::mem::take(&mut state.pressed_controls);
        let mut controls = Controls::default();
        let mut edges = Controls::default();
        for (index, slot) in [0, 1, 6, 7].into_iter().enumerate() {
            controls.0[slot] = state.controls[index] || directions[index];
            edges.0[slot] = directions[index];
        }
        drop(state);
        let bindings = self
            .preferences
            .as_ref()
            .map_or(&self.settings.config.controls, |draft| &draft.controls);
        for (slot, button) in [2, 3, 4, 5, 8, 9, 11].into_iter().zip(bindings.buttons) {
            let index = config::ControllerButton::ALL
                .iter()
                .position(|b| *b == button)
                .unwrap();
            controls.0[slot] |= raw_buttons[index];
            edges.0[slot] = pressed[index];
        }
        keyboard_controls(ctx, &mut controls, &mut edges);
        // Held buttons keep controls visible. Pointer movement and all key presses
        // also reveal them, including taps that arrive between video frames.
        let local_activity = controls.0.iter().any(|held| *held)
            || raw_buttons.iter().any(|held| *held)
            || ctx.input(|input| {
                !input.keys_down.is_empty()
                    || input.pointer.any_down()
                    || input.events.iter().any(|event| {
                        matches!(
                            event,
                            egui::Event::PointerMoved(_)
                                | egui::Event::MouseMoved(_)
                                | egui::Event::PointerButton { pressed: true, .. }
                                | egui::Event::MouseWheel { .. }
                                | egui::Event::Key { pressed: true, .. }
                        )
                    })
            });
        if local_activity || status.paused || status.loading {
            self.overlay_activity = Instant::now();
        }
        if let Some(menu::Panel::Rebind(index)) = self.panel {
            self.capture_binding(index, raw_buttons, ctx);
            if self.player.active() {
                self.paint_video(ctx);
            } else {
                egui::CentralPanel::default().show(ctx, |_| {});
            }
            self.draw_panel(ctx);
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        // A cheap timer enables the neutral gate and repeat without rendering continuously when idle.
        if self.input.waiting_for_neutral() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if controls.0[0] || controls.0[1] {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        let actions = self
            .input
            .update_with_edges(controls, edges, Instant::now());
        for action in actions {
            self.action(action, ctx);
            if self.input.waiting_for_neutral() {
                break;
            }
        }
        if self.panel.is_some() {
            if self.player.active() {
                self.paint_video(ctx);
            } else {
                egui::CentralPanel::default().show(ctx, |_| {});
            }
            self.draw_panel(ctx);
            return;
        }
        if self.player.active() {
            self.playback_ui(ctx);
            return;
        }
        self.phone.refresh(self.web.endpoint());
        ctx.request_repaint_after(Duration::from_secs(10));
        egui::TopBottomPanel::top("header")
            .exact_height(116.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let name = self
                        .browser
                        .selected_entry()
                        .map(|e| {
                            e.name
                                .to_string_lossy()
                                .chars()
                                .take(180)
                                .collect::<String>()
                        })
                        .unwrap_or_default();
                    let phone_width = 360.0_f32.min(ui.available_width() * 0.45);
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width() - phone_width, 96.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add(egui::Label::new(name).truncate());
                        },
                    );
                    self.phone.show_header(ui, 96.0);
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(error) = &self.pads.state.lock().unwrap().error {
                ui.colored_label(egui::Color32::YELLOW, error);
            }
            if let Some(error) = &self.browser.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            let clicked =
                browser_ui::show(ui, &mut self.browser, &mut self.camera, self.ensure_visible);
            self.ensure_visible = false;
            if let Some(index) = clicked {
                self.browser.select(index);
                self.action(Action::Confirm, ctx);
            }
        });
    }
}

fn main() -> eframe::Result {
    let settings = match config::load() {
        Ok(Some(settings)) => settings,
        Ok(None) => return Ok(()),
        Err(e) => {
            eprintln!("{e}\nUse --help for usage.");
            std::process::exit(2);
        }
    };
    let options = window_options(&settings.config.window);
    eframe::run_native(
        "media-launcher",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, settings)))),
    )
}

fn window_options(window: &config::WindowConfig) -> eframe::NativeOptions {
    let viewport = egui::ViewportBuilder::default()
        .with_title("Media Launcher")
        .with_decorations(window.decorations)
        .with_resizable(window.resizable)
        .with_app_id("media-launcher");
    // egui-winit reapplies size and maximized after window creation. On Windows
    // those can shrink a fullscreen window without restoring its window borders.
    let viewport = match window.mode {
        config::WindowMode::Fullscreen => viewport.with_fullscreen(true),
        config::WindowMode::Maximized => viewport.with_maximized(true),
        config::WindowMode::Windowed => viewport.with_inner_size(window.size),
    };
    eframe::NativeOptions {
        viewport,
        centered: window.mode == config::WindowMode::Windowed,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;

    #[test]
    fn startup_mode_does_not_apply_windowed_geometry_to_fullscreen() {
        let mut window = config::WindowConfig::default();
        let options = window_options(&window);
        assert_eq!(options.viewport.fullscreen, Some(true));
        assert_eq!(options.viewport.inner_size, None);
        assert_eq!(options.viewport.maximized, None);

        window.mode = config::WindowMode::Maximized;
        let options = window_options(&window);
        assert_eq!(options.viewport.maximized, Some(true));
        assert_eq!(options.viewport.inner_size, None);

        window.mode = config::WindowMode::Windowed;
        let options = window_options(&window);
        assert!(options.centered);
        assert_eq!(options.viewport.inner_size, Some(window.size.into()));
        assert_eq!(options.viewport.decorations, Some(true));
        assert_eq!(options.viewport.resizable, Some(true));
    }
    #[test]
    fn keyboard_taps_between_frames_are_not_lost() {
        let context = egui::Context::default();
        let mut controls = Controls::default();
        let mut edges = Controls::default();
        let events = [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Default::default(),
            })
            .collect();
        let _ = context.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                keyboard_controls(ctx, &mut controls, &mut edges);
            },
        );
        assert!(controls.0[3]);
    }
}
