mod browser;
mod input;
mod player;

use eframe::egui;
use input::{Action, Controls, Input};
use std::{
    ffi::OsString,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

struct Options {
    root: PathBuf,
    mpv: OsString,
    scale: f32,
}
impl Options {
    fn parse() -> Result<Option<Self>, String> {
        let mut options = Self {
            root: "/srv/downloads/complete".into(),
            mpv: "/run/current-system/sw/bin/mpv".into(),
            scale: 1.0,
        };
        let mut args = std::env::args_os().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--help" || arg == "-h" {
                println!(
                    "media-launcher [--root PATH] [--mpv EXECUTABLE] [--ui-scale NUMBER]\nDefaults: /srv/downloads/complete, /run/current-system/sw/bin/mpv, 1.0\nUI scale must be between 0.5 and 4.0."
                );
                return Ok(None);
            }
            match arg.to_str() {
                Some("--root") => {
                    options.root = args.next().ok_or("--root requires a path")?.into()
                }
                Some("--mpv") => options.mpv = args.next().ok_or("--mpv requires an executable")?,
                Some("--ui-scale") => {
                    options.scale = args
                        .next()
                        .and_then(|v| v.to_str().and_then(|s| s.parse::<f32>().ok()))
                        .ok_or("--ui-scale requires a number")?;
                    if !options.scale.is_finite() || !(0.5..=4.0).contains(&options.scale) {
                        return Err("--ui-scale must be between 0.5 and 4.0".into());
                    }
                }
                _ => return Err(format!("Unknown option: {}", arg.to_string_lossy())),
            }
        }
        if !options.root.is_absolute() {
            options.root = std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(options.root);
        }
        Ok(Some(options))
    }
}

#[derive(Default)]
struct PadState {
    controls: [bool; 6],
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
                let mut controls = [false; 6];
                for (_, pad) in pads.gamepads() {
                    use gilrs::{Axis, Button};
                    let y = pad.value(Axis::LeftStickY);
                    controls[0] |= pad.is_pressed(Button::DPadUp) || y > 0.35;
                    controls[1] |= pad.is_pressed(Button::DPadDown) || y < -0.35;
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
    mpv: OsString,
    player: player::Player,
    input: Input,
    pads: Gamepads,
    ensure_visible: bool,
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>, options: Options) -> Self {
        cc.egui_ctx.set_zoom_factor(options.scale);
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
        Self {
            browser: browser::Browser::new(options.root),
            mpv: options.mpv,
            player: player::Player::default(),
            input: Input::new(Instant::now()),
            pads: Gamepads::new(cc.egui_ctx.clone()),
            ensure_visible: true,
        }
    }
    fn action(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
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
                        .and_then(|path| self.player.launch(&self.mpv, &path));
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
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if self.player.active() {
            if ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            if let Some(result) = self.player.poll() {
                self.browser.refresh();
                if let Err(e) = result {
                    eprintln!("{e}");
                    self.browser.error = Some(e);
                }
                self.input.block();
                self.ensure_visible = true;
            }
            ctx.request_repaint_after(Duration::from_millis(100));
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Playing in mpv");
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
        for action in self.input.update(controls, Instant::now()) {
            self.action(action, ctx);
            if self.player.active() {
                break;
            }
        }
        if self.player.active() {
            ctx.request_repaint();
            return;
        }
        egui::TopBottomPanel::top("path").show(ctx, |ui| {
            ui.add_space(12.0);
            ui.heading("Videos");
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
                ui.label("No visible folders or supported videos in this directory.");
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
    let options = match Options::parse() {
        Ok(Some(options)) => options,
        Ok(None) => return Ok(()),
        Err(e) => {
            eprintln!("{e}\nUse --help for usage.");
            std::process::exit(2);
        }
    };
    eframe::run_native(
        "media-launcher",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("Videos")
                .with_fullscreen(true)
                .with_app_id("media-launcher"),
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(App::new(cc, options)))),
    )
}
