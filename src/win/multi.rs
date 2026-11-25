//! Multi-instance support.
//!
//! * **Mutex mode** (default, no admin): hold Roblox's singleton mutexes/events so
//!   new clients think an instance is already registered, plus the Error-773 fix
//!   that locks `RobloxCookies.dat`. Roblox must be closed before enabling.
//! * **Handle mode** (admin): a background monitor closes the singleton handles
//!   inside each client as it starts (see [`super::handles`]). Works with clients
//!   already open.

use crate::error::{AppError, AppResult};
use crate::store::settings::MultiMethod;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SINGLETON_NAMES: [&str; 3] = ["ROBLOX_SingletonEvent", "ROBLOX_singletonEvent", "ROBLOX_singletonMutex"];

/// Opaque live handle; dropping it releases the mutexes / stops the monitor.
pub struct MultiGuard {
    mode: MultiMethod,
    #[cfg(windows)]
    mutexes: Vec<MutexHandle>,
    #[cfg(windows)]
    _login_files: Vec<std::fs::File>,
    pub(crate) launch_gate: Mutex<bool>,
    monitor_stop: Option<Arc<AtomicBool>>,
    monitor_ready: Arc<Mutex<std::collections::HashSet<super::process::ProcId>>>,
}

#[cfg(windows)]
struct MutexHandle {
    handle: windows_sys::Win32::Foundation::HANDLE,
    owned: bool,
}
// SAFETY: a Windows HANDLE is a process-wide identifier; releasing/closing it from
// another thread is sound, and access is serialised by the Core locks around the guard.
#[cfg(windows)]
unsafe impl Send for MutexHandle {}
#[cfg(windows)]
unsafe impl Sync for MutexHandle {}

impl MultiGuard {
    pub fn mode(&self) -> MultiMethod {
        self.mode
    }

    /// In handle mode, whether every one of `ids` has had its singleton handles cleared.
    pub fn handles_ready(&self, ids: &[super::process::ProcId]) -> bool {
        if self.mode != MultiMethod::Handle {
            return true;
        }
        let ready = self.monitor_ready.lock().unwrap_or_else(|p| p.into_inner());
        ids.iter().all(|id| ready.contains(id))
    }
}

impl Drop for MultiGuard {
    fn drop(&mut self) {
        if let Some(stop) = &self.monitor_stop {
            stop.store(true, Ordering::SeqCst);
        }
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::Foundation::CloseHandle;
            use windows_sys::Win32::System::Threading::ReleaseMutex;
            for mutex in &self.mutexes {
                if mutex.owned {
                    ReleaseMutex(mutex.handle);
                }
                CloseHandle(mutex.handle);
            }
        }
        crate::log_info!("Multi Roblox stopped");
    }
}

/// Starts multi-instance support in the requested mode.
#[cfg(windows)]
pub fn enable(method: MultiMethod, _legacy_lock_setting: bool) -> AppResult<MultiGuard> {
    use std::os::windows::fs::OpenOptionsExt;
    if !super::process::list_roblox().is_empty() {
        return Err(AppError::new("ROBLOX_RUNNING", "Close Roblox first", "Close all Roblox clients before starting Multi Roblox."));
    }
    let dir = crate::paths::local_appdata()
        .ok_or_else(|| AppError::invalid("LOCALAPPDATA_MISSING", "Cannot locate Roblox storage."))?
        .join("Roblox")
        .join("LocalStorage");
    std::fs::create_dir_all(&dir).map_err(|e| AppError::io("Opening Roblox storage", &e))?;
    let mut files = Vec::new();
    for name in ["RobloxCookies.dat", "appStorage.json"] {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(dir.join(name))
            .map_err(|e| AppError::io("Protecting shared Roblox login files", &e))?;
        files.push(file);
    }
    let mut guard = match method {
        MultiMethod::Mutex => enable_mutex(),
        MultiMethod::Handle => enable_handle(),
    }?;
    guard._login_files = files;
    Ok(guard)
}

#[cfg(not(windows))]
pub fn enable(_method: MultiMethod, _cookie_lock_773: bool) -> AppResult<MultiGuard> {
    Err(AppError::new("WINDOWS_ONLY", "Windows Only", "Multi Roblox is only available on Windows."))
}

#[cfg(windows)]
fn enable_mutex() -> AppResult<MultiGuard> {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, SetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    if !super::process::list_roblox().is_empty() {
        return Err(AppError::new(
            "ROBLOX_RUNNING",
            "Close Roblox First",
            "Close all Roblox clients before enabling mutex-based Multi Roblox, or switch to Handle mode.",
        ));
    }

    let mut mutexes = Vec::new();
    for name in SINGLETON_NAMES {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: the name buffer is null-terminated; the returned handle is stored and later closed.
        unsafe {
            SetLastError(0);
            let handle = CreateMutexW(std::ptr::null(), 1, wide.as_ptr());
            if handle.is_null() {
                continue;
            }
            let owned = GetLastError() != ERROR_ALREADY_EXISTS;
            mutexes.push(MutexHandle { handle, owned });
        }
    }
    if mutexes.is_empty() {
        return Err(AppError::new("MUTEX_FAILED", "Could Not Start", "No Roblox singleton mutex could be created."));
    }

    crate::log_info!("Multi Roblox started (mutex mode)");
    Ok(MultiGuard {
        mode: MultiMethod::Mutex,
        mutexes,
        _login_files: Vec::new(),
        launch_gate: Mutex::new(false),
        monitor_stop: None,
        monitor_ready: Arc::new(Mutex::new(std::collections::HashSet::new())),
    })
}

#[cfg(windows)]
fn enable_handle() -> AppResult<MultiGuard> {
    if !super::handles::is_elevated() {
        return Err(AppError::new(
            "NEEDS_ADMIN",
            "Administrator Required",
            "Handle mode needs Rodon to run as administrator. Restart it as admin or use Mutex mode.",
        ));
    }
    let stop = Arc::new(AtomicBool::new(false));
    let ready = Arc::new(Mutex::new(std::collections::HashSet::new()));
    let worker_stop = Arc::clone(&stop);
    let worker_ready = Arc::clone(&ready);
    std::thread::Builder::new()
        .name("multi-handle-monitor".into())
        .spawn(move || handle_monitor(worker_stop, worker_ready))
        .map_err(|e| AppError::unexpected("start handle monitor", e))?;
    crate::log_info!("Multi Roblox started (handle mode)");
    Ok(MultiGuard {
        mode: MultiMethod::Handle,
        mutexes: Vec::new(),
        _login_files: Vec::new(),
        launch_gate: Mutex::new(false),
        monitor_stop: Some(stop),
        monitor_ready: ready,
    })
}

#[cfg(windows)]
fn handle_monitor(stop: Arc<AtomicBool>, ready: Arc<Mutex<std::collections::HashSet<super::process::ProcId>>>) {
    let mut done: std::collections::HashSet<super::process::ProcId> = std::collections::HashSet::new();
    while !stop.load(Ordering::SeqCst) {
        let current: Vec<super::process::ProcId> = super::process::list_roblox().into_iter().map(|p| p.id).collect();
        done.retain(|id| current.contains(id));
        {
            let mut set = ready.lock().unwrap_or_else(|p| p.into_inner());
            set.retain(|id| current.contains(id));
        }
        for id in &current {
            if done.contains(id) {
                continue;
            }
            super::handles::close_singleton_handles(id.pid);
            if super::handles::singleton_handles_clear(id.pid) {
                done.insert(*id);
                ready.lock().unwrap_or_else(|p| p.into_inner()).insert(*id);
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn singleton_names_cover_event_and_mutex() {
        assert!(super::SINGLETON_NAMES.iter().any(|n| n.contains("Mutex")));
        assert!(super::SINGLETON_NAMES.iter().any(|n| n.contains("Event")));
    }
}
