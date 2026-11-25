//! Downloads and installs a Roblox WindowsPlayer deployment (port of
//! `roblox_downloader.py`). Fetches the package manifest, downloads each package
//! and extracts it into the version folder, then writes AppSettings.xml.

use crate::error::{AppError, AppResult};
use crate::roblox::agent;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

const CLIENT_VERSION_URL: &str = "https://clientsettings.roblox.com/v2/client-version/WindowsPlayer/";
const HOST: &str = "https://setup-aws.rbxcdn.com";

const APP_SETTINGS_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Settings>\n\t<ContentFolder>content</ContentFolder>\n\t<BaseUrl>http://www.roblox.com</BaseUrl>\n</Settings>\n";

static VERSION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^version-[A-Za-z0-9]+$").unwrap());
static PACKAGE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_.-]+\.zip$").unwrap());

/// Where each package's contents are extracted, relative to the version folder.
fn extract_root(package: &str) -> Option<&'static str> {
    Some(match package {
        "RobloxApp.zip" | "redist.zip" | "WebView2.zip" => "",
        "shaders.zip" => "shaders/",
        "ssl.zip" => "ssl/",
        "WebView2RuntimeInstaller.zip" => "WebView2RuntimeInstaller/",
        "content-avatar.zip" => "content/avatar/",
        "content-configs.zip" => "content/configs/",
        "content-fonts.zip" => "content/fonts/",
        "content-sky.zip" => "content/sky/",
        "content-sounds.zip" => "content/sounds/",
        "content-textures2.zip" => "content/textures/",
        "content-models.zip" => "content/models/",
        "content-platform-fonts.zip" => "PlatformContent/pc/fonts/",
        "content-platform-dictionaries.zip" => "PlatformContent/pc/shared_compression_dictionaries/",
        "content-terrain.zip" => "PlatformContent/pc/terrain/",
        "content-textures3.zip" => "PlatformContent/pc/textures/",
        "extracontent-luapackages.zip" => "ExtraContent/LuaPackages/",
        "extracontent-translations.zip" => "ExtraContent/translations/",
        "extracontent-models.zip" => "ExtraContent/models/",
        "extracontent-textures.zip" => "ExtraContent/textures/",
        "extracontent-places.zip" => "ExtraContent/places/",
        _ => return None,
    })
}

pub fn default_versions_dir() -> Option<PathBuf> {
    crate::paths::local_appdata().map(|p| p.join("Roblox").join("Versions"))
}

fn error(message: &str) -> AppError {
    AppError::new("ROBLOX_DOWNLOAD_FAILED", "Roblox Download Failed", message.to_owned())
}

/// Resolves "LIVE" (or an empty string) to the current version, or normalises a hash.
pub fn resolve_version(input: &str) -> AppResult<String> {
    let value = input.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("live") {
        return latest_version();
    }
    let normalised =
        if value.to_lowercase().starts_with("version-") { value.to_lowercase() } else { format!("version-{}", value.to_lowercase()) };
    if VERSION_RE.is_match(&normalised) {
        Ok(normalised)
    } else {
        Err(AppError::invalid("ROBLOX_VERSION_INVALID", "The Roblox version hash contains invalid characters."))
    }
}

fn latest_version() -> AppResult<String> {
    let mut response = agent().get(CLIENT_VERSION_URL).call().map_err(|_| error("Could not reach Roblox ClientSettings."))?;
    let value: serde_json::Value = response.body_mut().read_json().map_err(|e| error(&format!("Unreadable version response: {e}")))?;
    let version = value.get("clientVersionUpload").and_then(|v| v.as_str()).unwrap_or_default();
    if !VERSION_RE.is_match(version) {
        return Err(error("Roblox returned an invalid LIVE version."));
    }
    Ok(version.to_owned())
}

fn fetch_manifest(version: &str) -> AppResult<(String, Vec<String>)> {
    for base in [format!("{HOST}/{version}-"), format!("{HOST}/channel/common/{version}-")] {
        let Ok(mut response) = agent().get(&format!("{base}rbxPkgManifest.txt")).call() else {
            continue;
        };
        if response.status().as_u16() != 200 {
            continue;
        }
        let text = response.body_mut().read_to_string().map_err(|e| error(&format!("Unreadable manifest: {e}")))?;
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        if lines.first() != Some(&"v0") {
            return Err(error("Roblox returned an unsupported package manifest."));
        }
        if !lines.contains(&"RobloxApp.zip") {
            return Err(error("The selected version is not a WindowsPlayer build."));
        }
        let mut packages = Vec::new();
        for line in lines.iter().filter(|l| l.ends_with(".zip")) {
            if !PACKAGE_RE.is_match(line) {
                return Err(error(&format!("The manifest has an unsafe package name: {line}")));
            }
            if extract_root(line).is_none() {
                return Err(error(&format!("The manifest has an unsupported package: {line}")));
            }
            packages.push((*line).to_owned());
        }
        if packages.is_empty() {
            return Err(error("The manifest contains no packages."));
        }
        return Ok((base, packages));
    }
    Err(error("Could not fetch the Roblox package manifest."))
}

/// Downloads and installs a version into `versions_dir/<version>`. `progress(percent, text)`
/// is called as it goes. Returns the install path.
pub fn download(version: &str, versions_dir: &Path, progress: &dyn Fn(u8, &str)) -> AppResult<PathBuf> {
    let version = resolve_version(version)?;
    progress(1, "Fetching manifest");
    let (base, packages) = fetch_manifest(&version)?;
    let install = versions_dir.join(&version);
    std::fs::create_dir_all(versions_dir).map_err(|e| AppError::io("Creating the versions folder", &e))?;
    let stage = tempfile::Builder::new()
        .prefix(".rodon-player-")
        .tempdir_in(versions_dir)
        .map_err(|e| AppError::io("Preparing Roblox download", &e))?;

    let per = 96.0 / packages.len() as f32;
    for (index, package) in packages.iter().enumerate() {
        let start = 2.0 + index as f32 * per;
        progress(start as u8, &format!("Downloading {package}"));
        let bytes = download_package(&format!("{base}{package}"))?;
        let root = extract_root(package).unwrap_or("");
        extract_zip(&bytes, &stage.path().join(root), package)?;
        progress((start + per) as u8, &format!("Installed {package}"));
    }
    std::fs::write(stage.path().join("AppSettings.xml"), APP_SETTINGS_XML).map_err(|e| AppError::io("Writing AppSettings.xml", &e))?;
    if !stage.path().join("RobloxPlayerBeta.exe").is_file() {
        return Err(error("The downloaded deployment is missing RobloxPlayerBeta.exe."));
    }
    install_directory(stage, &install)?;
    progress(100, "Done");
    Ok(install)
}

/// Stage on the destination volume. Preserve the previous installation, including custom files.
pub(crate) fn install_directory(stage: tempfile::TempDir, dest: &Path) -> AppResult<()> {
    let parent = dest.parent().ok_or_else(|| error("Invalid installation path"))?;
    let mut old = None;
    if dest.exists() {
        let backup = parent.join(format!(".rodon-previous-{:016x}", rand::random::<u64>()));
        std::fs::rename(dest, &backup).map_err(|e| AppError::io("Saving previous installation", &e))?;
        old = Some(backup);
    }
    if let Err(e) = std::fs::rename(stage.path(), dest) {
        if let Some(path) = old {
            std::fs::rename(path, dest).map_err(|e| AppError::io("Restoring previous installation", &e))?;
        }
        return Err(AppError::io("Installing staged files", &e));
    }
    if let Some(path) = old {
        crate::log_info!("Previous installation saved in {}", path.display());
    }
    Ok(())
}

fn download_package(url: &str) -> AppResult<Vec<u8>> {
    let mut response = agent().get(url).call().map_err(|_| error(&format!("Could not download {url}")))?;
    response.body_mut().with_config().limit(512 * 1024 * 1024).read_to_vec().map_err(|e| error(&format!("Download failed: {e}")))
}

/// Extracts a zip into `dest`, refusing any entry that escapes it (zip-slip guard).
pub(crate) fn extract_zip(bytes: &[u8], dest: &Path, package: &str) -> AppResult<()> {
    let dest = dest.to_path_buf();
    std::fs::create_dir_all(&dest).map_err(|e| AppError::io("Creating the extract folder", &e))?;
    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| error(&format!("{package} is not a valid zip: {e}")))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| error(&format!("Reading {package}: {e}")))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let relative = Path::new(&name);
        if name.contains(':')
            || relative.components().any(|p| !matches!(p, std::path::Component::Normal(_) | std::path::Component::CurDir))
            || entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(error("Unsafe archive entry"));
        }
        let target = dest.join(relative);
        // Resolve and confirm the target stays inside dest.
        let normalized = normalize(&target);
        if !normalized.starts_with(&dest) {
            return Err(error(&format!("Unsafe path in {package}: {name}")));
        }
        if let Some(parent) = normalized.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::io("Creating a folder", &e))?;
        }
        let mut out = std::fs::File::create(&normalized).map_err(|e| AppError::io("Writing a file", &e))?;
        let mut buf = [0u8; 1 << 16];
        loop {
            let read = entry.read(&mut buf).map_err(|e| error(&format!("Extracting {package}: {e}")))?;
            if read == 0 {
                break;
            }
            use std::io::Write;
            out.write_all(&buf[..read]).map_err(|e| AppError::io("Writing a file", &e))?;
        }
    }
    Ok(())
}

/// Lexically normalises a path (resolving `.`/`..`) without touching the filesystem.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_normalisation() {
        assert_eq!(resolve_version("abc123").unwrap(), "version-abc123");
        assert_eq!(resolve_version("version-ABC").unwrap(), "version-abc");
        assert!(resolve_version("bad hash!").is_err());
    }

    #[test]
    fn extract_roots_cover_core_packages() {
        assert_eq!(extract_root("RobloxApp.zip"), Some(""));
        assert_eq!(extract_root("content-textures2.zip"), Some("content/textures/"));
        assert_eq!(extract_root("evil.zip"), None);
    }

    #[test]
    fn normalize_blocks_traversal() {
        let dest = PathBuf::from("/install");
        let escaped = normalize(&dest.join("../etc/passwd"));
        assert!(!escaped.starts_with(&dest));
        let inside = normalize(&dest.join("content/a.txt"));
        assert!(inside.starts_with(&dest));
    }
}
