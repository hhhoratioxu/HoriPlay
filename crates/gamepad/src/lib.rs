//! SDL3 is loaded from an explicit absolute path; never search the current directory for a DLL.
use anyhow::{anyhow, bail, Context, Result};
use horiplay_core::{PadInfo, PadState};
use libloading::Library;
use std::{
    collections::HashMap,
    ffi::{c_char, c_void, CStr},
    path::{Path, PathBuf},
    sync::Arc,
};

type Handle = *mut c_void;
pub struct SdlGamepads {
    api: Arc<Api>,
    handles: HashMap<u32, Handle>,
    _runtime: SdlRuntime,
}
/// Initialize on the application main thread. Keep a reference there until samplers stop.
#[derive(Clone)]
pub struct SdlRuntime(Arc<RuntimeInner>);
struct RuntimeInner {
    api: Arc<Api>,
    init_thread: std::thread::ThreadId,
}
impl Drop for RuntimeInner {
    fn drop(&mut self) {
        if std::thread::current().id() == self.init_thread {
            unsafe {
                (self.api.quit)(0x00002000);
            }
        } else {
            // Never call SDL shutdown on a foreign thread or unload a library with live SDL
            // system threads. A misused runtime is safely retained until process exit.
            std::mem::forget(self.api.clone());
        }
    }
}
struct Api {
    _lib: Library,
    quit: unsafe extern "C" fn(u32),
    update: unsafe extern "C" fn(),
    list: unsafe extern "C" fn(*mut i32) -> *mut u32,
    free: unsafe extern "C" fn(*mut c_void),
    open: unsafe extern "C" fn(u32) -> Handle,
    close: unsafe extern "C" fn(Handle),
    name: unsafe extern "C" fn(Handle) -> *const c_char,
    vendor: unsafe extern "C" fn(Handle) -> u16,
    product: unsafe extern "C" fn(Handle) -> u16,
    connected: unsafe extern "C" fn(Handle) -> bool,
    axis: unsafe extern "C" fn(Handle, i32) -> i16,
    button: unsafe extern "C" fn(Handle, i32) -> bool,
    props: unsafe extern "C" fn(Handle) -> u32,
    boolean: unsafe extern "C" fn(u32, *const c_char, bool) -> bool,
    power: unsafe extern "C" fn(Handle, *mut i32) -> i32,
    connection: unsafe extern "C" fn(Handle) -> i32,
    rumble: unsafe extern "C" fn(Handle, u16, u16, u32) -> bool,
    error: unsafe extern "C" fn() -> *const c_char,
}
impl SdlGamepads {
    pub fn default_path() -> Result<PathBuf> {
        if let Some(path) = std::env::var_os("HORIPLAY_SDL3") {
            return Ok(path.into());
        }
        let name = if cfg!(target_os = "windows") {
            "SDL3.dll"
        } else if cfg!(target_os = "macos") {
            "libSDL3.0.dylib"
        } else {
            "libSDL3.so.0"
        };
        Ok(std::env::current_exe()?
            .parent()
            .ok_or_else(|| anyhow!("missing executable directory"))?
            .join(name))
    }
    /// Main-thread convenience constructor for native diagnostics.
    pub fn load(path: &Path) -> Result<Self> {
        Ok(SdlRuntime::initialize(path)?.open())
    }
}
impl SdlRuntime {
    /// SDL requires main-thread initialization. Polling uses SDL_UpdateGamepads, not PumpEvents.
    pub fn initialize(path: &Path) -> Result<Self> {
        if !path.is_absolute() {
            bail!("SDL3 library path must be absolute");
        }
        // SAFETY: signatures below mirror the SDL 3.2+ public ABI. The library outlives all handles.
        unsafe {
            let lib = Library::new(path).with_context(|| {
                format!(
                    "SDL3 unavailable: {}. Set HORIPLAY_SDL3 to an official SDL3 library.",
                    path.display()
                )
            })?;
            let init: unsafe extern "C" fn(u32) -> bool = *lib.get(b"SDL_Init\0")?;
            macro_rules! sym {
                ($name:literal) => {
                    *lib.get(concat!($name, "\0").as_bytes())?
                };
            }
            let gamepad_events: unsafe extern "C" fn(bool) = sym!("SDL_SetGamepadEventsEnabled");
            let joystick_events: unsafe extern "C" fn(bool) = sym!("SDL_SetJoystickEventsEnabled");
            let api = Api {
                quit: sym!("SDL_QuitSubSystem"),
                update: sym!("SDL_UpdateGamepads"),
                list: sym!("SDL_GetGamepads"),
                free: sym!("SDL_free"),
                open: sym!("SDL_OpenGamepad"),
                close: sym!("SDL_CloseGamepad"),
                name: sym!("SDL_GetGamepadName"),
                vendor: sym!("SDL_GetGamepadVendor"),
                product: sym!("SDL_GetGamepadProduct"),
                connected: sym!("SDL_GamepadConnected"),
                axis: sym!("SDL_GetGamepadAxis"),
                button: sym!("SDL_GetGamepadButton"),
                props: sym!("SDL_GetGamepadProperties"),
                boolean: sym!("SDL_GetBooleanProperty"),
                power: sym!("SDL_GetGamepadPowerInfo"),
                connection: sym!("SDL_GetGamepadConnectionState"),
                rumble: sym!("SDL_RumbleGamepad"),
                error: sym!("SDL_GetError"),
                _lib: lib,
            };
            if !init(0x00002000) {
                bail!("SDL_Init: {}", read_string((api.error)()));
            }
            gamepad_events(false);
            joystick_events(false);
            Ok(Self(Arc::new(RuntimeInner {
                api: Arc::new(api),
                init_thread: std::thread::current().id(),
            })))
        }
    }
    /// A sampler owns its handles on one worker thread; it neither initializes nor quits SDL.
    pub fn open(&self) -> SdlGamepads {
        SdlGamepads {
            api: self.0.api.clone(),
            handles: HashMap::new(),
            _runtime: self.clone(),
        }
    }
}
impl SdlGamepads {
    pub fn poll(&mut self) -> Result<Vec<PadInfo>> {
        // SAFETY: owned SDL pointers are only accessed on this object's original thread.
        unsafe {
            (self.api.update)();
            let mut count = 0;
            let ids = (self.api.list)(&mut count);
            if ids.is_null() {
                bail!("SDL_GetGamepads: {}", read_string((self.api.error)()));
            }
            let found = std::slice::from_raw_parts(ids, count.max(0) as usize).to_vec();
            (self.api.free)(ids.cast());
            self.handles.retain(|id, pad| {
                if !found.contains(id) || !(self.api.connected)(*pad) {
                    (self.api.close)(*pad);
                    false
                } else {
                    true
                }
            });
            for id in found {
                self.handles
                    .entry(id)
                    .or_insert_with(|| (self.api.open)(id));
            }
            self.handles.retain(|_, p| !p.is_null());
            let mut result = Vec::new();
            for (id, pad) in &self.handles {
                let props = (self.api.props)(*pad);
                let mut percent = -1;
                let power = (self.api.power)(*pad, &mut percent);
                let state = PadState {
                    buttons: (0..15).fold(0, |b, i| {
                        b | if (self.api.button)(*pad, i) {
                            1 << i
                        } else {
                            0
                        }
                    }),
                    axes: std::array::from_fn(|i| (self.api.axis)(*pad, i as i32)),
                    triggers: std::array::from_fn(|i| {
                        (self.api.axis)(*pad, 4 + i as i32).max(0) as u16
                    }),
                };
                result.push(PadInfo {
                    id: *id,
                    name: read_string((self.api.name)(*pad)),
                    vendor: (self.api.vendor)(*pad),
                    product: (self.api.product)(*pad),
                    connection: match (self.api.connection)(*pad) {
                        1 => "USB / wired",
                        2 => "Wireless (transport reported by SDL)",
                        _ => "Unknown",
                    }
                    .into(),
                    power: match power {
                        1 => "On battery",
                        2 => "No battery",
                        3 => "Charging",
                        4 => "Charged",
                        _ => "Unknown",
                    }
                    .into(),
                    battery_percent: if (0..=100).contains(&percent) {
                        Some(percent)
                    } else {
                        None
                    },
                    rumble: (self.api.boolean)(props, c"SDL.gamepad.cap.rumble".as_ptr(), false),
                    trigger_rumble: (self.api.boolean)(
                        props,
                        c"SDL.gamepad.cap.trigger_rumble".as_ptr(),
                        false,
                    ),
                    state,
                });
            }
            result.sort_by_key(|p| p.id);
            Ok(result)
        }
    }
    pub fn rumble(&self, id: u32, low: u16, high: u16, milliseconds: u32) -> Result<()> {
        let pad = self.handles.get(&id).context("controller disconnected")?;
        // SAFETY: a live handle and bounded duration are passed to SDL.
        unsafe {
            if !(self.api.rumble)(*pad, low, high, milliseconds.min(1000)) {
                bail!("rumble unsupported: {}", read_string((self.api.error)()));
            }
        }
        Ok(())
    }
}
unsafe fn read_string(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}
impl Drop for SdlGamepads {
    fn drop(&mut self) {
        unsafe {
            for pad in self.handles.values() {
                (self.api.rumble)(*pad, 0, 0, 0);
                (self.api.close)(*pad);
            }
        }
    }
}
