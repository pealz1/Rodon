//! Locates browsers for interactive Roblox account approval.

use std::path::PathBuf;

/// An installed browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Browser {
    pub label: String,
    pub exe: PathBuf,
}

fn program_dirs() -> Vec<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"].iter().filter_map(|v| std::env::var_os(v).map(PathBuf::from)).collect()
}

/// Candidate `(label, relative path under a program dir)` for each supported browser.
const CANDIDATES: [(&str, &str); 7] = [
    ("Mozilla Firefox", r"Mozilla Firefox\firefox.exe"),
    ("Google Chrome", r"Google\Chrome\Application\chrome.exe"),
    ("Microsoft Edge", r"Microsoft\Edge\Application\msedge.exe"),
    ("Brave", r"BraveSoftware\Brave-Browser\Application\brave.exe"),
    ("Vivaldi", r"Vivaldi\Application\vivaldi.exe"),
    ("Opera", r"Opera\opera.exe"),
    ("Opera GX", r"Opera GX\opera.exe"),
];

/// All browsers found on this machine, portable Chromium last.
pub fn available() -> Vec<Browser> {
    let mut found = Vec::new();
    let dirs = program_dirs();
    for (label, relative) in CANDIDATES {
        for dir in &dirs {
            let exe = dir.join(relative);
            if exe.exists() && !found.iter().any(|b: &Browser| b.label == label) {
                found.push(Browser { label: label.to_owned(), exe });
            }
        }
    }
    if let Some(exe) = super::chromium::chrome_exe() {
        found.push(Browser { label: "Portable Chromium".into(), exe });
    }
    found
}

/// The preferred browser: the one whose label matches `preference`, else the first found.
pub fn resolve(preference: &str) -> Option<Browser> {
    let browsers = available();
    browsers
        .iter()
        .find(|b| b.label.eq_ignore_ascii_case(preference) || b.label.to_lowercase().contains(&preference.to_lowercase()))
        .cloned()
        .or_else(|| browsers.into_iter().next())
}
