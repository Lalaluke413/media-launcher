//! The small portion of libmpv's stable C ABI used by this application.
//! Runtime loading keeps builds independent of platform-specific linker files.
use libloading::Library;
use std::{
    ffi::{CStr, c_char, c_int, c_void},
    path::{Path, PathBuf},
    sync::Arc,
};

#[repr(C)]
pub struct Event {
    pub id: c_int,
    pub error: c_int,
    pub userdata: u64,
    pub data: *mut c_void,
}
#[repr(C)]
pub struct EndFile {
    pub reason: c_int,
    pub error: c_int,
    pub entry: i64,
    pub insert: i64,
    pub count: c_int,
}
#[repr(C)]
pub struct Property {
    pub name: *const c_char,
    pub format: c_int,
    pub data: *mut c_void,
}
#[repr(C)]
pub struct Param {
    pub kind: c_int,
    pub data: *mut c_void,
}
impl Param {
    pub fn new<T>(kind: c_int, value: &mut T) -> Self {
        Self {
            kind,
            data: std::ptr::from_mut(value).cast(),
        }
    }
    pub fn end() -> Self {
        Self {
            kind: 0,
            data: std::ptr::null_mut(),
        }
    }
}
#[repr(C)]
pub struct GlInit {
    pub get_proc: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    pub context: *mut c_void,
}
#[repr(C)]
pub struct Fbo {
    pub id: c_int,
    pub width: c_int,
    pub height: c_int,
    pub format: c_int,
}
pub type Callback = Option<unsafe extern "C" fn(*mut c_void)>;

pub struct Api {
    _library: Library,
    pub create: unsafe extern "C" fn() -> *mut c_void,
    pub destroy: unsafe extern "C" fn(*mut c_void),
    pub initialize: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub set_option: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    pub command_sync: unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int,
    pub command: unsafe extern "C" fn(*mut c_void, u64, *const *const c_char) -> c_int,
    pub wait_event: unsafe extern "C" fn(*mut c_void, f64) -> *const Event,
    pub observe: unsafe extern "C" fn(*mut c_void, u64, *const c_char, c_int) -> c_int,
    pub wakeup: unsafe extern "C" fn(*mut c_void, Callback, *mut c_void),
    pub error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    pub render_create: unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *mut Param) -> c_int,
    pub render_free: unsafe extern "C" fn(*mut c_void),
    pub render_update: unsafe extern "C" fn(*mut c_void) -> u64,
    pub render_callback: unsafe extern "C" fn(*mut c_void, Callback, *mut c_void),
    pub render: unsafe extern "C" fn(*mut c_void, *mut Param) -> c_int,
}
impl Api {
    pub fn load(path: Option<&Path>) -> Result<Arc<Self>, String> {
        let candidates = if let Some(path) = path {
            vec![path.to_owned()]
        } else {
            let names: &[&str] = if cfg!(windows) {
                &["mpv-2.dll", "libmpv-2.dll"]
            } else if cfg!(target_os = "macos") {
                &["libmpv.2.dylib", "libmpv.dylib"]
            } else {
                &["libmpv.so.2", "libmpv.so"]
            };
            let mut paths = Vec::new();
            if let Ok(executable) = std::env::current_exe()
                && let Some(directory) = executable.parent()
            {
                paths.extend(names.iter().map(|name| directory.join(name)));
            }
            paths.extend(names.iter().map(PathBuf::from));
            paths
        };
        let mut errors = Vec::new();
        for path in candidates {
            // SAFETY: Loading native code is intentional. Only the user's explicit
            // library, application directory, and OS library search are used.
            let loaded =
                unsafe { load_library(&path).and_then(|library| Self::from_library(library)) };
            match loaded {
                Ok(api) => return Ok(Arc::new(api)),
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        Err(format!(
            "Could not load libmpv (client API 2). Install libmpv or set player.libmpv. For separate-window playback, set player.backend = 'external'.\n{}",
            errors.join("\n")
        ))
    }
    unsafe fn from_library(library: Library) -> Result<Self, String> {
        // SAFETY: Signatures and repr(C) layouts mirror mpv/client.h and render*.h.
        // All symbols are checked, and the library outlives every copied pointer.
        unsafe {
            let version = library
                .get::<unsafe extern "C" fn() -> std::ffi::c_ulong>(b"mpv_client_api_version\0")
                .map_err(|e| e.to_string())?();
            if version >> 16 != 2 {
                return Err(format!("Unsupported libmpv client API {}", version >> 16));
            }
            macro_rules! symbol {
                ($name:literal) => {
                    *library
                        .get(concat!($name, "\0").as_bytes())
                        .map_err(|e| e.to_string())?
                };
            }
            Ok(Self {
                create: symbol!("mpv_create"),
                destroy: symbol!("mpv_terminate_destroy"),
                initialize: symbol!("mpv_initialize"),
                set_option: symbol!("mpv_set_option_string"),
                command_sync: symbol!("mpv_command"),
                command: symbol!("mpv_command_async"),
                wait_event: symbol!("mpv_wait_event"),
                observe: symbol!("mpv_observe_property"),
                wakeup: symbol!("mpv_set_wakeup_callback"),
                error_string: symbol!("mpv_error_string"),
                render_create: symbol!("mpv_render_context_create"),
                render_free: symbol!("mpv_render_context_free"),
                render_update: symbol!("mpv_render_context_update"),
                render_callback: symbol!("mpv_render_context_set_update_callback"),
                render: symbol!("mpv_render_context_render"),
                _library: library,
            })
        }
    }
    pub fn check(&self, result: c_int, operation: &str) -> Result<(), String> {
        if result >= 0 {
            Ok(())
        } else {
            // SAFETY: libmpv returns a static NUL-terminated error string.
            let error = unsafe { CStr::from_ptr((self.error_string)(result)) }.to_string_lossy();
            Err(format!("{operation}: {error}"))
        }
    }
}
unsafe fn load_library(path: &Path) -> Result<Library, String> {
    #[cfg(windows)]
    {
        // An explicit DLL can have dependencies beside it rather than beside us.
        let flags = 0x1000 | if path.is_absolute() { 0x100 } else { 0 };
        unsafe { libloading::os::windows::Library::load_with_flags(path, flags) }
            .map(Library::from)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        unsafe { Library::new(path) }.map_err(|e| e.to_string())
    }
}
