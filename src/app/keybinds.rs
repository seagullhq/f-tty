//! User-editable keyboard shortcuts.
//!
//! One entry per chord: actions with two defaults (paste, zoom) get two rows.
//! Arrow chords match any arrow key, so focus and resize keep their four
//! directions with a single binding.

use super::tree::Dir;
use gpui::{Keystroke, Modifiers};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Chord {
    modifiers: Modifiers,
    /// Normalized key name; `"arrow"` matches any arrow key.
    key: String,
}

impl Chord {
    fn new(modifiers: Modifiers, key: impl Into<String>) -> Self {
        Self {
            modifiers,
            key: key.into(),
        }
    }

    /// A keystroke is bindable only with a modifier, so plain typing still
    /// reaches the shell. Escape alone is reserved for cancelling a capture.
    pub(crate) fn from_keystroke(ks: &Keystroke) -> Option<Self> {
        if !ks.modifiers.modified() || is_modifier_key(&ks.key) {
            return None;
        }
        Some(Self::new(ks.modifiers, normalize_key(&ks.key)))
    }

    pub(crate) fn matches(&self, ks: &Keystroke) -> bool {
        self.modifiers == ks.modifiers
            && (self.key == "arrow" && Dir::parse(&ks.key).is_some()
                || self.key == normalize_key(&ks.key))
    }

    /// Canonical lowercase form used in the settings file.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let mut modifiers = Modifiers::none();
        let mut key = None;
        for part in value.split('+') {
            match part.trim().to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.control = true,
                "alt" | "option" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                "win" | "super" | "cmd" | "meta" | "platform" => modifiers.platform = true,
                "fn" | "function" => modifiers.function = true,
                "" => return None,
                other if key.is_none() => key = Some(normalize_key(other)),
                _ => return None,
            }
        }
        Some(Self::new(modifiers, key?))
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.control {
            f.write_str("Ctrl+")?;
        }
        if self.modifiers.alt {
            f.write_str("Alt+")?;
        }
        if self.modifiers.shift {
            f.write_str("Shift+")?;
        }
        if self.modifiers.platform {
            f.write_str(if cfg!(target_os = "macos") {
                "Cmd+"
            } else {
                "Win+"
            })?;
        }
        if self.modifiers.function {
            f.write_str("Fn+")?;
        }
        let key = match self.key.as_str() {
            "arrow" => "Arrow".to_string(),
            "pageup" => "PageUp".to_string(),
            "pagedown" => "PageDown".to_string(),
            "space" => "Space".to_string(),
            "tab" => "Tab".to_string(),
            "enter" => "Enter".to_string(),
            "insert" => "Insert".to_string(),
            "escape" => "Escape".to_string(),
            key if key.len() == 1 => key.to_uppercase(),
            key => key.to_string(),
        };
        f.write_str(&key)
    }
}

/// Physical key names as GPUI reports them, folded to stable storage names.
fn normalize_key(key: &str) -> String {
    match key.trim().to_ascii_lowercase().as_str() {
        "up" | "arrowup" | "down" | "arrowdown" | "left" | "arrowleft" | "right" | "arrowright" => {
            "arrow".to_string()
        }
        "{" => "[".to_string(),
        "}" => "]".to_string(),
        "+" => "=".to_string(),
        "esc" => "escape".to_string(),
        " " => "space".to_string(),
        other => other.to_string(),
    }
}

fn is_modifier_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "control"
            | "ctrl"
            | "shift"
            | "alt"
            | "option"
            | "platform"
            | "super"
            | "cmd"
            | "meta"
            | "function"
            | "fn"
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    NewTab,
    ClosePane,
    SplitRight,
    SplitDown,
    FocusPane,
    PrevPane,
    NextPane,
    Zoom,
    ResizePane,
    Equalize,
    SwapPrev,
    SwapNext,
    Broadcast,
    NextTab,
    PrevTab,
    Copy,
    Paste,
    Find,
    NextPrompt,
    PrevPrompt,
    ScrollUp,
    ScrollDown,
}

impl Action {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::NewTab => "new_tab",
            Self::ClosePane => "close_pane",
            Self::SplitRight => "split_right",
            Self::SplitDown => "split_down",
            Self::FocusPane => "focus_pane",
            Self::PrevPane => "prev_pane",
            Self::NextPane => "next_pane",
            Self::Zoom => "zoom",
            Self::ResizePane => "resize_pane",
            Self::Equalize => "equalize",
            Self::SwapPrev => "swap_prev",
            Self::SwapNext => "swap_next",
            Self::Broadcast => "broadcast",
            Self::NextTab => "next_tab",
            Self::PrevTab => "prev_tab",
            Self::Copy => "copy",
            Self::Paste => "paste",
            Self::Find => "find",
            Self::NextPrompt => "next_prompt",
            Self::PrevPrompt => "prev_prompt",
            Self::ScrollUp => "scroll_up",
            Self::ScrollDown => "scroll_down",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Binding {
    pub(crate) action: Action,
    pub(crate) label: &'static str,
    pub(crate) chord: Chord,
}

/// Ordered bindings; the index is what the settings UI captures into.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KeyBinds {
    entries: Vec<Binding>,
}

impl Default for KeyBinds {
    fn default() -> Self {
        let bindings = [
            ("New tab", Action::NewTab, "ctrl+shift+t"),
            (
                "Close pane (last pane: tab)",
                Action::ClosePane,
                "ctrl+shift+w",
            ),
            ("Split right", Action::SplitRight, "ctrl+shift+d"),
            ("Split down", Action::SplitDown, "ctrl+shift+e"),
            ("Focus pane", Action::FocusPane, "ctrl+shift+arrow"),
            ("Previous pane", Action::PrevPane, "ctrl+shift+["),
            ("Next pane", Action::NextPane, "ctrl+shift+]"),
            ("Zoom pane", Action::Zoom, "ctrl+shift+z"),
            ("Zoom pane", Action::Zoom, "ctrl+shift+enter"),
            ("Resize panes", Action::ResizePane, "ctrl+alt+shift+arrow"),
            ("Equalize panes", Action::Equalize, "ctrl+alt+shift+="),
            ("Swap with previous", Action::SwapPrev, "ctrl+alt+shift+["),
            ("Swap with next", Action::SwapNext, "ctrl+alt+shift+]"),
            ("Broadcast typing", Action::Broadcast, "ctrl+shift+b"),
            ("Next tab", Action::NextTab, "ctrl+tab"),
            ("Previous tab", Action::PrevTab, "ctrl+shift+tab"),
            ("Copy selection", Action::Copy, "ctrl+shift+c"),
            ("Paste", Action::Paste, "ctrl+shift+v"),
            ("Paste", Action::Paste, "shift+insert"),
            ("Find in scrollback", Action::Find, "ctrl+shift+f"),
            ("Next prompt", Action::NextPrompt, "ctrl+shift+pageup"),
            ("Previous prompt", Action::PrevPrompt, "ctrl+shift+pagedown"),
            ("Scroll back", Action::ScrollUp, "shift+pageup"),
            ("Scroll forward", Action::ScrollDown, "shift+pagedown"),
        ];
        Self {
            entries: bindings
                .into_iter()
                .map(|(label, action, chord)| Binding {
                    action,
                    label,
                    chord: Chord::parse(chord).expect("default chord"),
                })
                .collect(),
        }
    }
}

impl KeyBinds {
    pub(crate) fn entries(&self) -> &[Binding] {
        &self.entries
    }

    pub(crate) fn match_action(&self, ks: &Keystroke) -> Option<Action> {
        self.entries
            .iter()
            .find(|entry| entry.chord.matches(ks))
            .map(|entry| entry.action)
    }

    /// Assigns a chord, exchanging it with the action that already owns it so
    /// two actions never share one key.
    pub(crate) fn rebind(&mut self, index: usize, chord: Chord) -> bool {
        if self
            .entries
            .get(index)
            .is_none_or(|entry| entry.chord == chord)
        {
            return false;
        }
        if let Some(other) = self.entries.iter().position(|other| other.chord == chord) {
            let taken = self.entries[other].chord.clone();
            self.entries[other].chord = self.entries[index].chord.clone();
            self.entries[index].chord = taken;
        } else {
            self.entries[index].chord = chord;
        }
        true
    }

    /// Applies one `bind=<name>:<chord>` line from the settings file. Unlike
    /// interactive rebinding this assigns directly: a saved file is already
    /// conflict-free, and swapping during load would undo later lines.
    pub(crate) fn set_named(&mut self, name: &str, chord: Chord) -> bool {
        let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.action.name() == name)
        else {
            return false;
        };
        if self
            .entries
            .iter()
            .enumerate()
            .any(|(i, other)| i != index && other.chord == chord)
        {
            return false;
        }
        self.entries[index].chord = chord;
        true
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(key: &str, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: None,
        }
    }

    #[::core::prelude::v1::test]
    fn default_chords_match_their_keys() {
        let binds = KeyBinds::default();
        let mut ctrl_shift = Modifiers::none();
        ctrl_shift.control = true;
        ctrl_shift.shift = true;
        assert_eq!(
            binds.match_action(&stroke("t", ctrl_shift)),
            Some(Action::NewTab)
        );
        assert_eq!(
            binds.match_action(&stroke("v", ctrl_shift)),
            Some(Action::Paste)
        );
    }

    #[::core::prelude::v1::test]
    fn one_arrow_chord_covers_all_directions() {
        let binds = KeyBinds::default();
        let mut ctrl_shift = Modifiers::none();
        ctrl_shift.control = true;
        ctrl_shift.shift = true;
        for key in ["left", "right", "up", "down"] {
            assert_eq!(
                binds.match_action(&stroke(key, ctrl_shift)),
                Some(Action::FocusPane),
                "key {key}"
            );
        }
    }

    #[::core::prelude::v1::test]
    fn capture_requires_a_modifier_and_groups_arrows() {
        assert!(Chord::from_keystroke(&stroke("t", Modifiers::none())).is_none());
        let mut modifiers = Modifiers::none();
        modifiers.control = true;
        modifiers.shift = true;
        let chord = Chord::from_keystroke(&stroke("left", modifiers)).unwrap();
        assert_eq!(chord.to_string(), "Ctrl+Shift+Arrow");
    }

    #[::core::prelude::v1::test]
    fn chord_text_round_trips() {
        for text in ["ctrl+shift+t", "ctrl+alt+shift+=", "shift+insert"] {
            let chord = Chord::parse(text).unwrap();
            assert_eq!(Chord::parse(&chord.to_string()).unwrap(), chord);
        }
        assert!(Chord::parse("ctrl+").is_none());
        assert!(Chord::parse("t").is_some());
    }

    #[::core::prelude::v1::test]
    fn rebinding_an_used_chord_swaps_the_two_actions() {
        let mut binds = KeyBinds::default();
        let new_tab = Chord::parse("ctrl+shift+t").unwrap();
        let copy = binds.entries[16].chord.clone();
        assert!(binds.rebind(16, new_tab.clone()));
        assert_eq!(binds.entries[16].chord, new_tab);
        assert_eq!(binds.entries[0].chord, copy);
        assert_eq!(binds.entries[16].action, Action::Copy);
        assert_eq!(binds.entries[0].action, Action::NewTab);
    }
}
