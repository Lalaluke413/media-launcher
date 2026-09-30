//! Opt-in native rendering/lifecycle acceptance test, using local test media only.
use eframe::egui;
use media_launcher::{
    config::{Config, Settings},
    player::Player,
};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

struct Smoke {
    player: Player,
    settings: Settings,
    media: PathBuf,
    marker: PathBuf,
    script_marker: PathBuf,
    watch_later: PathBuf,
    report: Arc<Mutex<Option<Result<(), String>>>>,
    started: Instant,
    checkpoint: Instant,
    phase: u8,
    paused_at: f64,
    captured: bool,
}
impl Smoke {
    fn step(&mut self, ctx: &egui::Context) -> Result<bool, String> {
        if self.started.elapsed() > Duration::from_secs(30) {
            return Err(format!(
                "Playback smoke test timed out in phase {}",
                self.phase
            ));
        }
        let ended = self.player.poll();
        let status = self.player.status();
        match self.phase {
            0 if !status.loading && status.position.is_some_and(|p| p > 0.15) => {
                self.player.command(&["keypress", "SPACE"])?;
                self.player.command(&["set", "pause", "yes"])?;
                self.phase = 1;
            }
            1 if status.paused => {
                self.paused_at = status.position.unwrap_or_default();
                self.checkpoint = Instant::now();
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(400.0, 360.0)));
                self.phase = 2;
            }
            2 => {
                if self.checkpoint.elapsed() > Duration::from_millis(150) {
                    if (status.position.unwrap_or_default() - self.paused_at).abs() > 0.05 {
                        return Err("Video position advanced while paused".into());
                    }
                    if !self.captured {
                        ctx.send_viewport_cmd(
                            egui::ViewportCommand::Screenshot(Default::default()),
                        );
                    }
                }
                for event in ctx.input(|i| i.events.clone()) {
                    if let egui::Event::Screenshot { image, .. } = event {
                        let w = image.size[0];
                        let h = image.size[1];
                        let red = image.pixels[(h / 4) * w + w / 2];
                        let blue = image.pixels[(3 * h / 4) * w + w / 2];
                        let point = (16.0 * ctx.pixels_per_point()).round() as usize;
                        let overlay = image.pixels[point * w + point];
                        if red.r() < 160 || red.b() > 100 || blue.b() < 160 || blue.r() > 100 {
                            return Err(format!(
                                "Video colors/orientation wrong: top={red:?}, bottom={blue:?}"
                            ));
                        }
                        if overlay.r() < 240 || overlay.g() < 240 || overlay.b() < 240 {
                            return Err(format!(
                                "UI overlay was lost after mpv rendering: {overlay:?}"
                            ));
                        }
                        self.captured = true;
                    }
                }
                if self.captured {
                    self.player.command(&["seek", "1", "absolute+exact"])?;
                    self.phase = 3;
                }
            }
            3 if status.position.is_some_and(|p| (0.95..1.2).contains(&p)) => {
                if !status.paused {
                    return Err("Seeking changed the pause state".into());
                }
                self.player.command(&["stop"])?;
                self.phase = 4;
            }
            4 if ended.is_some() => {
                ended.unwrap()?;
                if self.player.active() {
                    return Err("Stop did not leave playback".into());
                }
                if fs::read_dir(&self.watch_later).map_or(true, |mut files| files.next().is_none())
                {
                    return Err("Back did not honor mpv's resume preference".into());
                }
                self.player.launch(&self.settings, self.media.as_os_str())?;
                self.phase = 5;
            }
            5 if !status.loading && status.position.is_some() => {
                if status.paused {
                    return Err("Second playback inherited pause".into());
                }
                self.player.command(&["seek", "3.8", "absolute+exact"])?;
                self.phase = 6;
            }
            6 if ended.is_some() => {
                ended.unwrap()?;
                self.player.launch(
                    &self.settings,
                    self.media.with_file_name("missing.mp4").as_os_str(),
                )?;
                self.phase = 7;
            }
            7 if ended.is_some() => {
                if ended.unwrap().is_ok() {
                    return Err("Missing file did not report a playback error".into());
                }
                self.player.launch(
                    &self.settings,
                    std::ffi::OsStr::new("https://media-launcher.invalid/embedded/test"),
                )?;
                self.phase = 8;
            }
            8 if !status.loading && status.position.is_some_and(|p| p > 0.05) => {
                if !self.marker.exists() {
                    return Err(
                        "Embedded playback did not invoke the user's yt-dlp extractor".into(),
                    );
                }
                if !self.script_marker.exists() {
                    return Err("User mpv script/options were not preserved".into());
                }
                self.player.command(&["stop"])?;
                self.phase = 9;
            }
            9 if ended.is_some() => {
                ended.unwrap()?;
                self.player.launch(&self.settings, self.media.as_os_str())?;
                self.phase = 10;
            }
            10 if !status.loading && status.position.is_some_and(|p| p > 0.05) => {
                // on_exit must free a renderer while it is actively playing.
                return Ok(true);
            }
            _ => {
                if let Some(result) = ended {
                    result?;
                    return Err(format!("Unexpected EOF in phase {}", self.phase));
                }
            }
        }
        Ok(false)
    }
}
impl eframe::App for Smoke {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let result = self.step(ctx);
        if !matches!(result, Ok(false)) {
            *self.report.lock().unwrap() = Some(result.map(|_| ()));
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(10));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                // A host's active clipping must not leak into libmpv's smaller
                // intermediate video passes. The player owns the full target.
                let clip = ui.clip_rect();
                ui.set_clip_rect(egui::Rect::from_min_max(
                    clip.min + egui::vec2(100.0, 100.0),
                    clip.max,
                ));
                self.player.paint(ui);
                ui.set_clip_rect(clip);
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(15.0, 15.0)),
                    0.0,
                    egui::Color32::WHITE,
                );
            });
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.player.close();
    }
}

#[test]
#[ignore = "requires a desktop/OpenGL, libmpv and yt-dlp; opens a small test window"]
fn embedded_rendering_controls_recovery_and_user_extractors() {
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("video,雪 ' $(literal).y4m");
    let mut video = b"YUV4MPEG2 W64 H64 F30:1 Ip A1:1 C420jpeg\n".to_vec();
    for _ in 0..120 {
        video.extend(b"FRAME\n");
        video.extend(std::iter::repeat_n(81, 64 * 32));
        video.extend(std::iter::repeat_n(41, 64 * 32));
        video.extend(std::iter::repeat_n(90, 32 * 16));
        video.extend(std::iter::repeat_n(240, 32 * 16));
        video.extend(std::iter::repeat_n(240, 32 * 16));
        video.extend(std::iter::repeat_n(110, 32 * 16));
    }
    fs::write(&media, &video).unwrap();
    let mut settings = Settings {
        config: Config::default(),
        config_path: dir.path().join("config.toml"),
        data_dir: dir.path().join("data,雪's"),
    };
    let mpv_config = dir.path().join("mpv-config");
    fs::create_dir(&mpv_config).unwrap();
    fs::write(mpv_config.join("input.conf"), "SPACE quit\n").unwrap();
    fs::write(mpv_config.join("mpv.conf"), "vo=gpu-next\nkeep-open=yes\nsave-position-on-quit=yes\nscript-opts=custom-option=yes,ytdl_hook-ytdl_path=nonexistent-extractor\nytdl-raw-options=force-ipv6=\n").unwrap();
    let script_marker = dir.path().join("mpv-script.loaded");
    fs::create_dir(mpv_config.join("scripts")).unwrap();
    let script = format!(
        r#"mp.observe_property('options/script-opts', 'native', function(_, value)
    if value and value['custom-option'] == 'yes' and value['ytdl_hook-ytdl_path'] == 'yt-dlp' then
        local file = assert(io.open([[{}]], 'w'))
        file:write('preserved')
        file:close()
    end
end)
"#,
        script_marker.display()
    );
    fs::write(mpv_config.join("scripts/preserved.lua"), script).unwrap();
    let watch_later = dir.path().join("watch_later");
    settings.config.player.args = vec![
        format!("--config-dir={}", mpv_config.display()),
        format!("--watch-later-dir={}", watch_later.display()),
        "--ao=null".into(),
        "--hwdec=no".into(),
        "--script-opts-append=ytdl_hook-try_ytdl_first=yes".into(),
    ];
    settings.config.yt_dlp.executable = Some("yt-dlp".into());
    settings.prepare().unwrap();
    let marker = dir.path().join("extractor.loaded");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/video.y4m", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let server = std::thread::spawn(move || {
        while !stopped.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                    let mut request = [0; 4096];
                    let _ = stream.read(&mut request);
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        video.len()
                    );
                    let _ = stream.write_all(&video);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(e) => panic!("test server: {e}"),
            }
        }
    });
    let source = format!(
        r#"from pathlib import Path
from yt_dlp.extractor.common import InfoExtractor
class EmbeddedCheckIE(InfoExtractor):
    _VALID_URL = r'https://media-launcher.invalid/embedded/(?P<id>test)'
    def _real_extract(self, url):
        Path(r"{}").write_text('loaded')
        return {{'id': 'test', 'title': 'Embedded test', 'url': '{url}', 'ext': 'y4m'}}
"#,
        marker.display()
    );
    fs::write(
        settings
            .plugin_dir()
            .join("user/yt_dlp_plugins/extractor/embedded_check.py"),
        source,
    )
    .unwrap();
    let report = Arc::new(Mutex::new(None));
    let app_report = report.clone();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 320.0]),
        event_loop_builder: Some(Box::new(|builder| {
            #[cfg(target_os = "linux")]
            {
                use winit::platform::x11::EventLoopBuilderExtX11;
                builder.with_any_thread(true);
            }
            #[cfg(windows)]
            {
                use winit::platform::windows::EventLoopBuilderExtWindows;
                builder.with_any_thread(true);
            }
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Media Launcher embedded test",
        options,
        Box::new(move |cc| {
            let mut player = Player::new(&settings, cc);
            if let Some(error) = player.startup_error() {
                return Err(std::io::Error::other(error).into());
            }
            player
                .launch(&settings, media.as_os_str())
                .map_err(std::io::Error::other)?;
            Ok(Box::new(Smoke {
                player,
                settings,
                media,
                marker,
                script_marker,
                watch_later,
                report: app_report,
                started: Instant::now(),
                checkpoint: Instant::now(),
                phase: 0,
                paused_at: 0.0,
                captured: false,
            }))
        }),
    )
    .unwrap();
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
    report
        .lock()
        .unwrap()
        .take()
        .expect("Test window closed prematurely")
        .unwrap();
}
