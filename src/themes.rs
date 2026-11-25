//! Portable Evanovar RAM themes and their locally owned background files.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::LazyLock,
};

pub type Colors = BTreeMap<String, String>;
pub static PRESETS: LazyLock<Vec<Theme>> =
    LazyLock::new(|| serde_json::from_str(include_str!("../assets/themes.json")).expect("built-in themes"));

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Background {
    pub enabled: bool,
    pub path: String,
    pub blur: u8,
    pub dim: u8,
    pub tint: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub format: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub name: String,
    pub colors: Colors,
    pub starting_colors: Colors,
    pub customized: Vec<String>,
    pub starting_customized: Vec<String>,
    pub background: Background,
}
const LIMIT: u64 = 250 * 1024 * 1024;
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid or unsupported RAM theme")
}
fn color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}
const DEPENDENCIES: &[(&str, &[&str])] = &[
    ("HOVER", &["INPUT", "TEXT"]),
    ("PRESSED", &["INPUT", "BG"]),
    ("RAISED", &["INPUT", "TEXT"]),
    ("ACCENT_HOVER", &["ACCENT"]),
    ("ACCENT_PRESSED", &["ACCENT"]),
    ("ACCENT_BRIGHT", &["ACCENT"]),
    ("ACCENT_SOFT", &["ACCENT", "BG"]),
    ("ACCENT_DEEP", &["ACCENT", "BG"]),
    ("ACCENT_TEXT", &["ACCENT", "BG"]),
    ("ON_ACCENT", &["ACCENT"]),
];

fn channels(hex: &str) -> [f64; 3] {
    [1, 3, 5].map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap() as f64)
}
fn blend(first: &str, second: &str, amount: f64) -> String {
    let (first, second) = (channels(first), channels(second));
    let [r, g, b] = [0, 1, 2].map(|i| (first[i] + (second[i] - first[i]) * amount).round_ties_even() as u8);
    format!("#{r:02X}{g:02X}{b:02X}")
}
fn contrast(first: &str, second: &str) -> f64 {
    fn luminance(hex: &str) -> f64 {
        let [r, g, b] = channels(hex).map(|c| {
            let c = c / 255.0;
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        });
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }
    let (a, b) = (luminance(first), luminance(second));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn derived_color(role: &str, palette: &Colors) -> String {
    let (accent, bg, input, text) = (&palette["ACCENT"], &palette["BG"], &palette["INPUT"], &palette["TEXT"]);
    match role {
        "HOVER" => blend(input, text, 0.14),
        "PRESSED" => blend(input, bg, 0.55),
        "RAISED" => blend(input, text, 0.07),
        "ACCENT_HOVER" => blend(accent, "#FFFFFF", 0.15),
        "ACCENT_PRESSED" => blend(accent, "#000000", 0.15),
        "ACCENT_BRIGHT" => blend(accent, "#FFFFFF", 0.10),
        "ACCENT_SOFT" => blend(bg, accent, 0.55),
        "ACCENT_DEEP" => blend(bg, accent, 0.14),
        "ACCENT_TEXT" => {
            let mut amount = 0.45;
            loop {
                let candidate = blend(accent, "#FFFFFF", amount);
                if contrast(&candidate, bg) >= 4.5 || amount >= 1.0 {
                    break candidate;
                }
                amount = (amount + 0.05).min(1.0);
            }
        }
        "ON_ACCENT" => if contrast(accent, "#FFFFFF") >= contrast(accent, "#101010") { "#FFFFFF" } else { "#101010" }.into(),
        _ => unreachable!("unknown derived role"),
    }
}
impl Theme {
    pub fn edit_color(&mut self, role: &str, value: Option<&str>) {
        // The UI can also read themes from hand-edited settings.
        if self.validate().is_err() || !self.colors.contains_key(role) {
            return;
        }
        let chosen = match value {
            Some(hex) if color(hex) => hex.to_ascii_uppercase(),
            Some(_) => return,
            None => self.starting_colors[role].clone(),
        };
        self.colors.insert(role.into(), chosen);
        if DEPENDENCIES.iter().any(|(name, _)| *name == role) {
            self.customized.retain(|r| r != role);
            if value.is_some() || self.starting_customized.iter().any(|r| r == role) {
                self.customized.push(role.into());
            }
        } else {
            for &(derived, deps) in DEPENDENCIES {
                if deps.contains(&role) && !self.customized.iter().any(|r| r == derived) {
                    let reset = deps.iter().all(|r| self.colors[*r] == self.starting_colors[*r]);
                    let next = if reset { self.starting_colors[derived].clone() } else { derived_color(derived, &self.colors) };
                    self.colors.insert(derived.into(), next);
                }
            }
        }
    }

    pub fn validate(&self) -> io::Result<()> {
        if self.format != "ram-theme"
            || self.version != 1
            || self.name.trim().is_empty()
            || self.name.chars().count() > 80
            || self.background.blur > 100
            || self.background.dim > 100
            || [&self.customized, &self.starting_customized]
                .iter()
                .any(|roles| roles.iter().any(|r| !DEPENDENCIES.iter().any(|(name, _)| name == r)))
            || !color(&self.background.tint)
            || [&self.colors, &self.starting_colors].iter().any(|p| p.keys().ne(PRESETS[0].colors.keys()) || !p.values().all(|s| color(s)))
        {
            return Err(invalid());
        }
        Ok(())
    }
    fn assign_local_id(&mut self) {
        let n = rand::random::<u128>();
        self.id = format!(
            "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
            n >> 96,
            (n >> 80) & 65535,
            (n >> 64) & 4095,
            (n >> 48) & 4095,
            n & 0xffffffffffff
        );
    }
    pub fn fork(&mut self, name: &str) {
        self.assign_local_id();
        self.name = name.trim().to_owned();
        self.starting_colors = self.colors.clone();
        self.starting_customized = self.customized.clone();
    }
}
fn ext(path: &Path) -> io::Result<String> {
    let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
    if ["png", "jpg", "jpeg", "bmp", "webp", "gif", "mp4", "wmv", "avi", "mov", "m4v", "mkv", "webm"].contains(&ext.as_str()) {
        Ok(ext)
    } else {
        Err(invalid())
    }
}
fn read_limit(r: impl Read, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    r.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid());
    }
    Ok(bytes)
}
fn store_media(media: &[u8], extension: &str, dir: &Path) -> io::Result<PathBuf> {
    let name = format!("{:x}.{extension}", Sha256::digest(media));
    let path = dir.join("themes").join("media").join(name);
    if !path.exists() {
        crate::store::atomic::write_atomic(&path, media)?;
    }
    Ok(path)
}
pub fn copy_media(path: &Path, dir: &Path) -> io::Result<PathBuf> {
    let ext = ext(path)?;
    store_media(&read_limit(fs::File::open(path)?, LIMIT)?, &ext, dir)
}
pub fn export(theme: &Theme, path: &Path) -> io::Result<()> {
    theme.validate()?;
    let mut t = theme.clone();
    t.id.clear();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    if !t.background.path.is_empty() {
        let file = Path::new(&t.background.path);
        let bytes = read_limit(fs::File::open(file)?, LIMIT)?;
        let name = format!("background.{}", ext(file)?);
        zip.start_file(&name, options)?;
        zip.write_all(&bytes)?;
        t.background.path = name;
    } else if t.background.enabled {
        return Err(invalid());
    }
    zip.start_file("theme.json", options)?;
    zip.write_all(&serde_json::to_vec(&t)?)?;
    crate::store::atomic::write_atomic(path, &zip.finish()?.into_inner())
}
pub fn import(path: &Path, dir: &Path) -> io::Result<Theme> {
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > 260 * 1024 * 1024 {
        return Err(invalid());
    }
    let mut zip = zip::ZipArchive::new(file)?;
    if !(1..=2).contains(&zip.len()) {
        return Err(invalid());
    }
    let mut t: Theme = serde_json::from_slice(&read_limit(zip.by_name("theme.json")?, 256 * 1024)?)?;
    t.validate()?;
    if t.background.path.is_empty() {
        if t.background.enabled || zip.len() != 1 {
            return Err(invalid());
        }
    } else {
        let file = Path::new(&t.background.path);
        if file.file_name().and_then(|n| n.to_str()) != Some(t.background.path.as_str()) || zip.len() != 2 {
            return Err(invalid());
        }
        let ext = ext(file)?;
        let bytes = read_limit(zip.by_name(&t.background.path)?, LIMIT)?;
        t.background.path = store_media(&bytes, &ext, dir)?.display().to_string();
    }
    t.assign_local_id();
    Ok(t)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_a_portable_archive_from_the_upstream_python_exporter() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("import.ramtheme");
        // Produced by ThemeLibrary.export_theme in evanovar/RobloxAccountManager at 8b37709.
        fs::write(&path, include_bytes!("../tests/fixtures/evanovar.ramtheme")).unwrap();
        let theme = import(&path, dir.path()).unwrap();
        assert_eq!(theme.name, "Evanovar export fixture");
        assert_eq!(theme.colors["ACCENT_HOVER"], "#C1542C");
        assert_ne!(theme.starting_colors["ACCENT_HOVER"], theme.colors["ACCENT_HOVER"]);
        assert_eq!(theme.customized, ["ACCENT_HOVER"]);
        assert_eq!(theme.id.len(), 36);
        let image = image::open(&theme.background.path).unwrap();
        assert_eq!((image.width(), image.height()), (2, 1));
        export(&theme, &path).unwrap();
        let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        let manifest: serde_json::Value = serde_json::from_reader(zip.by_name("theme.json").unwrap()).unwrap();
        assert!(manifest.get("id").is_none());
        assert_eq!(manifest["background"]["path"], "background.png");
    }
    #[test]
    fn color_edits_preserve_overrides_and_reset_the_original_palette() {
        let mut t = PRESETS[0].clone();
        let start = t.colors.clone();
        t.edit_color("ACCENT_HOVER", Some("#00ee00"));
        t.edit_color("ACCENT", Some("#ffffff"));
        assert_eq!(t.colors["ON_ACCENT"], "#101010");
        assert_eq!(t.colors["ACCENT_HOVER"], "#00EE00");
        t.edit_color("ACCENT_HOVER", None);
        t.edit_color("ACCENT", None);
        assert_eq!(t.colors, start);
        assert!(t.customized.is_empty());
    }
    #[test]
    fn rejects_media_outside_archive() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("bad.ramtheme");
        let mut t = PRESETS[0].clone();
        t.background.path = "../outside.png".into();
        let mut z = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        z.start_file("theme.json", options).unwrap();
        z.write_all(&serde_json::to_vec(&t).unwrap()).unwrap();
        z.start_file("../outside.png", options).unwrap();
        z.write_all(b"x").unwrap();
        z.finish().unwrap();
        assert!(import(&path, root.path()).is_err());
        assert!(!root.path().join("themes").exists());
    }
    #[test]
    fn upstream_theme_round_trip_with_media() {
        let dir = tempfile::tempdir().unwrap();
        let mut t = PRESETS[3].clone();
        let image = dir.path().join("source.png");
        fs::write(&image, b"sample").unwrap();
        t.background.path = image.display().to_string();
        t.background.enabled = true;
        let path = dir.path().join("nord.ramtheme");
        export(&t, &path).unwrap();
        fs::remove_file(image).unwrap();
        let copy = import(&path, dir.path()).unwrap();
        assert_eq!(copy.colors, t.colors);
        assert_eq!(fs::read(copy.background.path).unwrap(), b"sample");
    }
}
