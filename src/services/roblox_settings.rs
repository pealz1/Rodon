//! Reads and edits `GlobalBasicSettings_13.xml`.
//!
//! * Basic presets (framerate cap, master volume, start quality) are applied before
//!   a client launches.
//! * The advanced editor lists every setting in the file and lets the user change a
//!   value. Values are substituted in place for whatever element type Roblox used
//!   (`<int>`, `<token>`, `<float>`, …); a key Roblox has not written yet is left
//!   alone rather than guessed at.

use crate::error::{AppError, AppResult};
use crate::store::settings::RobloxPresets;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

const FILE_NAME: &str = "GlobalBasicSettings_13.xml";

/// One editable Roblox setting read from the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RobloxSetting {
    pub name: String,
    pub kind: String,
    pub value: String,
}

/// The settings file to edit: the custom path if set, else the default location.
pub fn settings_path(presets: &RobloxPresets) -> Option<PathBuf> {
    let custom = presets.settings_path.trim();
    if !custom.is_empty() {
        return Some(PathBuf::from(custom));
    }
    crate::paths::local_appdata().map(|p| p.join("Roblox").join(FILE_NAME))
}

/// Parses every `<type name="...">value</type>` setting from the file.
pub fn list_all(path: &Path) -> AppResult<Vec<RobloxSetting>> {
    let xml = std::fs::read_to_string(path).map_err(|e| AppError::io("Reading Roblox settings", &e))?;
    let doc = roxmltree::Document::parse(&xml).map_err(|e| {
        AppError::new("ROBLOX_SETTINGS_INVALID", "Settings File Unreadable", "The Roblox settings file could not be parsed.")
            .with_detail(e.to_string())
    })?;
    const TYPES: [&str; 12] = [
        "bool",
        "int",
        "int64",
        "float",
        "double",
        "token",
        "string",
        "BinaryString",
        "SecurityCapabilities",
        "SharedString",
        "Vector2",
        "Vector3",
    ];
    let mut out = Vec::new();
    for node in doc.descendants() {
        let kind = node.tag_name().name();
        if !TYPES.contains(&kind) {
            continue;
        }
        if let Some(name) = node.attribute("name") {
            out.push(RobloxSetting {
                name: name.to_owned(),
                kind: kind.to_owned(),
                value: if kind.starts_with("Vector") {
                    node.children().filter(|n| n.is_element()).map(|n| n.text().unwrap_or("0")).collect::<Vec<_>>().join(", ")
                } else {
                    node.text().unwrap_or("").to_owned()
                },
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    Ok(out)
}

/// Validates a value against its element type (so a hand-typed value can't corrupt the file).
pub fn validate(kind: &str, value: &str) -> AppResult<()> {
    let ok = match kind {
        "bool" => matches!(value, "true" | "false"),
        "int" | "int64" | "token" => value.parse::<i64>().is_ok(),
        "float" | "double" => value.parse::<f64>().is_ok_and(f64::is_finite),
        "Vector2" | "Vector3" => {
            let parts: Vec<_> = value.split(',').map(str::trim).collect();
            parts.len() == if kind == "Vector2" { 2 } else { 3 } && parts.iter().all(|s| s.parse::<f64>().is_ok_and(f64::is_finite))
        }
        "SecurityCapabilities" => value.parse::<u64>().is_ok(),
        "string" | "BinaryString" | "SharedString" => value.chars().all(|ch| ch >= ' ' || matches!(ch, '\n' | '\r' | '\t')),
        _ => true,
    };
    if ok { Ok(()) } else { Err(AppError::invalid("ROBLOX_SETTING_VALUE_INVALID", format!("'{value}' is not a valid {kind} value."))) }
}

/// Saves a single advanced setting by name, keeping Roblox's element type.
pub fn save_named(path: &Path, name: &str, kind: &str, value: &str) -> AppResult<()> {
    validate(kind, value)?;
    let xml = std::fs::read_to_string(path).map_err(|e| AppError::io("Reading Roblox settings", &e))?;
    if !has_key(&xml, name) {
        return Err(AppError::not_found("Roblox Setting", name));
    }
    let updated = set_value(&xml, name, value);
    if updated == xml {
        return Ok(());
    }
    write_possibly_readonly(path, &updated)
}

/// Replaces the inner value of `<type name="KEY">value</type>` for the given key,
/// keeping Roblox's own element type. Returns the edited XML (unchanged if absent).
fn set_value(xml: &str, key: &str, value: &str) -> String {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return xml.into() };
    let Some(node) = doc.descendants().find(|n| n.attribute("name") == Some(key)) else { return xml.into() };
    let escape = |v: &str| v.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let fragment = if node.tag_name().name().starts_with("Vector") {
        let values = value.split(',').map(str::trim);
        ["X", "Y", "Z"].into_iter().zip(values).map(|(axis, v)| format!("<{axis}>{}</{axis}>", escape(v))).collect()
    } else {
        escape(value)
    };
    let range = node.range();
    let original = &xml[range.clone()];
    let Some(open) = original.find('>') else { return xml.into() };
    let content = if original[..open].ends_with('/') {
        format!("{}>{}</{}>", &original[..open - 1], fragment, node.tag_name().name())
    } else if let Some(close) = original.rfind("</") {
        format!("{}{}{}", &original[..=open], fragment, &original[close..])
    } else {
        return xml.into();
    };
    let mut out = xml.to_owned();
    out.replace_range(range, &content);
    out
}

fn has_key(xml: &str, key: &str) -> bool {
    roxmltree::Document::parse(xml).is_ok_and(|doc| doc.descendants().any(|n| n.attribute("name") == Some(key)))
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Profile {
    source: PathBuf,
    values: BTreeMap<String, (String, String)>,
}
static OWNED_LOCK: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(Mutex::default);
static WRITER: Mutex<()> = Mutex::new(());
fn profile_file() -> PathBuf {
    crate::paths::data_file("RobloxSettings.json")
}
fn profile(path: &Path) -> AppResult<Profile> {
    match std::fs::read(profile_file()) {
        Ok(bytes) => {
            let p: Profile = serde_json::from_slice(&bytes).map_err(|e| AppError::unexpected("Roblox settings profile", e))?;
            if p.source == path {
                return Ok(p);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(AppError::io("Reading settings profile", &e)),
    }
    Ok(Profile { source: path.into(), ..Default::default() })
}
pub fn stage(path: &Path, key: &str, kind: &str, value: &str) -> AppResult<()> {
    validate(kind, value)?;
    let mut p = profile(path)?;
    p.values.insert(key.into(), (kind.into(), value.into()));
    crate::store::atomic::write_json(&profile_file(), &p).map_err(|e| AppError::io("Saving local profile", &e))
}
pub fn staged(path: &Path) -> AppResult<Vec<RobloxSetting>> {
    let mut rows = list_all(path)?;
    let p = profile(path)?;
    for r in &mut rows {
        if let Some((kind, value)) = p.values.get(&r.name)
            && kind == &r.kind
        {
            r.value = value.clone();
        }
    }
    Ok(rows)
}
pub fn discard_profile(path: &Path) -> AppResult<()> {
    let p = Profile { source: path.into(), ..Default::default() };
    crate::store::atomic::write_json(&profile_file(), &p).map_err(|e| AppError::io("Reloading settings profile", &e))
}
#[allow(clippy::permissions_set_readonly_false)]
pub fn unlock_profile() {
    let mut owned = OWNED_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(path) = owned.as_ref()
        && let Ok(mut perms) = std::fs::metadata(path).map(|m| m.permissions())
    {
        perms.set_readonly(false);
        if std::fs::set_permissions(path, perms).is_err() {
            return;
        }
    }
    *owned = None;
}
pub fn apply_profile(path: &Path, lock: bool) -> AppResult<()> {
    let _writer = WRITER.lock().unwrap_or_else(|e| e.into_inner());
    if !crate::win::process::list_roblox().is_empty() {
        return Err(AppError::invalid("ROBLOX_RUNNING", "Close Roblox before applying the settings profile."));
    }
    let p = profile(path)?;
    let original = std::fs::read_to_string(path).map_err(|e| AppError::io("Reading Roblox settings", &e))?;
    let mut updated = original.clone();
    for (key, (kind, value)) in p.values {
        validate(&kind, &value)?;
        updated = set_value(&updated, &key, &value);
    }
    if updated != original {
        write_possibly_readonly(path, &updated)?;
    }
    if lock {
        let mut perms = std::fs::metadata(path).map_err(|e| AppError::io("Checking settings permissions", &e))?.permissions();
        let different = OWNED_LOCK.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|p| p != path);
        if different {
            unlock_profile();
        }
        if !perms.readonly() {
            perms.set_readonly(true);
            std::fs::set_permissions(path, perms).map_err(|e| AppError::io("Locking settings", &e))?;
            *OWNED_LOCK.lock().unwrap_or_else(|e| e.into_inner()) = Some(path.into());
        }
    }
    Ok(())
}
/// Applies the enabled basic presets. Best-effort: returns Ok and logs when the
/// file is missing or a key isn't present, so it never blocks a launch.
pub fn apply_presets(presets: &RobloxPresets) -> AppResult<()> {
    if presets.advanced_auto_apply
        && let Some(path) = settings_path(presets)
        && path.exists()
        && crate::win::process::list_roblox().is_empty()
    {
        apply_profile(&path, true)?;
    }
    if !presets.framerate_cap_enabled && !presets.master_volume_enabled && !presets.graphics_quality_enabled {
        return Ok(());
    }
    let Some(path) = settings_path(presets) else {
        return Ok(());
    };
    if !path.exists() {
        crate::log_warn!("Roblox settings file not found; presets not applied ({})", path.display());
        return Ok(());
    }
    let original = std::fs::read_to_string(&path).map_err(|e| AppError::io("Reading Roblox settings", &e))?;
    let mut xml = original.clone();
    if presets.framerate_cap_enabled {
        apply_one(&mut xml, "FramerateCap", &presets.framerate_cap.to_string());
    }
    if presets.master_volume_enabled {
        // Roblox stores master volume as 0.0–1.0.
        apply_one(&mut xml, "MasterVolume", &format!("{:.4}", presets.master_volume.clamp(0.0, 1.0)));
    }
    if presets.graphics_quality_enabled {
        apply_one(&mut xml, "SavedQualityLevel", &presets.graphics_quality.min(10).to_string());
    }
    if xml == original {
        return Ok(());
    }
    write_possibly_readonly(&path, &xml)
}

fn apply_one(xml: &mut String, key: &str, value: &str) {
    if has_key(xml, key) {
        *xml = set_value(xml, key, value);
    } else {
        crate::log_warn!("Roblox settings has no '{key}' entry yet; skipped. Launch Roblox once to create it.");
    }
}

/// Writes the file, temporarily clearing the read-only flag if Roblox set it.
#[allow(clippy::permissions_set_readonly_false)] // we restore the flag afterward
fn write_possibly_readonly(path: &std::path::Path, contents: &str) -> AppResult<()> {
    if let Ok(bytes) = std::fs::read(path) {
        let mut backup = path.as_os_str().to_owned();
        backup.push(".ram.bak");
        crate::store::atomic::write_atomic(Path::new(&backup), &bytes).map_err(|e| AppError::io("Backing up Roblox settings", &e))?;
    }
    let was_readonly = std::fs::metadata(path).map(|m| m.permissions().readonly()).unwrap_or(false);
    if was_readonly {
        let mut perms = std::fs::metadata(path).map_err(|e| AppError::io("Reading Roblox settings", &e))?.permissions();
        perms.set_readonly(false);
        let _ = std::fs::set_permissions(path, perms);
    }
    let result = crate::store::atomic::write_atomic(path, contents.as_bytes()).map_err(|e| AppError::io("Writing Roblox settings", &e));
    if was_readonly && let Ok(mut perms) = std::fs::metadata(path).map(|m| m.permissions()) {
        perms.set_readonly(true);
        let _ = std::fs::set_permissions(path, perms);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<roblox><Item class="UserGameSettings"><Properties>
<int name="FramerateCap">60</int>
<float name="MasterVolume">1</float>
<token name="SavedQualityLevel">0</token>
</Properties></Item></roblox>"#;

    #[test]
    fn sets_known_values_keeping_element_type() {
        let xml = set_value(SAMPLE, "FramerateCap", "240");
        assert!(xml.contains(r#"<int name="FramerateCap">240</int>"#));
        let xml = set_value(&xml, "MasterVolume", "0.5000");
        assert!(xml.contains(r#"<float name="MasterVolume">0.5000</float>"#));
        let xml = set_value(&xml, "SavedQualityLevel", "5");
        assert!(xml.contains(r#"<token name="SavedQualityLevel">5</token>"#));
    }

    #[test]
    fn missing_key_leaves_xml_unchanged() {
        assert_eq!(set_value(SAMPLE, "NotThere", "9"), SAMPLE);
        assert!(!has_key(SAMPLE, "NotThere"));
        assert!(has_key(SAMPLE, "FramerateCap"));
    }

    #[test]
    fn apply_presets_is_noop_when_all_disabled() {
        assert!(apply_presets(&RobloxPresets::default()).is_ok());
    }

    #[test]
    fn list_all_parses_and_sorts_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("GlobalBasicSettings_13.xml");
        std::fs::write(&path, SAMPLE).unwrap();
        let settings = list_all(&path).unwrap();
        let names: Vec<&str> = settings.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["FramerateCap", "MasterVolume", "SavedQualityLevel"]);
        assert_eq!(settings[0].kind, "int");
        assert_eq!(settings[0].value, "60");
    }

    #[test]
    fn save_named_updates_only_the_named_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("GlobalBasicSettings_13.xml");
        std::fs::write(&path, SAMPLE).unwrap();
        save_named(&path, "MasterVolume", "float", "0.25").unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains(r#"<float name="MasterVolume">0.25</float>"#));
        assert!(after.contains(r#"<int name="FramerateCap">60</int>"#));
        assert_eq!(save_named(&path, "Nope", "int", "1").unwrap_err().code, "NOT_FOUND");
    }

    #[test]
    fn validate_checks_types() {
        assert!(validate("int", "60").is_ok());
        assert!(validate("int", "6x").is_err());
        assert!(validate("bool", "true").is_ok());
        assert!(validate("bool", "yes").is_err());
        assert!(validate("float", "0.5").is_ok());
    }
}

#[cfg(test)]
mod editor_tests {
    use super::*;
    #[test]
    fn handles_xml_entities_vectors_and_empty_elements() {
        let before = r#"<root><string name="Text"/><Vector2 name="Size"><X>1</X><Y>2</Y></Vector2></root>"#;
        let escaped = set_value(before, "Text", "A & B < C");
        let result = set_value(&escaped, "Size", "4.5, 7");
        let doc = roxmltree::Document::parse(&result).unwrap();
        assert_eq!(doc.descendants().find(|n| n.attribute("name") == Some("Text")).unwrap().text(), Some("A & B < C"));
        assert!(result.contains("<X>4.5</X><Y>7</Y>"));
    }
}
