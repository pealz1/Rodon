//! Temporarily set aside Roblox's installer and restore the same version on exit.
use std::{fs, io, path::Path};
const NAME: &str = "RobloxPlayerInstaller.exe";
fn transfer(versions: &Path, quarantine: &Path, restore: bool) -> io::Result<()> {
    let (from, to) = if restore { (quarantine, versions) } else { (versions, quarantine) };
    if !from.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let label = entry.file_name();
        let Some(version) = label.to_str().and_then(|s| s.strip_prefix("version-")) else { continue };
        if version.is_empty() || !version.bytes().all(|c| c.is_ascii_alphanumeric()) || !entry.file_type()?.is_dir() {
            continue;
        }
        let src = entry.path().join(NAME);
        if !src.is_file() || src.symlink_metadata()?.file_type().is_symlink() {
            continue;
        }
        let folder = to.join(&label);
        if restore && !folder.is_dir() {
            continue;
        }
        if folder.exists() && folder.symlink_metadata()?.file_type().is_symlink() {
            continue;
        }
        fs::create_dir_all(&folder)?;
        let target = folder.join(NAME);
        if target.exists() {
            continue;
        }
        fs::rename(src, target)?;
    }
    Ok(())
}
pub fn apply(enable: bool) -> io::Result<()> {
    if let Some(base) = crate::paths::local_appdata() {
        transfer(&base.join("Roblox/Versions"), &base.join("Rodon/Quarantine"), !enable)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restore_preserves_a_new_installer() {
        let root = tempfile::tempdir().unwrap();
        let v = root.path().join("Versions");
        let q = root.path().join("Q");
        let path = v.join("version-abc").join(NAME);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"original").unwrap();
        transfer(&v, &q, false).unwrap();
        fs::write(&path, b"new").unwrap();
        transfer(&v, &q, true).unwrap();
        assert_eq!(fs::read(path).unwrap(), b"new");
        assert_eq!(fs::read(q.join("version-abc").join(NAME)).unwrap(), b"original");
    }
}
