//! Safety policy: risk classification and per-mode decisions (HANDOFF.md §12).

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::action::{Mods, ResolvedAction};
use crate::keymap::{Key, KeyCombo, NamedKey};
use crate::types::{AxElement, AxSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// Screenshots + model calls, no input events.
    Observe,
    /// Every action needs confirmation.
    ConfirmAll,
    /// Only risky actions need confirmation.
    #[default]
    ConfirmRisky,
    /// No confirmations except hard-deny rules and provider safety checks.
    Auto,
}

impl std::str::FromStr for Mode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "observe" => Ok(Mode::Observe),
            "confirm-all" => Ok(Mode::ConfirmAll),
            "confirm-risky" => Ok(Mode::ConfirmRisky),
            "auto" => Ok(Mode::Auto),
            _ => Err(format!(
                "unknown mode '{s}' (observe | confirm-all | confirm-risky | auto)"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Risk {
    Safe,
    Risky(String),
    Deny(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Allow,
    /// Observe mode: log what would happen, post nothing.
    Simulate,
    Confirm(String),
    Deny(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PolicyConfig {
    pub sensitive_apps: Vec<String>,
    pub terminal_apps: Vec<String>,
    /// Extra `(?i)` regexes on the target element's text that are always denied.
    pub deny_element_patterns: Vec<String>,
    /// Treat cmd+w as risky (closes windows/tabs).
    pub cmd_w_is_risky: bool,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        PolicyConfig {
            sensitive_apps: vec![
                "com.apple.keychainaccess".into(),
                "com.apple.systempreferences".into(),
                "com.apple.Passwords".into(),
            ],
            terminal_apps: vec![
                "com.apple.Terminal".into(),
                "com.googlecode.iterm2".into(),
                "com.mitchellh.ghostty".into(),
                "com.github.wez.wezterm".into(),
                "dev.warp.Warp-Stable".into(),
                "com.microsoft.VSCode".into(),
            ],
            deny_element_patterns: Vec::new(),
            cmd_w_is_risky: true,
        }
    }
}

/// What the policy knows about the target of an action.
#[derive(Debug, Clone, Default)]
pub struct PolicyContext<'a> {
    /// AX element under the pointer (pointer actions).
    pub element: Option<&'a AxElement>,
    pub focused: Option<&'a AxSummary>,
    /// PID of the terminal app that launched `cu`, if known.
    pub own_terminal_pid: Option<i32>,
}

pub struct Policy {
    pub mode: Mode,
    cfg: PolicyConfig,
    risky_words: Regex,
    deny_patterns: Vec<Regex>,
    secret_patterns: Vec<Regex>,
}

const RISKY_WORDS: &str = r"(?i)\b(delete|remove|erase|trash|empty|discard|send|submit|post|publish|pay|purchase|buy|order|checkout|transfer|confirm|sign|agree|accept|install|allow|grant|unlock|share|reply all)\b";

impl Policy {
    pub fn new(mode: Mode, cfg: PolicyConfig) -> anyhow::Result<Self> {
        let deny_patterns = cfg
            .deny_element_patterns
            .iter()
            .map(|p| Regex::new(&format!("(?i){p}")))
            .collect::<Result<_, _>>()?;
        Ok(Policy {
            mode,
            cfg,
            risky_words: Regex::new(RISKY_WORDS).unwrap(),
            deny_patterns,
            secret_patterns: vec![
                Regex::new(r"\bsk-[A-Za-z0-9_\-]{16,}").unwrap(),
                Regex::new(r"\b(ghp|gho|ghu|ghs|github_pat)_[A-Za-z0-9_]{20,}").unwrap(),
                Regex::new(r"\bAKIA[0-9A-Z]{16}\b").unwrap(),
                Regex::new(r"\bxox[abpr]-[A-Za-z0-9\-]{10,}").unwrap(),
                Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----").unwrap(),
            ],
        })
    }

    pub fn decide(&self, action: &ResolvedAction, ctx: &PolicyContext) -> (Decision, Risk) {
        let risk = self.classify(action, ctx);
        let decision = match (&risk, self.mode) {
            (Risk::Deny(r), _) => Decision::Deny(r.clone()),
            (_, Mode::Observe) => Decision::Simulate,
            (Risk::Risky(r), Mode::ConfirmAll | Mode::ConfirmRisky) => Decision::Confirm(r.clone()),
            (Risk::Safe, Mode::ConfirmAll) => Decision::Confirm("confirm-all mode".into()),
            (_, Mode::ConfirmRisky | Mode::Auto) => Decision::Allow,
        };
        (decision, risk)
    }

    pub fn classify(&self, action: &ResolvedAction, ctx: &PolicyContext) -> Risk {
        let focused_bundle = ctx.focused.and_then(|f| f.bundle_id());
        let focused_pid = ctx.focused.and_then(|f| f.app.as_ref()).map(|a| a.pid);
        let in_terminal =
            focused_bundle.is_some_and(|b| self.cfg.terminal_apps.iter().any(|t| t == b));
        // Native WhatsApp uses bare Return to send; missing AX labels must not
        // turn chat submission into routine keyboard input. Exempt a positively
        // identified search field, not arbitrary help/value text.
        let whatsapp_submission = focused_bundle == Some("net.whatsapp.WhatsApp")
            && !ctx
                .focused
                .and_then(|f| f.element.as_ref())
                .is_some_and(|e| {
                    e.role.as_deref() == Some("AXSearchField")
                        || (e.role.as_deref() == Some("AXTextField")
                            && e.title
                                .iter()
                                .chain(e.description.iter())
                                .chain(e.identifier.iter())
                                .any(|s| s.trim().eq_ignore_ascii_case("Search")))
                });
        let is_keyboard = matches!(
            action,
            ResolvedAction::Type { .. }
                | ResolvedAction::Key { .. }
                | ResolvedAction::HoldKey { .. }
        );

        // --- hard deny ---
        if is_keyboard && focused_bundle == Some("dev.ankojh.cu") {
            return Risk::Deny("keyboard input into Cu's own controls is not allowed; close the menu-bar panel first".into());
        }
        if is_keyboard && ctx.own_terminal_pid.is_some() && focused_pid == ctx.own_terminal_pid {
            return Risk::Deny(
                "keyboard input into the terminal running cu is not allowed; focus another app first".into(),
            );
        }
        if let Some(el) = ctx.element {
            let texts: Vec<&str> = el.texts().collect();
            if self
                .deny_patterns
                .iter()
                .any(|re| texts.iter().any(|t| re.is_match(t)))
            {
                return Risk::Deny(format!("{} matches a configured deny rule", el.label()));
            }
            let el_bundle = el.app.as_ref().and_then(|a| a.bundle_id.as_deref());
            // Feedback must never turn an underlying Send/Delete hit test into
            // an apparently safe Cu element. Nor may Cu approve its own controls.
            if el_bundle == Some("dev.ankojh.cu") {
                return Risk::Deny(
                    "input into Cu's own controls or cursor overlay is not allowed".into(),
                );
            }
            let window = ctx
                .focused
                .and_then(|f| f.window_title.as_deref())
                .unwrap_or("");
            if el_bundle == Some("com.apple.systempreferences")
                && (window.contains("Privacy") || texts.iter().any(|t| t.contains("Privacy")))
                && matches!(
                    el.role.as_deref(),
                    Some("AXCheckBox" | "AXSwitch" | "AXToggle")
                )
            {
                return Risk::Deny(
                    "changing Privacy & Security permission toggles is not allowed".into(),
                );
            }
        }

        // Pointer motion and scrolling do not activate controls or change account
        // state, even in Settings. Hard denials above and confirm-all still win.
        if matches!(
            action,
            ResolvedAction::MouseMove { .. } | ResolvedAction::Scroll { .. }
        ) {
            return Risk::Safe;
        }

        // Exact global navigation shortcuts do not modify the sensitive app.
        // All hard-denies above still win; confirm-all still confirms this Risk::Safe.
        if let ResolvedAction::Key { combo, repeat: 1 } = action {
            if [
                KeyCombo::with(Mods::CMD, Key::Named(NamedKey::Tab)),
                KeyCombo::with(Mods::CMD | Mods::SHIFT, Key::Named(NamedKey::Tab)),
                KeyCombo::with(Mods::CMD, Key::Named(NamedKey::Space)),
            ]
            .contains(combo)
            {
                return Risk::Safe;
            }
        }
        // --- risky ---
        if let Some(b) = focused_bundle {
            if self.cfg.sensitive_apps.iter().any(|s| s == b) {
                return Risk::Risky(format!("focused app {b} is on the sensitive list"));
            }
        }
        match action {
            ResolvedAction::Key { combo, .. } | ResolvedAction::HoldKey { combo, .. } => {
                if let Some(why) = self.risky_combo(combo) {
                    return Risk::Risky(why);
                }
                if (combo == &KeyCombo::with(Mods::NONE, Key::Named(NamedKey::Return))
                    || combo == &KeyCombo::with(Mods::NONE, Key::Named(NamedKey::Space)))
                    && ctx
                        .focused
                        .and_then(|f| f.element.as_ref())
                        .is_some_and(|e| e.texts().any(|t| self.risky_words.is_match(t)))
                {
                    return Risk::Risky("activating a consequential focused control".into());
                }
                if whatsapp_submission
                    && (combo == &KeyCombo::with(Mods::NONE, Key::Named(NamedKey::Return))
                        || combo == &KeyCombo::with(Mods::NONE, Key::Named(NamedKey::KeypadEnter)))
                {
                    return Risk::Risky("Return in native WhatsApp may send a message".into());
                }
                if in_terminal
                    && (combo.is_named(NamedKey::Return) || combo.is_named(NamedKey::KeypadEnter))
                {
                    return Risk::Risky("Return in a terminal executes a command".into());
                }
            }
            ResolvedAction::Type { text } => {
                if whatsapp_submission && (text.contains('\n') || text.contains('\r')) {
                    return Risk::Risky(
                        "typing newlines in native WhatsApp may send messages".into(),
                    );
                }
                if in_terminal && (text.contains('\n') || text.contains('\r')) {
                    return Risk::Risky("typing a newline in a terminal executes a command".into());
                }
                let secure_field = ctx
                    .focused
                    .and_then(|f| f.element.as_ref())
                    .is_some_and(|e| e.secure);
                if ctx.focused.is_some_and(|f| f.secure_input) || secure_field {
                    return Risk::Risky("typing into a secure (password) field".into());
                }
                if let Some(why) = self.looks_secret(text) {
                    return Risk::Risky(format!("typed text looks like {why}"));
                }
            }
            ResolvedAction::Click { .. }
            | ResolvedAction::Drag { .. }
            | ResolvedAction::MouseButton { down: true, .. } => {
                if let Some(el) = ctx.element {
                    // Focusing an editable field is not Send/Delete just because
                    // its help text explains a send shortcut. Typing credentials
                    // and activating focused controls remain separately guarded.
                    let focusing_field = matches!(action, ResolvedAction::Click { .. })
                        && matches!(
                            el.role.as_deref(),
                            Some("AXTextField" | "AXTextArea" | "AXSearchField" | "AXComboBox")
                        );
                    let consequential_label = if focusing_field {
                        [&el.title, &el.description, &el.identifier]
                            .into_iter()
                            .filter_map(|s| s.as_deref())
                            .any(|t| self.risky_words.is_match(t))
                    } else {
                        el.texts().any(|t| self.risky_words.is_match(t))
                    };
                    if consequential_label {
                        return Risk::Risky(format!("clicking {}", el.label()));
                    }
                }
            }
            _ => {}
        }
        Risk::Safe
    }

    fn risky_combo(&self, combo: &KeyCombo) -> Option<String> {
        let c = |m: Mods, k: Key| KeyCombo::with(m, k);
        let q = Key::Char('q');
        let risky = [
            (
                c(Mods::CMD, Key::Named(NamedKey::Return)),
                "cmd+return may send or submit",
            ),
            (
                c(Mods::CTRL, Key::Named(NamedKey::Return)),
                "ctrl+return may send or submit",
            ),
            (
                c(Mods::CMD | Mods::SHIFT, Key::Named(NamedKey::Return)),
                "cmd+shift+return may send or submit",
            ),
            (c(Mods::CMD, q), "cmd+q quits the app"),
            (c(Mods::CMD | Mods::OPT, q), "cmd+opt+q quits the app"),
            (c(Mods::CTRL | Mods::CMD, q), "ctrl+cmd+q locks the screen"),
            (c(Mods::CMD | Mods::SHIFT, q), "cmd+shift+q logs out"),
            (
                c(Mods::CMD | Mods::OPT | Mods::SHIFT, q),
                "cmd+opt+shift+q logs out immediately",
            ),
            (
                c(Mods::CMD, Key::Named(NamedKey::Backspace)),
                "cmd+backspace deletes",
            ),
            (
                c(Mods::CMD, Key::Named(NamedKey::ForwardDelete)),
                "cmd+delete deletes",
            ),
            (
                c(Mods::CMD | Mods::SHIFT, Key::Named(NamedKey::Backspace)),
                "cmd+shift+backspace empties the trash",
            ),
            (
                c(
                    Mods::CMD | Mods::OPT | Mods::SHIFT,
                    Key::Named(NamedKey::Backspace),
                ),
                "empties the trash",
            ),
            (
                c(Mods::CMD | Mods::OPT, Key::Named(NamedKey::Escape)),
                "cmd+opt+esc opens Force Quit",
            ),
        ];
        if let Some((_, why)) = risky.iter().find(|(r, _)| r == combo) {
            return Some((*why).into());
        }
        if self.cfg.cmd_w_is_risky && *combo == c(Mods::CMD, Key::Char('w')) {
            return Some("cmd+w closes the window/tab".into());
        }
        None
    }

    fn looks_secret(&self, text: &str) -> Option<&'static str> {
        if self.secret_patterns.iter().any(|re| re.is_match(text)) {
            return Some("an API key or private key");
        }
        if has_luhn_card_number(text) {
            return Some("a payment card number");
        }
        if text.split_whitespace().any(is_high_entropy_token) {
            return Some("a password or token");
        }
        None
    }
}

/// 13–19 digit runs (spaces/dashes allowed) that pass the Luhn check.
fn has_luhn_card_number(text: &str) -> bool {
    static CARD: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"\b\d(?:[ -]?\d){12,18}\b").unwrap());
    CARD.find_iter(text).any(|m| {
        let digits: Vec<u32> = m.as_str().chars().filter_map(|c| c.to_digit(10)).collect();
        luhn(&digits)
    })
}

fn luhn(digits: &[u32]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let x = d * 2;
                if x > 9 {
                    x - 9
                } else {
                    x
                }
            } else {
                d
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// Long tokens mixing character classes with high Shannon entropy, e.g. `x7Gq9!pL2...`.
fn is_high_entropy_token(t: &str) -> bool {
    if t.len() < 20 || t.contains("://") {
        return false;
    }
    let classes = [
        t.chars().any(|c| c.is_ascii_lowercase()),
        t.chars().any(|c| c.is_ascii_uppercase()),
        t.chars().any(|c| c.is_ascii_digit()),
        t.chars().any(|c| !c.is_ascii_alphanumeric()),
    ]
    .iter()
    .filter(|&&b| b)
    .count();
    if classes < 3 {
        return false;
    }
    let mut counts = std::collections::HashMap::new();
    for c in t.chars() {
        *counts.entry(c).or_insert(0usize) += 1;
    }
    let n = t.chars().count() as f64;
    let entropy: f64 = counts
        .values()
        .map(|&k| {
            let p = k as f64 / n;
            -p * p.log2()
        })
        .sum();
    entropy > 4.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Button, PointP};
    use crate::types::AppInfo;

    fn policy(mode: Mode) -> Policy {
        Policy::new(
            mode,
            PolicyConfig {
                deny_element_patterns: vec![r"\bformat disk\b".into()],
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn key(s: &str) -> ResolvedAction {
        ResolvedAction::Key {
            combo: KeyCombo::parse(s).unwrap(),
            repeat: 1,
        }
    }

    fn typing(s: &str) -> ResolvedAction {
        ResolvedAction::Type { text: s.into() }
    }

    fn click() -> ResolvedAction {
        ResolvedAction::Click {
            at: Some(PointP { x: 1.0, y: 1.0 }),
            button: Button::Left,
            count: 1,
            mods: Mods::NONE,
        }
    }

    fn app(bundle: &str, pid: i32) -> AxSummary {
        AxSummary {
            app: Some(AppInfo {
                pid,
                name: None,
                bundle_id: Some(bundle.into()),
            }),
            ..Default::default()
        }
    }

    fn button(title: &str) -> AxElement {
        AxElement {
            role: Some("AXButton".into()),
            title: Some(title.into()),
            ..Default::default()
        }
    }

    #[test]
    fn native_whatsapp_submit_is_critical_but_plain_typing_and_search_are_routine() {
        let mut chat = app("net.whatsapp.WhatsApp", 55);
        chat.element = Some(AxElement {
            role: Some("AXTextArea".into()),
            ..Default::default()
        });
        let p = policy(Mode::ConfirmRisky);
        for element in [chat.element.clone(), None] {
            chat.element = element;
            let ctx = PolicyContext {
                focused: Some(&chat),
                ..Default::default()
            };
            for action in [
                key("return"),
                key("keypadenter"),
                typing("line one\nline two"),
                typing("line\r"),
            ] {
                assert!(matches!(p.decide(&action, &ctx).0, Decision::Confirm(_)));
            }
            for action in [
                typing("ordinary message text"),
                key("shift+return"),
                key("cmd+f"),
            ] {
                assert!(matches!(p.decide(&action, &ctx).0, Decision::Allow));
            }
        }
        chat.element = Some(AxElement {
            role: Some("AXSearchField".into()),
            ..Default::default()
        });
        assert!(matches!(
            p.decide(
                &key("return"),
                &PolicyContext {
                    focused: Some(&chat),
                    ..Default::default()
                }
            )
            .0,
            Decision::Allow
        ));
        let other = app("com.example.editor", 56);
        assert!(matches!(
            p.decide(
                &typing("line one\nline two"),
                &PolicyContext {
                    focused: Some(&other),
                    ..Default::default()
                }
            )
            .0,
            Decision::Allow
        ));
    }
    #[test]
    fn agent_cannot_type_into_or_approve_its_own_menu_bar_panel() {
        let cu = app("dev.ankojh.cu", 42);
        let ctx = PolicyContext {
            focused: Some(&cu),
            ..Default::default()
        };
        for mode in [
            Mode::Auto,
            Mode::ConfirmRisky,
            Mode::ConfirmAll,
            Mode::Observe,
        ] {
            for action in [
                typing("approve"),
                key("return"),
                key("cmd+return"),
                key("cmd+space"),
            ] {
                assert!(matches!(
                    policy(mode).decide(&action, &ctx).0,
                    Decision::Deny(_)
                ));
            }
        }
    }

    #[test]
    fn routine_pointer_navigation_and_field_focus_do_not_request_approval() {
        let settings = app("com.apple.systempreferences", 12);
        let movement = ResolvedAction::MouseMove {
            at: PointP { x: 10., y: 20. },
            mods: Mods::NONE,
        };
        let mut field = button("Message body");
        field.role = Some("AXTextArea".into());
        field.help = Some("Press Return to send".into());
        assert_eq!(
            policy(Mode::ConfirmRisky)
                .decide(
                    &movement,
                    &PolicyContext {
                        focused: Some(&settings),
                        ..Default::default()
                    }
                )
                .0,
            Decision::Allow
        );
        assert_eq!(
            policy(Mode::ConfirmRisky)
                .decide(
                    &click(),
                    &PolicyContext {
                        element: Some(&field),
                        ..Default::default()
                    }
                )
                .0,
            Decision::Allow
        );
        assert!(matches!(
            policy(Mode::ConfirmAll)
                .decide(&movement, &PolicyContext::default())
                .0,
            Decision::Confirm(_)
        ));
        let mut explicit_send = field.clone();
        explicit_send.title = Some("Send".into());
        assert!(matches!(
            policy(Mode::ConfirmRisky)
                .decide(
                    &click(),
                    &PolicyContext {
                        element: Some(&explicit_send),
                        ..Default::default()
                    }
                )
                .0,
            Decision::Confirm(_)
        ));
        let focus = AxSummary {
            element: Some(field),
            ..Default::default()
        };
        assert!(matches!(
            policy(Mode::ConfirmRisky)
                .decide(
                    &key("return"),
                    &PolicyContext {
                        focused: Some(&focus),
                        ..Default::default()
                    }
                )
                .0,
            Decision::Confirm(_)
        ));
    }
    #[test]
    fn routine_window_close_override_does_not_disable_consequential_guards() {
        let p = Policy::new(
            Mode::ConfirmRisky,
            PolicyConfig {
                cmd_w_is_risky: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            p.decide(&key("cmd+w"), &PolicyContext::default()).0,
            Decision::Allow
        );
        for combo in ["cmd+return", "ctrl+return", "cmd+backspace", "cmd+shift+q"] {
            assert!(matches!(
                p.decide(&key(combo), &PolicyContext::default()).0,
                Decision::Confirm(_)
            ));
        }
        for title in ["Send", "Delete", "Pay", "Publish", "Allow", "Sign in"] {
            let element = button(title);
            assert!(matches!(
                p.decide(
                    &click(),
                    &PolicyContext {
                        element: Some(&element),
                        ..Default::default()
                    }
                )
                .0,
                Decision::Confirm(_)
            ));
        }
    }

    #[test]
    fn global_navigation_leaves_settings_without_suppressing_real_approvals() {
        let settings = app("com.apple.systempreferences", 12);
        let ctx = PolicyContext {
            focused: Some(&settings),
            ..Default::default()
        };
        for shortcut in ["cmd+tab", "cmd+shift+tab", "cmd+space"] {
            assert!(matches!(
                policy(Mode::ConfirmRisky).decide(&key(shortcut), &ctx).0,
                Decision::Allow
            ));
            assert!(matches!(
                policy(Mode::ConfirmAll).decide(&key(shortcut), &ctx).0,
                Decision::Confirm(_)
            ));
        }
        assert!(matches!(
            policy(Mode::ConfirmRisky).decide(&key("return"), &ctx).0,
            Decision::Confirm(_)
        ));
        assert!(matches!(
            policy(Mode::ConfirmRisky).decide(&key("cmd+q"), &ctx).0,
            Decision::Confirm(_)
        ));
        for shortcut in ["cmd+return", "ctrl+return", "cmd+shift+return"] {
            assert!(matches!(
                policy(Mode::ConfirmRisky)
                    .decide(&key(shortcut), &PolicyContext::default())
                    .0,
                Decision::Confirm(_)
            ));
        }
        let send = button("Send");
        assert!(matches!(
            policy(Mode::ConfirmRisky)
                .decide(
                    &click(),
                    &PolicyContext {
                        element: Some(&send),
                        ..Default::default()
                    }
                )
                .0,
            Decision::Confirm(_)
        ));
    }

    #[test]
    fn risky_classification_table() {
        let p = policy(Mode::ConfirmRisky);
        let none = PolicyContext::default();
        let terminal = app("com.apple.Terminal", 10);
        let in_term = PolicyContext {
            focused: Some(&terminal),
            ..Default::default()
        };
        let mut secure = app("com.apple.Safari", 11);
        secure.secure_input = true;
        let in_secure = PolicyContext {
            focused: Some(&secure),
            ..Default::default()
        };
        let keychain = app("com.apple.keychainaccess", 12);
        let in_keychain = PolicyContext {
            focused: Some(&keychain),
            ..Default::default()
        };
        let delete = button("Delete");
        let on_delete = PolicyContext {
            element: Some(&delete),
            ..Default::default()
        };
        let ok_btn = button("Open");
        let on_ok = PolicyContext {
            element: Some(&ok_btn),
            ..Default::default()
        };

        let cases: Vec<(&str, ResolvedAction, &PolicyContext, bool)> = vec![
            ("cmd+q", key("cmd+q"), &none, true),
            ("cmd+opt+q", key("cmd+opt+q"), &none, true),
            ("cmd+w", key("super+w"), &none, true),
            ("cmd+backspace", key("cmd+BackSpace"), &none, true),
            (
                "cmd+shift+backspace",
                key("cmd+shift+BackSpace"),
                &none,
                true,
            ),
            ("cmd+opt+esc", key("cmd+alt+Escape"), &none, true),
            ("ctrl+cmd+q", key("ctrl+cmd+q"), &none, true),
            ("cmd+shift+q", key("cmd+shift+q"), &none, true),
            ("cmd+c", key("cmd+c"), &none, false),
            ("return elsewhere", key("Return"), &none, false),
            ("return in terminal", key("Return"), &in_term, true),
            ("newline in terminal", typing("ls\n"), &in_term, true),
            ("plain typing in terminal", typing("ls"), &in_term, false),
            ("secure input", typing("hunter2"), &in_secure, true),
            ("api key", typing("sk-abcdefghijklmnop1234"), &none, true),
            ("card", typing("4111 1111 1111 1111"), &none, true),
            ("not a card", typing("4111 1111 1111 1112"), &none, false),
            ("password-ish", typing("Xq7!pL2#vB9$mN4&zR8w"), &none, true),
            (
                "prose",
                typing("Hello, this is a perfectly normal sentence."),
                &none,
                false,
            ),
            (
                "url",
                typing("https://example.com/a/very/long/path?with=Query&and=1"),
                &none,
                false,
            ),
            ("sensitive app", click(), &in_keychain, true),
            ("click delete", click(), &on_delete, true),
            ("click open", click(), &on_ok, false),
        ];
        for (name, action, ctx, risky) in cases {
            let r = p.classify(&action, ctx);
            assert_eq!(matches!(r, Risk::Risky(_)), risky, "{name}: {r:?}");
        }
    }

    #[test]
    fn hard_deny() {
        let p = policy(Mode::Auto);
        let term = app("com.apple.Terminal", 42);
        let own = PolicyContext {
            focused: Some(&term),
            own_terminal_pid: Some(42),
            ..Default::default()
        };
        assert!(matches!(
            p.decide(&typing("rm -rf ~"), &own).0,
            Decision::Deny(_)
        ));
        // Clicking our terminal is fine; only keyboard input is denied.
        assert_eq!(p.decide(&click(), &own).0, Decision::Allow);

        let el = button("Format Disk now");
        let ctx = PolicyContext {
            element: Some(&el),
            ..Default::default()
        };
        assert!(matches!(p.decide(&click(), &ctx).0, Decision::Deny(_)));

        let mut toggle = AxElement {
            role: Some("AXCheckBox".into()),
            title: Some("Terminal".into()),
            ..Default::default()
        };
        toggle.app = Some(AppInfo {
            pid: 1,
            name: None,
            bundle_id: Some("com.apple.systempreferences".into()),
        });
        let mut settings = app("com.apple.systempreferences", 1);
        settings.window_title = Some("Privacy & Security".into());
        let ctx = PolicyContext {
            element: Some(&toggle),
            focused: Some(&settings),
            ..Default::default()
        };
        assert!(matches!(p.decide(&click(), &ctx).0, Decision::Deny(_)));
    }

    #[test]
    fn modes() {
        let none = PolicyContext::default();
        assert_eq!(
            policy(Mode::Observe).decide(&click(), &none).0,
            Decision::Simulate
        );
        assert!(matches!(
            policy(Mode::ConfirmAll).decide(&click(), &none).0,
            Decision::Confirm(_)
        ));
        assert_eq!(
            policy(Mode::ConfirmRisky).decide(&click(), &none).0,
            Decision::Allow
        );
        assert!(matches!(
            policy(Mode::ConfirmRisky).decide(&key("cmd+q"), &none).0,
            Decision::Confirm(_)
        ));
        assert_eq!(
            policy(Mode::Auto).decide(&key("cmd+q"), &none).0,
            Decision::Allow
        );
    }
}
