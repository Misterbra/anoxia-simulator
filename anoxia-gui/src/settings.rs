//! User preferences persisted between sessions (language, theme, tutorial).
//!
//! Stored as JSON in the user's configuration directory:
//! `%APPDATA%\AnoxiaSimulator\settings.json` on Windows,
//! `$XDG_CONFIG_HOME/anoxia-simulator/settings.json` (or `~/.config/…`) elsewhere.

use anoxia_core::Language;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub lang: Language,
    pub dark: bool,
    /// Show the "getting started" tutorial when the application starts
    pub show_tutorial_at_start: bool,
    pub open_report_after_export: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { lang: Language::French, dark: false, show_tutorial_at_start: true, open_report_after_export: true }
    }
}

impl Settings {
    fn path() -> Option<PathBuf> {
        let dir = if cfg!(target_os = "windows") {
            std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("AnoxiaSimulator"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
                .map(|d| d.join("anoxia-simulator"))
        }?;
        Some(dir.join("settings.json"))
    }

    /// Load the saved preferences, or the defaults.
    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Save the preferences (errors are ignored: preferences are a convenience).
    pub fn save(&self) {
        if let Some(p) = Self::path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(p, json);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let s = Settings {
            lang: Language::English,
            dark: true,
            show_tutorial_at_start: false,
            open_report_after_export: false,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
        // Missing fields take defaults (forward compatibility)
        let partial: Settings = serde_json::from_str(r#"{"dark":true}"#).unwrap();
        assert!(partial.dark && partial.show_tutorial_at_start && partial.lang == Language::French);
    }
}
