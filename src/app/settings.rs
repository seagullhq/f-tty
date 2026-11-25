//! Persistence of user preferences across launches.

use crate::app::TabPosition;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SavedSettings {
    pub tabs_position: TabPosition,
    /// Program of the shell selected for new tabs (empty = auto-detect).
    pub shell: String,
    pub font: String,
    /// Side tab list width in pixels.
    pub strip: f32,
}

impl Default for SavedSettings {
    fn default() -> Self {
        Self {
            tabs_position: TabPosition::Top,
            shell: String::new(),
            font: crate::FONT.to_string(),
            strip: crate::ui::theme::STRIP_WIDTH.into(),
        }
    }
}

/// Config file: `%APPDATA%/ftty/settings.txt` on Windows,
/// `$XDG_CONFIG_HOME/ftty/settings.txt` (or `~/.config`) elsewhere.
fn path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("settings.txt"))
}

/// `…/ftty`, under the platform's per-user config folder.
pub(crate) fn config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| {
        std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("AppData\\Roaming"))
    });

    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));

    base.map(|dir| dir.join("ftty"))
}

pub(crate) fn load() -> SavedSettings {
    let Some(path) = path() else {
        return SavedSettings::default();
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return SavedSettings::default();
    };
    decode(&contents)
}

pub(crate) fn save(settings: &SavedSettings) {
    let Some(path) = path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, encode(settings));
}

fn encode(settings: &SavedSettings) -> String {
    format!(
        "position={}\nshell={}\nfont={}\nstrip={}\n",
        settings.tabs_position.label(),
        one_line(&settings.shell),
        one_line(&settings.font),
        settings.strip,
    )
}

fn decode(contents: &str) -> SavedSettings {
    let mut settings = SavedSettings::default();
    for line in contents.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim_end_matches('\r');
        match key {
            "position" => {
                if let Some(position) = TabPosition::parse(value) {
                    settings.tabs_position = position;
                }
            }
            "shell" => settings.shell = value.to_string(),
            "font" if !value.trim().is_empty() => settings.font = value.to_string(),
            "strip" => {
                if let Ok(width) = value.parse::<f32>() {
                    settings.strip = width;
                }
            }
            _ => {}
        }
    }
    settings
}

/// Values live on a single line, so a stray newline cannot corrupt the file.
fn one_line(value: &str) -> String {
    value.chars().filter(|c| *c != '\n' && *c != '\r').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn settings_round_trip() {
        let saved = SavedSettings {
            tabs_position: TabPosition::Left,
            shell: "C:\\Program Files\\PowerShell\\7\\pwsh.exe".into(),
            font: "Cascadia Mono".into(),
            strip: 96.,
        };
        assert_eq!(decode(&encode(&saved)), saved);
    }

    #[::core::prelude::v1::test]
    fn decode_ignores_unknown_and_invalid_values() {
        let decoded = decode("position=Nowhere\nshell=/bin/zsh\nfont=\nunknown=x\n");
        assert_eq!(decoded.tabs_position, TabPosition::Top);
        assert_eq!(decoded.shell, "/bin/zsh");
        assert_eq!(decoded.font, SavedSettings::default().font);
    }
}
