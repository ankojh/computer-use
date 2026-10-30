//! Provider key names (Anthropic xdotool-style, OpenAI uppercase) → canonical key combos.
//!
//! The canonical string form (`"ctrl+shift+t"`, `"cmd+return"`, `"plus"`) is what the Swift layer
//! parses; see `Keys.swift`. Unknown names are errors returned to the model, never guesses.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::action::Mods;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamedKey {
    Return,
    Tab,
    Space,
    Backspace,
    ForwardDelete,
    Escape,
    Home,
    End,
    PageUp,
    PageDown,
    Left,
    Right,
    Up,
    Down,
    CapsLock,
    Help,
    KeypadEnter,
    F(u8),
}

impl NamedKey {
    fn canonical(self) -> String {
        match self {
            NamedKey::Return => "return".into(),
            NamedKey::Tab => "tab".into(),
            NamedKey::Space => "space".into(),
            NamedKey::Backspace => "backspace".into(),
            NamedKey::ForwardDelete => "forwarddelete".into(),
            NamedKey::Escape => "escape".into(),
            NamedKey::Home => "home".into(),
            NamedKey::End => "end".into(),
            NamedKey::PageUp => "pageup".into(),
            NamedKey::PageDown => "pagedown".into(),
            NamedKey::Left => "left".into(),
            NamedKey::Right => "right".into(),
            NamedKey::Up => "up".into(),
            NamedKey::Down => "down".into(),
            NamedKey::CapsLock => "capslock".into(),
            NamedKey::Help => "help".into(),
            NamedKey::KeypadEnter => "keypadenter".into(),
            NamedKey::F(n) => format!("f{n}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Named(NamedKey),
    /// A single printable character; letters are stored lowercase.
    Char(char),
}

/// Modifiers plus at most one main key. `key: None` is a modifier-only combo (e.g. "shift").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub mods: Mods,
    pub key: Option<Key>,
}

#[derive(Debug, Error, PartialEq)]
pub enum KeyError {
    #[error("unknown key name '{0}'")]
    Unknown(String),
    #[error("empty key combo")]
    Empty,
    #[error("key combo '{0}' has more than one non-modifier key")]
    MultipleKeys(String),
}

impl KeyCombo {
    pub fn key(key: Key) -> Self {
        KeyCombo {
            mods: Mods::NONE,
            key: Some(key),
        }
    }

    pub fn with(mods: Mods, key: Key) -> Self {
        KeyCombo {
            mods,
            key: Some(key),
        }
    }

    pub fn is_named(&self, k: NamedKey) -> bool {
        self.key == Some(Key::Named(k))
    }

    /// Canonical string understood by the Swift layer.
    pub fn canonical(&self) -> String {
        let mut parts: Vec<String> = self.mods.names().into_iter().map(String::from).collect();
        match self.key {
            Some(Key::Named(n)) => parts.push(n.canonical()),
            Some(Key::Char('+')) => parts.push("plus".into()),
            Some(Key::Char(c)) => parts.push(c.to_string()),
            None => {}
        }
        parts.join("+")
    }

    /// Parses a `+`-joined combo in either provider's vocabulary ("ctrl+shift+t", "super+c",
    /// "Return", "ctrl+plus", "ctrl++").
    pub fn parse(s: &str) -> Result<Self, KeyError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(KeyError::Empty);
        }
        let mut tokens: Vec<&str> = s.split('+').collect();
        // "ctrl++" / "+" → the last token(s) are empty because the key itself is '+'.
        if s.ends_with('+') {
            while tokens.last() == Some(&"") {
                tokens.pop();
            }
            tokens.push("+");
        }
        if tokens.iter().any(|t| t.trim().is_empty()) {
            return Err(KeyError::Unknown(s.to_string()));
        }
        Self::from_tokens(&tokens, s)
    }

    /// Builds a chord from separate key names (OpenAI `keypress.keys`, e.g. `["CTRL", "A"]`).
    pub fn from_keys<S: AsRef<str>>(keys: &[S]) -> Result<Self, KeyError> {
        if keys.is_empty() {
            return Err(KeyError::Empty);
        }
        let tokens: Vec<&str> = keys.iter().map(|k| k.as_ref()).collect();
        Self::from_tokens(&tokens, &tokens.join("+"))
    }

    fn from_tokens(tokens: &[&str], whole: &str) -> Result<Self, KeyError> {
        let mut combo = KeyCombo {
            mods: Mods::NONE,
            key: None,
        };
        for t in tokens {
            let t = t.trim();
            if let Some(m) = modifier_from_name(t) {
                combo.mods |= m;
                continue;
            }
            let key = key_from_name(t).ok_or_else(|| KeyError::Unknown(t.to_string()))?;
            if combo.key.is_some() {
                return Err(KeyError::MultipleKeys(whole.to_string()));
            }
            combo.key = Some(key);
        }
        Ok(combo)
    }
}

impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical())
    }
}

impl Serialize for KeyCombo {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.canonical())
    }
}

impl<'de> Deserialize<'de> for KeyCombo {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        KeyCombo::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Modifier names from both providers. `super`/`meta`/`win` mean Command on macOS.
pub fn modifier_from_name(name: &str) -> Option<Mods> {
    Some(match name.to_ascii_lowercase().as_str() {
        "cmd" | "command" | "super" | "super_l" | "super_r" | "meta" | "meta_l" | "meta_r"
        | "win" => Mods::CMD,
        "ctrl" | "control" | "control_l" | "control_r" | "ctrl_l" | "ctrl_r" => Mods::CTRL,
        "alt" | "option" | "opt" | "alt_l" | "alt_r" => Mods::OPT,
        "shift" | "shift_l" | "shift_r" => Mods::SHIFT,
        "fn" => Mods::FN,
        _ => return None,
    })
}

fn key_from_name(name: &str) -> Option<Key> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_control() {
            return None;
        }
        return Some(Key::Char(c.to_lowercase().next().unwrap_or(c)));
    }
    let lower = name.to_ascii_lowercase();
    if let Some(n) = lower.strip_prefix('f').and_then(|d| d.parse::<u8>().ok()) {
        return (1..=20).contains(&n).then_some(Key::Named(NamedKey::F(n)));
    }
    use NamedKey::*;
    let named = match lower.as_str() {
        "return" | "enter" => Return,
        "kp_enter" | "keypadenter" => KeypadEnter,
        "tab" | "iso_left_tab" => Tab,
        "space" => Space,
        "backspace" | "back_space" => Backspace,
        "delete" | "del" | "forwarddelete" | "forward_delete" | "kp_delete" => ForwardDelete,
        "escape" | "esc" => Escape,
        "home" | "kp_home" => Home,
        "end" | "kp_end" => End,
        "page_up" | "pageup" | "prior" | "kp_prior" | "kp_page_up" => PageUp,
        "page_down" | "pagedown" | "next" | "kp_next" | "kp_page_down" => PageDown,
        "left" | "arrowleft" | "kp_left" => Left,
        "right" | "arrowright" | "kp_right" => Right,
        "up" | "arrowup" | "kp_up" => Up,
        "down" | "arrowdown" | "kp_down" => Down,
        "caps_lock" | "capslock" => CapsLock,
        "help" | "insert" => Help,
        _ => return keysym_char(&lower).map(Key::Char),
    };
    Some(Key::Named(named))
}

/// X11 keysym names for punctuation (xdotool style, used by Claude).
fn keysym_char(name: &str) -> Option<char> {
    Some(match name {
        "minus" => '-',
        "equal" => '=',
        "plus" => '+',
        "comma" => ',',
        "period" => '.',
        "slash" => '/',
        "backslash" => '\\',
        "semicolon" => ';',
        "apostrophe" | "quote" | "quoteright" => '\'',
        "grave" | "quoteleft" | "backquote" => '`',
        "bracketleft" => '[',
        "bracketright" => ']',
        "exclam" => '!',
        "at" => '@',
        "numbersign" => '#',
        "dollar" => '$',
        "percent" => '%',
        "asciicircum" => '^',
        "ampersand" => '&',
        "asterisk" => '*',
        "parenleft" => '(',
        "parenright" => ')',
        "underscore" => '_',
        "braceleft" => '{',
        "braceright" => '}',
        "bar" => '|',
        "colon" => ':',
        "quotedbl" => '"',
        "less" => '<',
        "greater" => '>',
        "question" => '?',
        "asciitilde" => '~',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canon(s: &str) -> String {
        KeyCombo::parse(s)
            .unwrap_or_else(|e| panic!("{s}: {e}"))
            .canonical()
    }

    #[test]
    fn anthropic_names() {
        let cases = [
            ("Return", "return"),
            ("Enter", "return"),
            ("KP_Enter", "keypadenter"),
            ("Tab", "tab"),
            ("space", "space"),
            ("BackSpace", "backspace"),
            ("Delete", "forwarddelete"),
            ("Escape", "escape"),
            ("Home", "home"),
            ("End", "end"),
            ("Page_Up", "pageup"),
            ("Prior", "pageup"),
            ("Page_Down", "pagedown"),
            ("Next", "pagedown"),
            ("Left", "left"),
            ("Right", "right"),
            ("Up", "up"),
            ("Down", "down"),
            ("F1", "f1"),
            ("F12", "f12"),
            ("super+c", "cmd+c"),
            ("cmd+shift+t", "shift+cmd+t"),
            ("ctrl+s", "ctrl+s"),
            ("alt+Tab", "opt+tab"),
            ("option+Left", "opt+left"),
            ("meta+q", "cmd+q"),
            ("fn+F11", "fn+f11"),
            ("ctrl+minus", "ctrl+-"),
            ("ctrl+plus", "ctrl+plus"),
            ("ctrl++", "ctrl+plus"),
            ("+", "plus"),
            ("shift", "shift"),
            ("super+space", "cmd+space"),
            ("ctrl+alt+super+period", "ctrl+opt+cmd+."),
        ];
        for (input, want) in cases {
            assert_eq!(canon(input), want, "{input}");
        }
    }

    #[test]
    fn openai_names() {
        let cases: &[(&[&str], &str)] = &[
            (&["ENTER"], "return"),
            (&["RETURN"], "return"),
            (&["TAB"], "tab"),
            (&["SPACE"], "space"),
            (&["BACKSPACE"], "backspace"),
            (&["DELETE"], "forwarddelete"),
            (&["DEL"], "forwarddelete"),
            (&["ESC"], "escape"),
            (&["ESCAPE"], "escape"),
            (&["HOME"], "home"),
            (&["END"], "end"),
            (&["PAGEUP"], "pageup"),
            (&["PAGEDOWN"], "pagedown"),
            (&["ARROWLEFT"], "left"),
            (&["LEFT"], "left"),
            (&["ARROWRIGHT"], "right"),
            (&["ARROWUP"], "up"),
            (&["ARROWDOWN"], "down"),
            (&["CTRL", "A"], "ctrl+a"),
            (&["CMD", "SHIFT", "T"], "shift+cmd+t"),
            (&["META", "C"], "cmd+c"),
            (&["COMMAND", "SPACE"], "cmd+space"),
            (&["ALT", "F4"], "opt+f4"),
            (&["OPTION", "LEFT"], "opt+left"),
            (&["CONTROL", "C"], "ctrl+c"),
            (&["SHIFT"], "shift"),
        ];
        for (keys, want) in cases {
            assert_eq!(
                KeyCombo::from_keys(keys).unwrap().canonical(),
                *want,
                "{keys:?}"
            );
        }
    }

    #[test]
    fn errors() {
        assert_eq!(KeyCombo::parse(""), Err(KeyError::Empty));
        assert!(matches!(
            KeyCombo::parse("hyper+x"),
            Err(KeyError::Unknown(_))
        ));
        assert!(matches!(
            KeyCombo::parse("ctrl+a+b"),
            Err(KeyError::MultipleKeys(_))
        ));
        assert!(matches!(KeyCombo::parse("F25"), Err(KeyError::Unknown(_))));
        assert!(matches!(
            KeyCombo::parse("ctrl++a"),
            Err(KeyError::Unknown(_))
        ));
        assert!(KeyCombo::from_keys::<&str>(&[]).is_err());
    }

    #[test]
    fn case_and_serde() {
        assert_eq!(canon("CTRL+SHIFT+A"), "ctrl+shift+a");
        assert_eq!(canon("Ctrl+Return"), "ctrl+return");
        let c: KeyCombo = serde_json::from_str(r#""cmd+z""#).unwrap();
        assert_eq!(serde_json::to_string(&c).unwrap(), r#""cmd+z""#);
        assert!(serde_json::from_str::<KeyCombo>(r#""bogus_key""#).is_err());
    }
}
