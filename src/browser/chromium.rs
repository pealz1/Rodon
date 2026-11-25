//! Downloads a portable Chromium build (and its matching chromedriver) from the
//! Chromium snapshot storage, for browser sign-in when no Chrome/Edge is installed.

use crate::error::{AppError, AppResult};
use crate::roblox::agent;
use std::path::PathBuf;

const SNAPSHOT_ROOT: &str = "https://storage.googleapis.com/chromium-browser-snapshots/Win_x64";

pub fn root() -> PathBuf {
    crate::paths::data_file("Chromium")
}

fn build_marker() -> PathBuf {
    root().join("snapshot_build.txt")
}

/// The portable chrome.exe, if installed.
pub fn chrome_exe() -> Option<PathBuf> {
    find(&root(), "chrome.exe")
}

/// The portable chromedriver.exe, if installed.
pub fn chromedriver_exe() -> Option<PathBuf> {
    find(&root(), "chromedriver.exe")
}

pub fn is_installed() -> bool {
    chrome_exe().is_some_and(|p| p.exists())
}

pub fn installed_build() -> Option<String> {
    std::fs::read_to_string(build_marker())
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty())
}

fn error(message: &str) -> AppError {
    AppError::new("CHROMIUM_DOWNLOAD_FAILED", "Chromium Download Failed", message.to_owned())
}

fn latest_build() -> AppResult<String> {
    let mut response =
        agent().get(&format!("{SNAPSHOT_ROOT}/LAST_CHANGE")).call().map_err(|_| error("Could not reach the Chromium snapshot server."))?;
    let build = response.body_mut().read_to_string().map_err(|e| error(&format!("Unreadable build id: {e}")))?.trim().to_owned();
    if build.chars().all(|c| c.is_ascii_digit()) && !build.is_empty() {
        Ok(build)
    } else {
        Err(error("The snapshot server returned an unexpected build id."))
    }
}

fn fetch(url: &str) -> AppResult<Vec<u8>> {
    let mut response = agent().get(url).call().map_err(|_| error(&format!("Could not download {url}")))?;
    if response.status().as_u16() != 200 {
        return Err(error(&format!("Download returned HTTP {}", response.status().as_u16())));
    }
    response.body_mut().with_config().limit(400 * 1024 * 1024).read_to_vec().map_err(|e| error(&format!("Download failed: {e}")))
}

/// Downloads and installs the latest portable Chromium + chromedriver.
pub fn download(progress: &dyn Fn(u8, &str)) -> AppResult<PathBuf> {
    progress(1, "Finding latest build");
    let build = latest_build()?;
    let root = root();
    let parent = root.parent().ok_or_else(|| error("Invalid Chromium folder"))?;
    std::fs::create_dir_all(parent).map_err(|e| AppError::io("Creating download folder", &e))?;
    let stage = tempfile::Builder::new()
        .prefix(".rodon-chromium-")
        .tempdir_in(parent)
        .map_err(|e| AppError::io("Preparing Chromium download", &e))?;
    progress(10, "Downloading Chromium");
    let chrome = fetch(&format!("{SNAPSHOT_ROOT}/{build}/chrome-win.zip"))?;
    crate::services::downloader::extract_zip(&chrome, stage.path(), "chrome-win.zip")?;
    drop(chrome);
    progress(75, "Downloading matching driver");
    let driver = fetch(&format!("{SNAPSHOT_ROOT}/{build}/chromedriver_win32.zip"))?;
    crate::services::downloader::extract_zip(&driver, stage.path(), "chromedriver_win32.zip")?;
    if find(stage.path(), "chrome.exe").is_none() || find(stage.path(), "chromedriver.exe").is_none() {
        return Err(error("The download is missing Chromium or its matching driver."));
    }
    std::fs::write(stage.path().join("snapshot_build.txt"), &build).map_err(|e| AppError::io("Saving Chromium version", &e))?;
    crate::services::downloader::install_directory(stage, &root)?;
    progress(100, "Done");
    chrome_exe().ok_or_else(|| error("Installed Chromium not found"))
}

/// Finds a file by name under `dir` (two levels deep is enough for these zips).
fn find(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.exists() {
        return Some(direct);
    }
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            let nested = path.join(name);
            if nested.exists() {
                return Some(nested);
            }
            // one more level (chrome-win/<build>/chrome.exe layouts)
            for sub in std::fs::read_dir(&path).into_iter().flatten().flatten() {
                let candidate = sub.path().join(name);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}
