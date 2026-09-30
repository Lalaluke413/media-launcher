use super::{
    PlaybackStatus,
    mpv::{self, Api, Param},
};
use crate::config::Settings;
use eframe::{
    egui,
    glow::{self, HasContext},
};
use std::{
    collections::HashMap,
    ffi::{CStr, CString, OsStr, c_char, c_void},
    io::Write,
    path::PathBuf,
    sync::Arc,
    thread::ThreadId,
};

pub struct Embedded {
    api: Arc<Api>,
    core: *mut c_void,
    render: *mut c_void,
    owner: ThreadId,
    notify: Box<egui::Context>,
    lookup: Box<Lookup>,
    _extractor: tempfile::TempPath,
    log: PathBuf,
    pub status: PlaybackStatus,
    pub active: bool,
    render_error: Option<String>,
    save_on_stop: bool,
}
// SAFETY: egui requires Send callbacks. This does not permit moving GL work:
// every access checks the creating thread, and the shared Mutex serializes it.
// Callback clones retain an inert object after close frees its GL/core handles.
unsafe impl Send for Embedded {}

impl Embedded {
    pub fn new(settings: &Settings, cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        let lookup = cc
            .get_proc_address
            .ok_or("Embedded playback requires the OpenGL renderer")?;
        let api = Api::load(settings.config.player.libmpv.as_deref())?;
        let mut extractor = tempfile::Builder::new()
            .prefix("media-launcher-ytdlp-")
            .suffix(".conf")
            .tempfile()
            .map_err(|e| format!("Cannot create yt-dlp configuration: {e}"))?;
        extractor
            .write_all(settings.extractor_config()?.as_bytes())
            .map_err(|e| e.to_string())?;
        let (log, file) = super::log_file()?;
        drop(file);
        // SAFETY: The loaded API has checked signatures and retains its library.
        let core = unsafe { (api.create)() };
        if core.is_null() {
            return Err("libmpv could not create a player".into());
        }
        let mut player = Self {
            api,
            core,
            render: std::ptr::null_mut(),
            owner: std::thread::current().id(),
            notify: Box::new(cc.egui_ctx.clone()),
            lookup: Box::new(Lookup {
                initial: std::ptr::null_mut(),
                functions: HashMap::new(),
            }),
            _extractor: extractor.into_temp_path(),
            log,
            status: PlaybackStatus::default(),
            active: false,
            render_error: None,
            save_on_stop: false,
        };
        // libmpv reads mpv.conf during initialize, potentially replacing initial
        // options. Apply startup-only options first, then reapply runtime options
        // and our integration after initialization, before creating the renderer.
        player.option("config", "yes")?;
        for arg in &settings.config.player.args {
            let (name, value) = option(arg)?;
            if startup_option(name) {
                player.option(name, value)?;
            }
        }
        player.option("vo", "libmpv")?;
        player.api.check(
            unsafe { (player.api.initialize)(core) },
            "Initializing libmpv",
        )?;
        // Legacy mpv input.conf (including an SDL gamepad source enabled by the
        // user's configuration) must not compete with application actions.
        player.init_command(&["define-section", "media-launcher", "", "force"])?;
        player.init_command(&["enable-section", "media-launcher", "exclusive"])?;
        for arg in &settings.config.player.args {
            let (name, value) = option(arg)?;
            if !startup_option(name) {
                player.apply_option(name, value)?;
            }
        }
        if let Some(executable) = &settings.config.yt_dlp.executable {
            player.apply_option(
                "script-opts-append",
                &format!("ytdl_hook-ytdl_path={}", executable.display()),
            )?;
        }
        let extractor = player
            ._extractor
            .to_str()
            .ok_or("Temporary directory must be valid Unicode for yt-dlp")?;
        player.apply_option(
            "ytdl-raw-options-append",
            &format!("config-locations={extractor}"),
        )?;
        let log = native_string(player.log.as_os_str())?;
        player.option_cstr(c"log-file", &log)?;
        // These belong to the application, even if mpv.conf specifies a VO/window.
        for (name, value) in [
            ("vo", "libmpv"),
            ("idle", "yes"),
            ("keep-open", "no"),
            ("terminal", "no"),
            ("input-terminal", "no"),
            ("input-default-bindings", "no"),
            ("input-vo-keyboard", "no"),
            ("osc", "no"),
        ] {
            player.option(name, value)?;
        }
        for (name, format) in [
            (c"pause", 3),
            (c"time-pos", 5),
            (c"duration", 5),
            (c"options/save-position-on-quit", 3),
        ] {
            player.api.check(
                unsafe { (player.api.observe)(core, 0, name.as_ptr(), format) },
                "Observing playback",
            )?;
        }
        let context = std::ptr::from_mut(player.notify.as_mut()).cast();
        unsafe { (player.api.wakeup)(core, Some(wake), context) };
        // eframe's resolver is borrowed only during construction. Keep the
        // callback data and resolved pointers owned for the renderer's lifetime.
        // libmpv resolves its GL functions synchronously in render_create.
        let mut initial = InitialLookup { get_proc: lookup };
        player.lookup.initial = std::ptr::from_mut(&mut initial).cast();
        let mut init = mpv::GlInit {
            get_proc,
            context: std::ptr::from_mut(player.lookup.as_mut()).cast(),
        };
        let mut params = vec![
            Param {
                kind: 1,
                data: c"opengl".as_ptr().cast_mut().cast(),
            },
            Param::new(2, &mut init),
        ];
        // Supplying the host display enables libmpv's compatible hardware-decoder
        // interop paths. Its lifetime includes on_exit, where our renderer is freed.
        use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
        if let Ok(display) = cc.display_handle() {
            match display.as_raw() {
                RawDisplayHandle::Xlib(display) => {
                    if let Some(display) = display.display {
                        params.push(Param {
                            kind: 8,
                            data: display.as_ptr(),
                        });
                    }
                }
                RawDisplayHandle::Wayland(display) => params.push(Param {
                    kind: 9,
                    data: display.display.as_ptr(),
                }),
                _ => {}
            }
        }
        params.push(Param::end());
        let result =
            unsafe { (player.api.render_create)(&mut player.render, core, params.as_mut_ptr()) };
        player.lookup.initial = std::ptr::null_mut();
        player.api.check(result, "Creating mpv OpenGL renderer")?;
        unsafe { (player.api.render_callback)(player.render, Some(wake), context) };
        eprintln!("Embedded mpv log: {}", player.log.display());
        Ok(player)
    }
    fn apply_option(&self, name: &str, value: &str) -> Result<(), String> {
        for suffix in ["append", "add", "pre", "del", "clr", "toggle", "set"] {
            if let Some(name) = name.strip_suffix(&format!("-{suffix}")) {
                // List suffixes are CLI syntax, not C option names. change-list
                // preserves existing values and applies the same list operation.
                return self.init_command(&["change-list", name, suffix, value]);
            }
        }
        self.option(name, value)
    }
    fn init_command(&self, args: &[&str]) -> Result<(), String> {
        debug_assert!(self.render.is_null());
        let args = args
            .iter()
            .map(|a| CString::new(*a))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "mpv option contains NUL")?;
        let pointers: Vec<_> = args
            .iter()
            .map(|arg| arg.as_ptr())
            .chain(std::iter::once(std::ptr::null()))
            .collect();
        // Synchronous calls are confined to initialization, before GL rendering.
        self.api.check(
            unsafe { (self.api.command_sync)(self.core, pointers.as_ptr()) },
            "Configuring embedded mpv",
        )
    }
    fn assert_owner(&self) {
        assert_eq!(
            self.owner,
            std::thread::current().id(),
            "libmpv GL context used on another thread"
        );
    }
    fn option(&self, name: &str, value: &str) -> Result<(), String> {
        let name = CString::new(name).map_err(|_| "mpv option contains NUL")?;
        let value = CString::new(value).map_err(|_| "mpv option contains NUL")?;
        self.option_cstr(&name, &value)
    }
    fn option_cstr(&self, name: &CStr, value: &CStr) -> Result<(), String> {
        // Only used before initializing the renderer; playback operations are async.
        self.api.check(
            unsafe { (self.api.set_option)(self.core, name.as_ptr(), value.as_ptr()) },
            &format!("Setting mpv option {}", name.to_string_lossy()),
        )
    }
    pub fn command(&self, args: &[&str]) -> Result<(), String> {
        // Back is the embedded equivalent of leaving mpv. Honor the user's
        // existing resume preference rather than inventing application history.
        if args == ["stop"] && self.active && !self.status.loading && self.save_on_stop {
            self.command_native(vec![c"write-watch-later-config".into()], 3)?;
        }
        self.command_native(
            args.iter()
                .map(|a| CString::new(*a).map_err(|_| "mpv command contains NUL".to_owned()))
                .collect::<Result<Vec<_>, _>>()?,
            2,
        )
    }
    fn command_native(&self, args: Vec<CString>, id: u64) -> Result<(), String> {
        self.assert_owner();
        if self.core.is_null() {
            return Err("Embedded player is closed".into());
        }
        let pointers: Vec<_> = args
            .iter()
            .map(|arg| arg.as_ptr())
            .chain(std::iter::once(std::ptr::null()))
            .collect();
        // mpv_command_async copies the strings before returning and is render-thread safe.
        self.api.check(
            unsafe { (self.api.command)(self.core, id, pointers.as_ptr()) },
            "mpv command",
        )
    }
    pub fn launch(&mut self, media: &OsStr) -> Result<(), String> {
        self.assert_owner();
        if self.active {
            return Err("Playback is already active".into());
        }
        self.poll(); // Drain any old idle notifications before accepting a new file.
        self.command(&["set", "pause", "no"])?;
        self.command_native(
            vec![c"loadfile".into(), native_string(media)?, c"replace".into()],
            1,
        )?;
        self.status = PlaybackStatus {
            loading: true,
            ..Default::default()
        };
        self.active = true;
        Ok(())
    }
    pub fn poll(&mut self) -> Option<Result<(), String>> {
        self.assert_owner();
        if self.core.is_null() {
            return None;
        }
        if let Some(error) = self.render_error.take() {
            let _ = self.command(&["stop"]);
            self.active = false;
            self.status = PlaybackStatus::default();
            return Some(Err(format!("{error}. Log: {}", self.log.display())));
        }
        let mut finished = None;
        // SAFETY: timeout 0 is render-thread safe. Consume each payload before the
        // next wait_event invalidates it. Only this thread drains this handle.
        unsafe {
            loop {
                let event = &*(self.api.wait_event)(self.core, 0.0);
                match event.id {
                    0 => break,
                    1 if self.active => {
                        finished = Some(Err("libmpv shut down".into()));
                    }
                    5 if event.error < 0 && event.userdata != 3 && self.active => {
                        finished = Some(self.api.check(event.error, "Playback command"));
                    }
                    7 if self.active && !event.data.is_null() => {
                        let end = &*event.data.cast::<mpv::EndFile>();
                        // Redirects (e.g. playlist expansion) are not playback completion.
                        if end.reason != 5 {
                            finished = Some(if end.error < 0 {
                                self.api.check(end.error, "Playback failed")
                            } else {
                                Ok(())
                            });
                        }
                    }
                    8 => self.status.loading = false,
                    22 if !event.data.is_null() => {
                        let property = &*event.data.cast::<mpv::Property>();
                        if property.name.is_null() {
                            continue;
                        }
                        let name = CStr::from_ptr(property.name);
                        match (name.to_bytes(), property.format) {
                            (b"options/save-position-on-quit", 3) if !property.data.is_null() => {
                                self.save_on_stop = *property.data.cast::<i32>() != 0;
                            }
                            (b"pause", 3) if !property.data.is_null() => {
                                self.status.paused = *property.data.cast::<i32>() != 0
                            }
                            (b"time-pos", 5) if !property.data.is_null() => {
                                self.status.position = Some(*property.data.cast::<f64>())
                            }
                            (b"duration", 5) if !property.data.is_null() => {
                                self.status.duration = Some(*property.data.cast::<f64>())
                            }
                            (b"time-pos", _) => self.status.position = None,
                            (b"duration", _) => self.status.duration = None,
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        finished.map(|result| {
            self.active = false;
            self.status = PlaybackStatus::default();
            if result.is_err() {
                let _ = self.command(&["stop"]);
            }
            result.map_err(|e| format!("{e}. Log: {}", self.log.display()))
        })
    }
    pub fn render(&mut self, info: egui::PaintCallbackInfo, painter: &eframe::egui_glow::Painter) {
        self.assert_owner();
        if self.render.is_null() || !self.active || info.screen_size_px.contains(&0) {
            return;
        }
        let framebuffer = painter.intermediate_fbo();
        let mut fbo = mpv::Fbo {
            id: framebuffer.map_or(0, |fb| fb.0.get() as i32),
            width: info.screen_size_px[0] as i32,
            height: info.screen_size_px[1] as i32,
            format: 0,
        };
        let mut flip = 1_i32;
        let mut block = 0_i32;
        let mut params = [
            Param::new(3, &mut fbo),
            Param::new(4, &mut flip),
            Param::new(12, &mut block),
            Param::end(),
        ];
        // SAFETY: eframe's context is current in its paint callback. The target is
        // the whole window framebuffer, and UI overlays are painted afterwards.
        unsafe {
            (self.api.render_update)(self.render);
            let result = (self.api.render)(self.render, params.as_mut_ptr());
            painter
                .gl()
                .bind_framebuffer(glow::FRAMEBUFFER, framebuffer);
            if result < 0 {
                self.render_error = self.api.check(result, "Rendering video").err();
                self.notify.request_repaint();
            }
        }
    }
    pub fn close(&mut self) {
        if self.core.is_null() && self.render.is_null() {
            return;
        }
        self.assert_owner();
        // The GL context must still be current. eframe calls on_exit before it
        // destroys the context. Renderer destruction must precede core destruction.
        unsafe {
            if !self.render.is_null() {
                (self.api.render_callback)(self.render, None, std::ptr::null_mut());
                (self.api.render_free)(self.render);
                self.render = std::ptr::null_mut();
            }
            if !self.core.is_null() {
                (self.api.wakeup)(self.core, None, std::ptr::null_mut());
                (self.api.destroy)(self.core);
                self.core = std::ptr::null_mut();
            }
        }
        self.active = false;
    }
}
impl Drop for Embedded {
    fn drop(&mut self) {
        if (!self.core.is_null() || !self.render.is_null())
            && self.owner != std::thread::current().id()
        {
            // Never unwind and unload a library with live C callbacks/GL resources.
            std::process::abort();
        }
        self.close();
    }
}

struct InitialLookup<'a> {
    get_proc: &'a dyn Fn(&CStr) -> *const c_void,
}
struct Lookup {
    initial: *mut c_void,
    functions: HashMap<CString, usize>,
}
unsafe extern "C" fn get_proc(context: *mut c_void, name: *const c_char) -> *mut c_void {
    // SAFETY: the boxed lookup outlives the renderer. Its initial resolver is
    // live only during render_create; subsequent requests use retained pointers.
    // GL API calls and this resolver are confined to the same window thread.
    let lookup = unsafe { &mut *context.cast::<Lookup>() };
    let name = unsafe { CStr::from_ptr(name) };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if let Some(pointer) = lookup.functions.get(name) {
            return *pointer as *mut c_void;
        }
        if lookup.initial.is_null() {
            return std::ptr::null_mut();
        }
        let initial = unsafe { &*lookup.initial.cast::<InitialLookup<'_>>() };
        let pointer = (initial.get_proc)(name);
        lookup.functions.insert(name.to_owned(), pointer as usize);
        pointer.cast_mut()
    }))
    .unwrap_or(std::ptr::null_mut())
}
unsafe extern "C" fn wake(context: *mut c_void) {
    // SAFETY: the boxed egui Context stays alive until callbacks are unregistered
    // and the mpv render/core handles have both been destroyed.
    let context = unsafe { &*context.cast::<egui::Context>() };
    // Do not let a Rust panic unwind through a C callback.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| context.request_repaint()));
}
fn native_string(value: &OsStr) -> Result<CString, String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        CString::new(value.as_bytes()).map_err(|_| "Media path contains NUL".into())
    }
    #[cfg(not(unix))]
    {
        CString::new(
            value
                .to_str()
                .ok_or("libmpv requires Unicode media paths on Windows")?,
        )
        .map_err(|_| "Media path contains NUL".into())
    }
}
fn option(arg: &str) -> Result<(&str, &str), String> {
    let option = arg
        .strip_prefix("--")
        .ok_or("mpv options must start with --")?;
    if let Some((name, value)) = option.split_once('=') {
        Ok((name, value))
    } else if let Some(name) = option.strip_prefix("no-") {
        Ok((name, "no"))
    } else {
        Ok((option, "yes"))
    }
}

fn startup_option(name: &str) -> bool {
    matches!(
        name,
        "config"
            | "config-dir"
            | "input-conf"
            | "load-scripts"
            | "scripts"
            | "player-operation-mode"
            | "input-app-events"
    )
}
