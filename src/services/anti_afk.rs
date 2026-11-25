//! Anti-AFK: periodically sends a key or mouse action to each Roblox window to
//! keep the session from being kicked for inactivity.

use crate::store::settings::AntiAfk;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// A running Anti-AFK loop; drop or `stop` to end it.
pub struct AntiAfkHandle {
    deadline: Arc<std::sync::Mutex<std::time::Instant>>,
    stop: Arc<AtomicBool>,
}

impl AntiAfkHandle {
    pub fn remaining(&self) -> u64 {
        self.deadline.lock().unwrap_or_else(|p| p.into_inner()).saturating_duration_since(std::time::Instant::now()).as_secs()
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for AntiAfkHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn start(config: AntiAfk) -> AntiAfkHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let body_stop = Arc::clone(&stop);
    let deadline = Arc::new(std::sync::Mutex::new(std::time::Instant::now()));
    let tick = deadline.clone();
    std::thread::Builder::new().name("anti-afk".into()).spawn(move || run(config, &body_stop, &tick)).ok();
    AntiAfkHandle { stop, deadline }
}

fn run(config: AntiAfk, stop: &AtomicBool, deadline: &std::sync::Mutex<std::time::Instant>) {
    let interval = Duration::from_secs(config.interval_minutes.clamp(1, 60) as u64 * 60);
    loop {
        *deadline.lock().unwrap_or_else(|e| e.into_inner()) = std::time::Instant::now() + interval;
        if super::interruptible_sleep(stop, interval) {
            return;
        }
        maintain(&config, stop);
    }
}

pub fn maintain(config: &AntiAfk, stop: &AtomicBool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, IsIconic, SW_HIDE, SW_MINIMIZE, SW_RESTORE, SetForegroundWindow, ShowWindow,
    };
    let pids: Vec<u32> = crate::win::process::list_roblox().iter().map(|p| p.id.pid).collect();
    let previous = unsafe { GetForegroundWindow() };
    for w in crate::win::window::windows_for(&pids) {
        if stop.load(Ordering::Acquire) {
            break;
        }
        let hwnd = w.hwnd as _;
        let minimized = unsafe { IsIconic(hwnd) != 0 };
        unsafe {
            if !w.visible || minimized {
                ShowWindow(hwnd, SW_RESTORE);
            }
            SetForegroundWindow(hwnd);
        }
        send_action(w.hwnd, config);
        unsafe {
            if !w.visible {
                ShowWindow(hwnd, SW_HIDE);
            } else if minimized {
                ShowWindow(hwnd, SW_MINIMIZE);
            }
        }
    }
    if !previous.is_null() {
        unsafe {
            SetForegroundWindow(previous);
        }
    }
}

/// Sends `config.action` to a single window `config.press_count` times.
#[cfg(windows)]
fn send_action(hwnd: isize, config: &AntiAfk) {
    use windows_sys::Win32::Foundation::{LPARAM, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_KEYDOWN, WM_KEYUP};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_XBUTTONDOWN,
        WM_XBUTTONUP,
    };
    let (down, up, param) = match config.action.as_str() {
        "lmb" => (WM_LBUTTONDOWN, WM_LBUTTONUP, 1),
        "rmb" => (WM_RBUTTONDOWN, WM_RBUTTONUP, 2),
        "mmb" => (WM_MBUTTONDOWN, WM_MBUTTONUP, 16),
        "mback" => (WM_XBUTTONDOWN, WM_XBUTTONUP, 1 << 16),
        "mfwd" => (WM_XBUTTONDOWN, WM_XBUTTONUP, 2 << 16),
        "scroll_up" => (WM_MOUSEWHEEL, 0, 120 << 16),
        "scroll_down" => (WM_MOUSEWHEEL, 0, (-120i32 as u32) << 16),
        name => {
            let Some(key) = virtual_key(name) else { return };
            (WM_KEYDOWN, WM_KEYUP, key)
        }
    };
    for _ in 0..config.press_count.max(1) {
        // SAFETY: posting a key message to a window handle has no pointer args.
        unsafe {
            PostMessageW(hwnd as _, down, param as WPARAM, 0 as LPARAM);
            std::thread::sleep(Duration::from_millis(120));
            if up != 0 {
                PostMessageW(hwnd as _, up, if up == WM_KEYUP || up == WM_XBUTTONUP { param as WPARAM } else { 0 }, 0 as LPARAM);
            }
        }
        std::thread::sleep(Duration::from_millis(80));
    }
}

#[cfg(not(windows))]
fn send_action(_hwnd: isize, _config: &AntiAfk) {}

/// Maps an action name to a virtual-key code. Mouse actions are not posted to
/// background windows reliably, so only keys are supported here.
#[cfg(windows)]
fn virtual_key(action: &str) -> Option<u32> {
    let a = action.trim().to_lowercase();
    Some(match a.as_str() {
        "space" => 0x20,
        "w" => 0x57,
        "a" => 0x41,
        "s" => 0x53,
        "d" => 0x44,
        "up" => 0x26,
        "down" => 0x28,
        "left" => 0x25,
        "right" => 0x27,
        "shift" => 0x10,
        "ctrl" => 0x11,
        single if single.len() == 1 && single.as_bytes()[0].is_ascii_alphanumeric() => single.as_bytes()[0].to_ascii_uppercase() as u32,
        f if f.starts_with('f') && f[1..].parse::<u32>().is_ok() => {
            let n = f[1..].parse::<u32>().unwrap();
            if (1..=12).contains(&n) { 0x70 + n - 1 } else { return None }
        }
        _ => return None,
    })
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn maps_common_keys() {
        assert_eq!(super::virtual_key("space"), Some(0x20));
        assert_eq!(super::virtual_key("W"), Some(0x57));
        assert_eq!(super::virtual_key("f5"), Some(0x74));
        assert_eq!(super::virtual_key("mmb"), None);
    }
}
