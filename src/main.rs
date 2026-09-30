mod browser;
mod input;
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
    controls: [bool; 10],
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
                let mut controls = [false; 10];
                for (_, pad) in pads.gamepads() {
                    use gilrs::{Axis, Button};
                    let y = pad.value(Axis::LeftStickY);
                    controls[0] |= pad.is_pressed(Button::DPadUp) || y > 0.35;
                    controls[1] |= pad.is_pressed(Button::DPadDown) || y < -0.35;
                    let x = pad.value(Axis::LeftStickX);
                    controls[6] |= pad.is_pressed(Button::DPadLeft) || x < -0.35;
                    controls[7] |= pad.is_pressed(Button::DPadRight) || x > 0.35;
                    for (index, button) in
                        [Button::South, Button::East, Button::West, Button::Start]
                            .into_iter()
                            .enumerate()
                    {
                        controls[index + 2] |= pad.is_pressed(button);
                    }
                }
                let mut state = shared.lock().unwrap();
                if controls != state.controls {
                    state.controls = controls;
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
    web: web::Server,
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
        Self {
            browser,
            player,
            input: Input::new(Instant::now()),
            pads: Gamepads::new(cc.egui_ctx.clone()),
            ensure_visible: true,
            web: web::Server::new(settings.config.listen, cc.egui_ctx.clone()),
            settings,
        }
    }
    fn action(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Action::Fullscreen => toggle_fullscreen(ctx),
            Action::SeekBackward | Action::SeekForward | Action::PlayPause => {}
            Action::Back => {
                self.browser.back();
                self.ensure_visible = true;
            }
            Action::Refresh => {
                self.browser.refresh();
                self.ensure_visible = true;
            }
            Action::Up => {
                self.browser
                    .select(self.browser.view.selected.saturating_sub(1));
                self.ensure_visible = true;
            }
            Action::Down => {
                self.browser
                    .select(self.browser.view.selected.saturating_add(1));
                self.ensure_visible = true;
            }
            Action::Open => {
                if self.browser.error.is_some() {
                    self.browser.refresh();
                    return;
                }
                let Some(entry) = self
                    .browser
                    .entries
                    .get(self.browser.view.selected)
                    .cloned()
                else {
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
                        Ok(()) => self.input.block(),
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
        egui::Area::new("player_controls".into()).order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -12.0]).show(ctx, |ui| {
                egui::Frame::new().fill(egui::Color32::from_black_alpha(210)).inner_margin(12.0).show(ui, |ui| {
                    if status.loading { ui.label("Loading media…"); }
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("−10s").clicked() { self.playback_command(&["seek", "-10", "relative"]); }
                        if ui.button(if status.paused { "Play" } else { "Pause" }).clicked() { self.playback_command(&["cycle", "pause"]); }
                        if ui.button("+10s").clicked() { self.playback_command(&["seek", "10", "relative"]); }
                        if ui.button("Back").clicked() { self.playback_command(&["stop"]); self.input.block(); }
                        if let Some(position) = status.position {
                            let duration = status.duration.map_or_else(|| "Live".into(), elapsed);
                            ui.label(format!("{} / {duration}", elapsed(position)));
                        }
                    });
                    ui.label("A / Enter / Space: Pause · B / Esc: Back · ↑ ↓: Volume · ← →: Seek · F: Fullscreen · Start / Q: Quit");
                });
            });
    }
}
fn toggle_fullscreen(ctx: &egui::Context) {
    let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
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
            let result = if self.player.active() {
                Err("Playback is already active".into())
            } else {
                self.player
                    .launch(&self.settings, std::ffi::OsStr::new(request.url.as_ref()))
            };
            if result.is_ok() {
                self.input.block();
            }
            let _ = request.reply.send(result);
        }

        if let Some(result) = self.player.poll() {
            self.browser.refresh();
            if let Err(error) = result {
                eprintln!("{error}");
                self.browser.error = Some(error);
            }
            self.input.block();
            self.ensure_visible = true;
        }
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
        let mut controls = Controls(self.pads.state.lock().unwrap().controls);
        let keys = [
            egui::Key::ArrowUp,
            egui::Key::ArrowDown,
            egui::Key::Enter,
            egui::Key::Escape,
            egui::Key::R,
            egui::Key::Q,
            egui::Key::ArrowLeft,
            egui::Key::ArrowRight,
            egui::Key::Space,
            egui::Key::F,
        ];
        ctx.input(|i| {
            for (index, key) in keys.into_iter().enumerate() {
                controls.0[index] |= i.key_down(key);
            }
        });
        // A cheap timer enables the neutral gate and repeat without rendering continuously when idle.
        if self.input.waiting_for_neutral() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if controls.0[0] || controls.0[1] {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        let actions = self.input.update(controls, Instant::now());
        if self.player.active() {
            for action in actions {
                match action {
                    Action::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Action::Back => {
                        self.playback_command(&["stop"]);
                        self.input.block();
                    }
                    Action::Open | Action::PlayPause => self.playback_command(&["cycle", "pause"]),
                    Action::SeekBackward => self.playback_command(&["seek", "-10", "relative"]),
                    Action::SeekForward => self.playback_command(&["seek", "10", "relative"]),
                    Action::Fullscreen => toggle_fullscreen(ctx),
                    Action::Up => self.playback_command(&["add", "volume", "5"]),
                    Action::Down => self.playback_command(&["add", "volume", "-5"]),
                    Action::Refresh => {}
                }
            }
            self.playback_ui(ctx);
            return;
        }
        for action in actions {
            self.action(action, ctx);
            if self.player.active() {
                ctx.request_repaint();
                break;
            }
        }
        if self.player.active() {
            return;
        }
        egui::TopBottomPanel::top("path").show(ctx, |ui| {
            ui.add_space(12.0);
            ui.heading("Media Launcher");
            if self.browser.configured_root.is_none() {
                ui.label("Waiting for media from your phone");
                ui.label(format!(
                    "Open http://THIS-COMPUTER-IP:{}/ on your phone",
                    self.settings.config.listen.port()
                ));
            }
            ui.add(egui::Label::new(self.browser.current.to_string_lossy()).wrap());
            ui.add_space(8.0);
        });
        egui::TopBottomPanel::bottom("controls").show(ctx, |ui| {
            if let Some(entry) = self.browser.entries.get(self.browser.view.selected) {
                egui::ScrollArea::vertical()
                    .id_salt("detail")
                    .max_height(140.0)
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(entry.name.to_string_lossy()).wrap());
                    });
            }
            ui.separator();
            ui.label("↑ ↓  Move    A / Enter  Open    B / Esc  Back");
            ui.label("X / R  Refresh    Start / Q  Quit");
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(error) = &self.pads.state.lock().unwrap().error {
                ui.colored_label(egui::Color32::YELLOW, error);
            }
            if let Some(error) = &self.browser.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
                ui.label("A / Enter or X / R to retry · B / Esc to dismiss · Start / Q to quit");
            }
            if self.browser.entries.is_empty() {
                if self.browser.configured_root.is_none() {
                    ui.label("To browse local videos, set root in config.toml or launch with --root PATH.");
                    ui.label(format!("Configuration: {}", self.settings.config_path.display()));
                } else {
                    ui.label("No visible folders or supported videos in this directory.");
                }
            }
            let mut clicked = None;
            let output = egui::ScrollArea::vertical()
                .id_salt("files")
                .vertical_scroll_offset(self.browser.view.scroll)
                .auto_shrink([false, false])
                .show_rows(ui, 52.0, self.browser.entries.len(), |ui, range| {
                    if self.ensure_visible {
                        let y = (self.browser.view.selected as f32 - range.start as f32)
                            * (52.0 + ui.spacing().item_spacing.y);
                        ui.scroll_to_rect(
                            egui::Rect::from_min_size(
                                ui.max_rect().min + egui::vec2(0.0, y),
                                egui::vec2(1.0, 52.0),
                            ),
                            None,
                        );
                    }
                    for index in range {
                        let entry = &self.browser.entries[index];
                        let selected = index == self.browser.view.selected;
                        let text = format!(
                            "{}  {}",
                            if entry.directory { "[DIR]" } else { "[VIDEO]" },
                            entry.name.to_string_lossy()
                        );
                        let button =
                            egui::Button::new(egui::RichText::new(text).color(if selected {
                                egui::Color32::BLACK
                            } else {
                                egui::Color32::WHITE
                            }))
                            .truncate()
                            .fill(if selected {
                                egui::Color32::from_rgb(120, 200, 255)
                            } else {
                                egui::Color32::from_gray(28)
                            });
                        if ui.add_sized([ui.available_width(), 52.0], button).clicked() {
                            clicked = Some(index);
                        }
                    }
                });
            self.browser.view.scroll = output.state.offset.y;
            self.ensure_visible = false;
            if let Some(index) = clicked {
                self.browser.select(index);
                self.action(Action::Open, ctx);
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
    let window = &settings.config.window;
    let viewport = egui::ViewportBuilder::default()
        .with_title("Media Launcher")
        .with_fullscreen(window.mode == config::WindowMode::Fullscreen)
        .with_maximized(window.mode == config::WindowMode::Maximized)
        .with_inner_size(window.size)
        .with_decorations(window.decorations)
        .with_resizable(window.resizable)
        .with_app_id("media-launcher");
    eframe::run_native(
        "media-launcher",
        eframe::NativeOptions {
            viewport,
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(App::new(cc, settings)))),
    )
}
