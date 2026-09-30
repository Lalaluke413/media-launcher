mod embedded;
mod external;
mod mpv;

use crate::config::{PlayerBackend, Settings};
use eframe::egui;
use std::{
    ffi::OsStr,
    fs::{File, OpenOptions},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Default)]
pub struct PlaybackStatus {
    pub loading: bool,
    pub paused: bool,
    pub volume: f64,
    pub muted: bool,
    pub position: Option<f64>,
    pub duration: Option<f64>,
}

/// Playback lifecycle shared by local media and phone submissions. Only the
/// embedded implementation owns rendering; the external backend stays usable.
enum Backend {
    Embedded(Arc<Mutex<embedded::Embedded>>),
    External(external::External),
    Unavailable(String),
}
pub struct Player {
    backend: Backend,
    // The owner stays on the window thread; only paint callback clones are Send.
    _window_thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl Player {
    pub fn new(settings: &Settings, cc: &eframe::CreationContext<'_>) -> Self {
        let backend = match settings.config.player.backend {
            PlayerBackend::External => Backend::External(external::External::default()),
            PlayerBackend::Embedded => match embedded::Embedded::new(settings, cc) {
                Ok(player) => Backend::Embedded(Arc::new(Mutex::new(player))),
                Err(error) => Backend::Unavailable(error),
            },
        };
        Self {
            backend,
            _window_thread: std::marker::PhantomData,
        }
    }
    pub fn startup_error(&self) -> Option<&str> {
        match &self.backend {
            Backend::Unavailable(error) => Some(error),
            _ => None,
        }
    }
    pub fn active(&self) -> bool {
        match &self.backend {
            Backend::Embedded(player) => player.lock().unwrap().active,
            Backend::External(player) => player.active(),
            Backend::Unavailable(_) => false,
        }
    }
    pub fn is_embedded(&self) -> bool {
        matches!(self.backend, Backend::Embedded(_))
    }
    pub fn launch(&mut self, settings: &Settings, media: &OsStr) -> Result<(), String> {
        match &mut self.backend {
            Backend::Embedded(player) => player.lock().unwrap().launch(media),
            Backend::External(player) => player.launch(settings, media),
            Backend::Unavailable(error) => Err(error.clone()),
        }
    }
    pub fn poll(&mut self) -> Option<Result<(), String>> {
        match &mut self.backend {
            Backend::Embedded(player) => player.lock().unwrap().poll(),
            Backend::External(player) => player.poll(),
            Backend::Unavailable(_) => None,
        }
    }
    pub fn command(&self, args: &[&str]) -> Result<(), String> {
        match &self.backend {
            Backend::Embedded(player) => player.lock().unwrap().command(args),
            _ => Err("Playback controls require embedded playback".into()),
        }
    }
    pub fn status(&self) -> PlaybackStatus {
        match &self.backend {
            Backend::Embedded(player) => player.lock().unwrap().status,
            _ => PlaybackStatus::default(),
        }
    }
    pub fn paint(&self, ui: &mut egui::Ui) {
        if let Backend::Embedded(player) = &self.backend {
            let player = player.clone();
            ui.painter().add(egui::PaintCallback {
                rect: ui.max_rect(),
                callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |info, painter| {
                    player.lock().unwrap().render(info, painter);
                })),
            });
        }
    }
    pub fn close(&mut self) {
        if let Backend::Embedded(player) = &self.backend {
            player.lock().unwrap().close();
        }
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.close();
    }
}
fn log_file() -> Result<(PathBuf, File), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("media-launcher-{}-{stamp}.log", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(&path)
        .map_err(|e| format!("Cannot create playback log: {e}"))?;
    Ok((path, file))
}
