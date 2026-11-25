//! Self-update from the fork's GitHub releases only, verified by SHA-256, with a
//! rollback copy kept until the new build starts.

use super::Services;
use crate::error::{AppError, AppResult};
use crate::roblox::agent;
use serde::Deserialize;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

const TRUSTED_HOSTS: [&str; 2] = ["github.com", "objects.githubusercontent.com"];

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub exe_url: String,
    pub sha256_url: String,
    pub notes: String,
}

#[derive(Deserialize)]
struct Release {
    #[serde(rename = "tag_name", default)]
    tag: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize, Clone)]
struct Asset {
    #[serde(default)]
    name: String,
    #[serde(rename = "browser_download_url", default)]
    url: String,
}

/// Parses `v1.2.3` / `1.2.3` into comparable parts.
fn parts(version: &str) -> Vec<u64> {
    version.trim().trim_start_matches(['v', 'V']).split(['.', '-', '+']).filter_map(|p| p.parse().ok()).collect()
}

/// True when `latest` is newer than `current`.
pub fn is_newer(current: &str, latest: &str) -> bool {
    parts(latest) > parts(current)
}

fn host_of(url: &str) -> &str {
    url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or("")
}

fn trusted(url: &str) -> bool {
    if !url.starts_with("https://") {
        return false;
    }
    let host = host_of(url);
    TRUSTED_HOSTS.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

/// Queries the fork's releases and returns an update if one is newer than this build.
pub fn check(beta: bool) -> AppResult<Option<UpdateInfo>> {
    let url = format!("https://api.github.com/repos/{}/releases?per_page=10", crate::REPO);
    let mut response = agent().get(&url).header("Accept", "application/vnd.github+json").call().map_err(|_| {
        AppError::new("UPDATE_CHECK_FAILED", "Update Check Failed", "Could not reach GitHub to check for updates.").retryable()
    })?;
    if response.status().as_u16() != 200 {
        return Err(AppError::new("UPDATE_CHECK_FAILED", "Update Check Failed", "GitHub did not return the releases.").retryable());
    }
    let releases: Vec<Release> = response.body_mut().read_json().map_err(|e| AppError::unexpected("release list", e))?;
    Ok(releases.into_iter().filter(|r| beta || !r.prerelease).find_map(|release| {
        if !is_newer(crate::VERSION, &release.tag) {
            return None;
        }
        let version = release.tag.trim_start_matches(['v', 'V']);
        let asset = format!("Rodon-v{version}.exe");
        let checksum = format!("{asset}.sha256");
        let exe = release.assets.iter().find(|a| a.name == asset && trusted(&a.url))?;
        let sha = release.assets.iter().find(|a| a.name == checksum && trusted(&a.url))?;
        Some(UpdateInfo {
            version: version.to_owned(),
            exe_url: exe.url.clone(),
            sha256_url: sha.url.clone(),
            notes: if release.body.is_empty() { release.name } else { release.body },
        })
    }))
}

/// One-shot background check that records any available update for the UI.
pub fn start_check(services: &Arc<Services>) {
    let stop = Arc::new(AtomicBool::new(false));
    let beta = services.core.settings().update_channel == crate::store::settings::UpdateChannel::Beta;
    let skipped = services.core.settings().skipped_version;
    let worker_services = Arc::clone(services);
    let worker = super::Worker::spawn("update-check", stop, move || {
        if let Ok(Some(info)) = check(beta)
            && info.version != skipped
        {
            *worker_services.live.update_notes.write().unwrap_or_else(|e| e.into_inner()) = info.notes;
            worker_services.live.set_update_available(Some(info.version));
            worker_services.request_repaint();
        }
    });
    services.add_worker(worker);
}

fn download(url: &str, limit: usize) -> AppResult<Vec<u8>> {
    if !trusted(url) {
        return Err(AppError::new("UPDATE_UNTRUSTED", "Untrusted Download", "The update URL is not a GitHub release."));
    }
    let mut response = agent()
        .get(url)
        .call()
        .map_err(|_| AppError::new("UPDATE_DOWNLOAD_FAILED", "Download Failed", "The update could not be downloaded.").retryable())?;
    let bytes = response.body_mut().with_config().limit(limit as u64).read_to_vec().map_err(|e| {
        AppError::new("UPDATE_DOWNLOAD_FAILED", "Download Failed", "The update could not be downloaded.").with_detail(e.to_string())
    })?;
    Ok(bytes)
}

/// Downloads the update, checks its SHA-256, and swaps the running exe. On Windows
/// a Rodon helper process waits for exit, replaces the exe (keeping a
/// `.old` rollback copy) and relaunches. Returns once the helper has been started.
pub fn download_and_apply(info: &UpdateInfo) -> AppResult<()> {
    use sha2::{Digest, Sha256};
    let exe_bytes = download(&info.exe_url, 200 * 1024 * 1024)?;
    let expected = String::from_utf8_lossy(&download(&info.sha256_url, 4096)?).split_whitespace().next().unwrap_or_default().to_lowercase();
    if expected.len() != 64 {
        return Err(AppError::new("UPDATE_NO_CHECKSUM", "No Checksum", "The release did not publish a usable SHA-256."));
    }
    let actual = format!("{:x}", Sha256::digest(&exe_bytes));
    if actual != expected {
        return Err(AppError::new(
            "UPDATE_CHECKSUM_MISMATCH",
            "Update Rejected",
            "The downloaded update failed its checksum and was discarded.",
        ));
    }
    let current = crate::paths::exe_path();
    let staged = current.with_extension("new");
    std::fs::write(&staged, &exe_bytes).map_err(|e| AppError::io("Writing the update", &e))?;
    apply_swap(&current, &staged)
}

#[cfg(windows)]
fn apply_swap(current: &std::path::Path, _staged: &std::path::Path) -> AppResult<()> {
    use std::os::windows::process::CommandExt;
    let helper = current.with_extension("rodon-update-helper.exe");
    std::fs::copy(current, &helper).map_err(|e| AppError::io("Preparing update helper", &e))?;
    std::process::Command::new(&helper)
        .arg("--finish-rodon-update")
        .arg(std::process::id().to_string())
        .args(std::env::args_os().skip(1))
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|e| AppError::io("Starting update helper", &e))?;
    Ok(())
}

pub fn finish_update(args: &[std::ffi::OsString]) -> std::io::Result<()> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let exe = std::env::current_exe()?;
    let stem = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_suffix(".rodon-update-helper"))
        .ok_or_else(|| std::io::Error::other("Invalid update helper name"))?;
    let pid = args
        .first()
        .and_then(|s| s.to_str())
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| std::io::Error::other("Missing process ID"))?;
    // Wait only for the instance that staged this update.
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !process.is_null() {
            let wait = WaitForSingleObject(process, 600000);
            CloseHandle(process);
            if wait == WAIT_TIMEOUT {
                return Err(std::io::Error::other("Timed out waiting for Rodon"));
            }
        }
    }
    let current = exe.with_file_name(format!("{stem}.exe"));
    let staged = current.with_extension("new");
    let old = current.with_extension("old");
    if !staged.is_file() {
        return Err(std::io::Error::other("Staged update is missing"));
    }
    std::fs::rename(&current, &old)?;
    if let Err(e) = std::fs::rename(&staged, &current) {
        std::fs::rename(&old, &current)?;
        return Err(e);
    }
    if let Err(e) = std::process::Command::new(&current).args(&args[1..]).spawn() {
        std::fs::rename(&current, &staged)?;
        std::fs::rename(&old, &current)?;
        std::process::Command::new(&current).args(&args[1..]).spawn()?;
        return Err(e);
    }
    Ok(())
}

#[cfg(not(windows))]
fn apply_swap(_current: &std::path::Path, _staged: &std::path::Path) -> AppResult<()> {
    Err(AppError::new("WINDOWS_ONLY", "Windows Only", "Self-update is only supported on Windows."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("3.0.0", "3.0.1"));
        assert!(is_newer("3.0.0", "v3.1.0"));
        assert!(!is_newer("3.0.0", "3.0.0"));
        assert!(!is_newer("3.1.0", "3.0.9"));
    }

    #[test]
    fn only_github_hosts_are_trusted() {
        assert!(trusted("https://github.com/x/y/releases/download/v1/Rodon.exe"));
        assert!(trusted("https://objects.githubusercontent.com/abc"));
        assert!(!trusted("https://evil.example.com/Rodon.exe"));
        assert!(!trusted("https://github.com.evil.com/x"));
    }
}
