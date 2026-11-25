use crate::services::{Services, Worker};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use windows_sys::Win32::UI::{
    Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey},
    WindowsAndMessaging::{MSG, PM_REMOVE, PeekMessageW, WM_HOTKEY},
};

fn binding(text: &str) -> Option<(u32, u32)> {
    let mut mods = 0;
    let mut key = None;
    for part in text.to_ascii_uppercase().split('+').map(str::trim) {
        match part {
            "CTRL" | "CONTROL" => mods |= MOD_CONTROL,
            "SHIFT" => mods |= MOD_SHIFT,
            "ALT" => mods |= MOD_ALT,
            "WIN" => mods |= MOD_WIN,
            s if key.is_none() => {
                key = Some(if s.len() == 1 && s.as_bytes()[0].is_ascii_alphanumeric() {
                    u32::from(s.as_bytes()[0])
                } else {
                    let n = s.strip_prefix('F')?.parse::<u32>().ok()?;
                    if !(1..=24).contains(&n) {
                        return None;
                    }
                    0x70 + n - 1
                })
            }
            _ => return None,
        }
    }
    Some((mods | MOD_NOREPEAT, key?))
}
pub fn start(services: &Arc<Services>) {
    let stop = Arc::new(AtomicBool::new(false));
    let quit = stop.clone();
    let weak = Arc::downgrade(services);
    services.add_worker(Worker::spawn("window-grid-shortcut", stop, move || {
        let mut last = None;
        let mut registered = false;
        while !quit.load(Ordering::Acquire) {
            let Some(s) = weak.upgrade() else { break };
            let cfg = s.core.settings();
            let requested = cfg.window_grid_hotkey_enabled.then_some(cfg.window_grid_hotkey);
            if requested != last {
                if registered {
                    unsafe {
                        UnregisterHotKey(std::ptr::null_mut(), 11);
                    }
                    registered = false;
                }
                if let Some(input) = &requested {
                    if let Some((mods, vk)) = binding(input) {
                        registered = unsafe { RegisterHotKey(std::ptr::null_mut(), 11, mods, vk) } != 0;
                    }
                    if !registered {
                        s.live.toast("Window grid shortcut is unavailable", true);
                        s.request_repaint();
                    }
                }
                last = requested;
            }
            let mut msg: MSG = unsafe { std::mem::zeroed() };
            while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), WM_HOTKEY, WM_HOTKEY, PM_REMOVE) } != 0 {
                if msg.wParam == 11
                    && let Err(e) = super::grid::tile_roblox_windows()
                {
                    s.live.toast(&e.message, true);
                    s.request_repaint();
                }
            }
            drop(s);
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
        if registered {
            unsafe {
                UnregisterHotKey(std::ptr::null_mut(), 11);
            }
        }
    }));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_configurable_key() {
        assert_eq!(binding("Ctrl+Shift+A"), Some((MOD_NOREPEAT | MOD_CONTROL | MOD_SHIFT, 65)));
        assert!(binding("Ctrl+garbage").is_none());
    }
}
