//! Data directory resolution (port of `app_paths.py`).
//!
//! Priority: `--data-dir <path>` > `RODON_DATA_DIR` > `<exe dir>\RodonData`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const DATA_DIR_ENV: &str = "RODON_DATA_DIR";
pub const DATA_DIR_FLAG: &str = "--data-dir";
const DEFAULT_DIR_NAME: &str = "RodonData";

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn exe_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("Rodon.exe"))
}

pub fn exe_dir() -> PathBuf {
    exe_path().parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

/// Expands `~` and `%VAR%` and makes the path absolute.
pub fn expand(raw: &str) -> PathBuf {
    let mut text = raw.trim().trim_matches('"').to_owned();
    if let Some(rest) = text.strip_prefix('~')
        && let Ok(home) = std::env::var("USERPROFILE")
    {
        text = format!("{home}{rest}");
    }
    let mut out = String::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find('%') {
        let (head, tail) = rest.split_at(start);
        out.push_str(head);
        match tail[1..].find('%') {
            Some(end) => {
                let name = &tail[1..=end];
                match std::env::var(name) {
                    Ok(value) => out.push_str(&value),
                    Err(_) => out.push_str(&tail[..=end + 1]),
                }
                rest = &tail[end + 2..];
            }
            None => {
                out.push_str(tail);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    let path = PathBuf::from(out);
    if path.is_absolute() { path } else { std::env::current_dir().unwrap_or_default().join(path) }
}

/// Removes `--data-dir` from `args` and returns the chosen override, if any.
pub fn take_data_dir_arg(args: &mut Vec<String>) -> Option<String> {
    let mut found = None;
    let mut index = 0;
    while index < args.len() {
        if args[index] == DATA_DIR_FLAG {
            args.remove(index);
            if index < args.len() {
                found = Some(args.remove(index));
            }
        } else if let Some(value) = args[index].strip_prefix("--data-dir=") {
            found = Some(value.to_owned());
            args.remove(index);
        } else {
            index += 1;
        }
    }
    found.filter(|value| !value.trim().is_empty())
}

pub fn init_data_dir(cli_override: Option<String>) -> &'static Path {
    let chosen = cli_override
        .or_else(|| {
            [DATA_DIR_ENV, "NOVA_DATA_DIR", "RAM_DATA_DIR"]
                .into_iter()
                .find_map(|key| std::env::var(key).ok().filter(|s| !s.trim().is_empty()))
        })
        .map(|raw| expand(&raw))
        .unwrap_or_else(|| exe_dir().join(DEFAULT_DIR_NAME));
    let _ = std::fs::create_dir_all(&chosen);
    DATA_DIR.get_or_init(|| chosen)
}

pub fn data_dir() -> &'static Path {
    DATA_DIR.get_or_init(|| {
        let dir = exe_dir().join(DEFAULT_DIR_NAME);
        let _ = std::fs::create_dir_all(&dir);
        dir
    })
}

pub fn data_file(name: &str) -> PathBuf {
    data_dir().join(name)
}

pub fn logs_dir() -> PathBuf {
    data_file("logs")
}

pub fn local_appdata() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

pub static WIPE_ON_EXIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Only remove Rodon's own names, because a custom data directory can contain unrelated files.
pub fn wipe(dir: &Path) -> std::io::Result<()> {
    use std::fs;
    for file in ["vault.json", "vault.json.bak", "vault.json.corrupt", "vault.lock", "settings.json", "RobloxSettings.json"] {
        let path = dir.join(file);
        if path.symlink_metadata().is_ok_and(|m| m.is_file()) {
            fs::remove_file(path)?;
        }
    }
    let root = dir.canonicalize()?;
    for (folder, kind) in [("logs", "log"), ("crash_logs", "log"), ("avatar_cache", "avatar"), ("themes/media", "media")] {
        let folder = dir.join(folder);
        if !folder.canonicalize().is_ok_and(|p| p.starts_with(&root)) || !folder.is_dir() {
            continue;
        }
        for e in fs::read_dir(&folder)? {
            let e = e?;
            if !e.file_type()?.is_file() {
                continue;
            }
            let p = e.path();
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let owned = match kind {
                "avatar" => stem.parse::<u64>().is_ok() && p.extension().is_some_and(|x| x == "png"),
                "media" => stem.len() == 64 && stem.bytes().all(|b| b.is_ascii_hexdigit()),
                _ => {
                    p.extension().is_some_and(|x| x == "log")
                        && ["session-", "update-", "crash-", "ui-hang-"].iter().any(|prefix| stem.starts_with(prefix))
                }
            };
            if owned {
                fs::remove_file(p)?;
            }
        }
        let _ = fs::remove_dir(&folder);
    }
    let chromium = dir.join("Chromium");
    if chromium.canonicalize().is_ok_and(|p| p.starts_with(&root)) && chromium.join("snapshot_build.txt").is_file() {
        fs::remove_dir_all(chromium)?;
    }
    let _ = fs::remove_dir(dir.join("themes"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_flag_is_removed_from_args() {
        let mut args = vec!["app".into(), "--data-dir".into(), "D:/x".into(), "--mcp".into()];
        assert_eq!(take_data_dir_arg(&mut args).as_deref(), Some("D:/x"));
        assert_eq!(args, vec!["app".to_string(), "--mcp".to_string()]);
        let mut args = vec!["app".into(), "--data-dir=E:/y".into()];
        assert_eq!(take_data_dir_arg(&mut args).as_deref(), Some("E:/y"));
    }

    #[test]
    fn expands_environment_variables() {
        unsafe { std::env::set_var("NOVA_TEST_VAR", "C:\\Profiles") };
        assert_eq!(expand("%NOVA_TEST_VAR%\\Alt"), PathBuf::from("C:\\Profiles\\Alt"));
        assert!(expand("%NOPE_NOT_SET%\\x").to_string_lossy().contains("%NOPE_NOT_SET%"));
    }
}

#[cfg(test)]
mod wipe_tests {
    use super::*;
    #[test]
    fn wiping_a_shared_data_directory_preserves_unrelated_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("logs")).unwrap();
        for name in ["vault.json", "logs/session-123.log", "logs/keep.txt", "unrelated.json"] {
            std::fs::write(root.join(name), b"sample").unwrap();
        }
        wipe(root).unwrap();
        assert!(!root.join("vault.json").exists());
        assert!(!root.join("logs/session-123.log").exists());
        assert!(root.join("unrelated.json").exists());
        assert!(root.join("logs/keep.txt").exists());
    }
}
