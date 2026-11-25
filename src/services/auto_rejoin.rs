//! Per-account Auto-Rejoin: relaunch an account when its client exits or it leaves
//! the target place. One worker thread per enabled account.

use super::{Services, interruptible_sleep};
use crate::core::Core;
use crate::roblox::account as api;
use crate::roblox::launch::LaunchRequest;
use crate::store::model::RejoinConfig;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Tracks the running Auto-Rejoin workers so they can be started and stopped per account.
#[derive(Default)]
pub struct RejoinManager {
    workers: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl RejoinManager {
    pub fn is_running(&self, account: &str) -> bool {
        self.workers.lock().unwrap_or_else(|p| p.into_inner()).contains_key(account)
    }

    pub fn running_accounts(&self) -> Vec<String> {
        self.workers.lock().unwrap_or_else(|p| p.into_inner()).keys().cloned().collect()
    }

    pub fn start(self: &Arc<Self>, services: &Arc<Services>, config: RejoinConfig) {
        let account = config.account.clone();
        if account.is_empty() {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut workers = self.workers.lock().unwrap_or_else(|p| p.into_inner());
            if workers.contains_key(&account) {
                return;
            }
            workers.insert(account.clone(), stop.clone());
        }
        let core = Arc::clone(&services.core);
        let live = Arc::clone(&services.live);
        let services = Arc::clone(services);
        let manager = Arc::clone(self);
        let worker = std::thread::Builder::new().name(format!("auto-rejoin-{account}")).spawn(move || {
            run(&core, &stop, &config, |status| {
                live.set_rejoin_status(&config.account, status);
                services.request_repaint();
            });
            live.clear_rejoin_status(&config.account);
            manager.workers.lock().unwrap_or_else(|p| p.into_inner()).remove(&config.account);
            services.request_repaint();
        });
        if let Err(e) = worker {
            self.workers.lock().unwrap_or_else(|p| p.into_inner()).remove(&account);
            crate::log_error!("Could not start Auto-Rejoin: {e}");
        }
    }

    pub fn stop(&self, account: &str) {
        if let Some(stop) = self.workers.lock().unwrap_or_else(|p| p.into_inner()).get(account) {
            stop.store(true, Ordering::SeqCst);
        }
    }

    pub fn stop_all(&self) {
        for stop in self.workers.lock().unwrap_or_else(|p| p.into_inner()).values() {
            stop.store(true, Ordering::SeqCst);
        }
    }
}

fn run(core: &Core, stop: &AtomicBool, config: &RejoinConfig, mut status: impl FnMut(&str)) {
    let Some(account) = core.account(&config.account) else {
        status("Account not found");
        return;
    };
    if config.place_id == 0 && config.target_user.trim().is_empty() {
        status("No place set");
        return;
    }
    let request = LaunchRequest {
        place_id: config.place_id.to_string(),
        private_server: config.private_server.clone(),
        job_id: config.job_id.clone(),
    };
    let interval = Duration::from_secs(config.check_interval_secs.max(5) as u64);
    let mut retries = 0u32;
    let mut consecutive_absent = 0u32;

    // Stagger a little so simultaneous starts don't all launch at once.
    if interruptible_sleep(stop, Duration::from_secs_f32(2.0 + rand::random::<f32>() * 4.0)) {
        return;
    }

    status(&format!("Active — place {}", config.place_id));
    while !stop.load(Ordering::SeqCst) {
        let destination = if config.target_user.trim().is_empty() {
            Some(request.clone())
        } else {
            match core.resolve_join_user(&account.cookie, config.target_user.trim()) {
                Ok(r) => Some(r),
                Err(err) => {
                    status(&format!("Waiting for {}: {}", config.target_user, err.message));
                    None
                }
            }
        };
        let Some(destination) = destination else {
            if interruptible_sleep(stop, interval) {
                return;
            }
            continue;
        };
        let Some(present) = account_is_present(&account, config, &destination) else {
            status("Presence unavailable");
            if interruptible_sleep(stop, interval) {
                return;
            }
            continue;
        };
        if present {
            retries = 0;
            consecutive_absent = 0;
        } else {
            consecutive_absent += 1;
            // Two consecutive misses before acting, to ride out a brief teleport.
            if consecutive_absent >= 2 {
                if retries >= config.max_retries {
                    status("Stopped — max retries");
                    return;
                }

                if config.check_internet && !has_internet(core) {
                    status("Waiting for internet");
                } else {
                    retries += 1;
                    status(&format!("Rejoining ({retries}/{})", config.max_retries));
                    if config.check_presence
                        && let Some(stale) = crate::roblox::logs::confirmed_client(account.user_id)
                        && !crate::win::process::kill(stale)
                    {
                        status("Waiting for the previous client to close");
                        if interruptible_sleep(stop, interval) {
                            return;
                        }
                        continue;
                    }
                    match core.launch(&config.account, &destination) {
                        Ok(_) => {
                            status(&format!("Active — place {}", config.place_id));
                            consecutive_absent = 0;
                            // Give the client time to come up before checking again.
                            if interruptible_sleep(stop, Duration::from_secs(12)) {
                                return;
                            }
                        }
                        Err(err) => {
                            status(&format!("Launch failed: {}", err.message));
                            if matches!(
                                err.code.as_str(),
                                "LAUNCH_PAUSED" | "ROBLOX_ACCOUNT_MISMATCH" | "ROBLOX_SECURITY_CHALLENGE" | "ROBLOX_LAUNCH_UNCONFIRMED"
                            ) {
                                return;
                            }
                        }
                    }
                }
            }
        }
        if interruptible_sleep(stop, interval) {
            return;
        }
    }
}

fn account_is_present(account: &crate::store::model::Account, config: &RejoinConfig, destination: &LaunchRequest) -> Option<bool> {
    if config.check_presence && account.user_id > 0 && !account.cookie.is_empty() {
        let p = api::presence(account.user_id, &account.cookie).ok()?;
        if !p.in_game {
            return Some(false);
        }
        if !config.target_user.is_empty() && !destination.job_id.is_empty() {
            return Some(p.job_id == destination.job_id);
        }
        if config.check_place_id {
            let expected = destination.place_id.parse().ok()?;
            return Some(p.place_id == Some(expected) || p.root_place_id == Some(expected));
        }
        return Some(true);
    }
    Some(super::activity::match_processes().values().any(|uid| *uid == account.user_id))
}

fn has_internet(core: &Core) -> bool {
    for url in core.settings().connectivity_check_urls {
        if crate::roblox::agent().get(&url).call().is_ok() {
            return true;
        }
    }
    false
}
