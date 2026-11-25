//! Actions for the Instances page: hide, show or close a specific running client.

use crate::win::{process::ProcId, window};
use std::collections::HashSet;
use std::sync::{Arc, LazyLock, Mutex, atomic::AtomicBool};
static HIDDEN: LazyLock<Mutex<HashSet<ProcId>>> = LazyLock::new(Mutex::default);

pub fn is_hidden(id: ProcId) -> bool {
    HIDDEN.lock().unwrap_or_else(|p| p.into_inner()).contains(&id)
}

pub fn restore_hidden() {
    let hidden = std::mem::take(&mut *HIDDEN.lock().unwrap_or_else(|p| p.into_inner()));
    for id in hidden {
        if crate::win::process::is_alive(id) {
            show(id.pid);
        }
    }
}

pub fn start_trim(services: &Arc<super::Services>) {
    let core = Arc::clone(&services.core);
    let stop = Arc::new(AtomicBool::new(false));
    let check = stop.clone();
    services.add_worker(super::Worker::spawn("working-set-trim", stop, move || {
        while !super::interruptible_sleep(&check, std::time::Duration::from_secs(15)) {
            let s = core.settings();
            if s.optimize_ram {
                for (pid, _) in crate::win::process::id_set() {
                    if crate::win::process::memory_mb(pid).is_some_and(|used| used >= f64::from(s.ram_limit_mb)) {
                        crate::win::process::trim_working_set(pid, s.ram_limit_mb);
                    }
                }
            }
        }
    }));
}

/// Hides every window of a client (headless).
pub fn hide(pid: u32) {
    let Some(&created) = crate::win::process::id_set().get(&pid) else {
        return;
    };
    HIDDEN.lock().unwrap_or_else(|p| p.into_inner()).insert(ProcId { pid, created });
    for w in window::windows_for(&[pid]) {
        window::show(w.hwnd, false);
    }
}

/// Shows every window of a client again.
pub fn show(pid: u32) {
    HIDDEN.lock().unwrap_or_else(|p| p.into_inner()).retain(|id| id.pid != pid);
    for w in window::windows_for(&[pid]) {
        window::show(w.hwnd, true);
    }
}

/// Closes one client.
pub fn close(pid: u32) -> bool {
    let created = crate::win::process::list_roblox().into_iter().find(|p| p.id.pid == pid).map(|p| p.id.created).unwrap_or(0);
    crate::win::process::kill(ProcId { pid, created })
}
