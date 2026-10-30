//! Provider-neutral actions and the geometry types they use.

use serde::{Deserialize, Serialize};

use crate::keymap::KeyCombo;

/// A point in model image space `I`: pixels of the image we sent, top-left origin.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointI {
    pub x: f64,
    pub y: f64,
}

/// A rectangle in model image space given by its corners (Anthropic `zoom.region` order).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RectI {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// A point in global points `P`: top-left of the main display, y down.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointP {
    pub x: f64,
    pub y: f64,
}

/// A rectangle in global points `P`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RectP {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl RectP {
    pub fn contains(&self, p: PointP) -> bool {
        p.x >= self.x && p.y >= self.y && p.x <= self.x + self.w && p.y <= self.y + self.h
    }

    pub fn center(&self) -> PointP {
        PointP {
            x: self.x + self.w / 2.0,
            y: self.y + self.h / 2.0,
        }
    }
}

/// A point in device pixels `D`, relative to the display's framebuffer origin.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointD {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    #[default]
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

/// Modifier set. Bit values match the C ABI (`CU_MOD_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(into = "Vec<String>", try_from = "Vec<String>")]
pub struct Mods(pub u32);

impl Mods {
    pub const NONE: Mods = Mods(0);
    pub const CMD: Mods = Mods(1);
    pub const SHIFT: Mods = Mods(2);
    pub const OPT: Mods = Mods(4);
    pub const CTRL: Mods = Mods(8);
    pub const FN: Mods = Mods(16);

    /// Canonical order used when rendering combos ("ctrl+opt+shift+cmd+fn").
    pub const ORDER: [(Mods, &'static str); 5] = [
        (Mods::CTRL, "ctrl"),
        (Mods::OPT, "opt"),
        (Mods::SHIFT, "shift"),
        (Mods::CMD, "cmd"),
        (Mods::FN, "fn"),
    ];

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn names(self) -> Vec<&'static str> {
        Self::ORDER
            .iter()
            .filter(|(m, _)| self.contains(*m))
            .map(|(_, n)| *n)
            .collect()
    }
}

impl std::ops::BitOr for Mods {
    type Output = Mods;
    fn bitor(self, rhs: Mods) -> Mods {
        Mods(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for Mods {
    fn bitor_assign(&mut self, rhs: Mods) {
        self.0 |= rhs.0;
    }
}

impl From<Mods> for Vec<String> {
    fn from(m: Mods) -> Self {
        m.names().into_iter().map(String::from).collect()
    }
}

impl TryFrom<Vec<String>> for Mods {
    type Error = String;
    fn try_from(names: Vec<String>) -> Result<Self, String> {
        let mut m = Mods::NONE;
        for n in names {
            m |= crate::keymap::modifier_from_name(&n)
                .ok_or_else(|| format!("unknown modifier '{n}'"))?;
        }
        Ok(m)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "unit", rename_all = "snake_case")]
pub enum ScrollDelta {
    /// Wheel clicks (Anthropic `scroll_amount`). Positive dy scrolls content down.
    Clicks { dx: i32, dy: i32 },
    /// Pixel deltas (OpenAI `scroll_x/scroll_y`), in image space. Positive dy scrolls content down.
    Pixels { dx: f64, dy: f64 },
}

fn one() -> u8 {
    1
}

fn is_none(m: &Mods) -> bool {
    m.is_empty()
}

/// Provider-neutral action. All coordinates are in model image space `I` of the referenced observation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// No outbound arguments: the host uses only the separately supplied public brief.
    PublicResearch,
    /// Host-generated content-free metadata only, never screenshots/labels/text.
    UiJudgment,
    Screenshot,
    /// Anthropic only.
    Zoom {
        region: RectI,
    },
    MouseMove {
        at: PointI,
        #[serde(default, skip_serializing_if = "is_none")]
        mods: Mods,
    },
    Click {
        #[serde(default)]
        at: Option<PointI>,
        #[serde(default)]
        button: Button,
        #[serde(default = "one")]
        count: u8,
        #[serde(default, skip_serializing_if = "is_none")]
        mods: Mods,
    },
    /// At the current cursor position.
    MouseDown {
        #[serde(default)]
        button: Button,
    },
    MouseUp {
        #[serde(default)]
        button: Button,
    },
    /// At least 2 points.
    Drag {
        path: Vec<PointI>,
        #[serde(default, skip_serializing_if = "is_none")]
        mods: Mods,
    },
    Scroll {
        #[serde(default)]
        at: Option<PointI>,
        delta: ScrollDelta,
        #[serde(default, skip_serializing_if = "is_none")]
        mods: Mods,
    },
    Type {
        text: String,
        /// Optional exact AX identity; a local reliability guard, not authorization.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_focus: Option<crate::types::ExpectedFocus>,
    },
    Key {
        combo: KeyCombo,
        #[serde(default = "one")]
        repeat: u8,
    },
    HoldKey {
        combo: KeyCombo,
        duration_ms: u64,
    },
    /// `None` → wait for the screen to settle (default timeout).
    Wait {
        #[serde(default)]
        duration_ms: Option<u64>,
    },
    CursorPosition,
}

impl Action {
    /// Short name for logs.
    pub fn kind(&self) -> &'static str {
        match self {
            Action::PublicResearch => "public_research",
            Action::UiJudgment => "ui_judgment",
            Action::Screenshot => "screenshot",
            Action::Zoom { .. } => "zoom",
            Action::MouseMove { .. } => "mouse_move",
            Action::Click { .. } => "click",
            Action::MouseDown { .. } => "mouse_down",
            Action::MouseUp { .. } => "mouse_up",
            Action::Drag { .. } => "drag",
            Action::Scroll { .. } => "scroll",
            Action::Type { .. } => "type",
            Action::Key { .. } => "key",
            Action::HoldKey { .. } => "hold_key",
            Action::Wait { .. } => "wait",
            Action::CursorPosition => "cursor_position",
        }
    }

    /// Whether executing this posts input events (and should be followed by a settle wait).
    pub fn changes_state(&self) -> bool {
        !matches!(
            self,
            Action::PublicResearch
                | Action::UiJudgment
                | Action::Screenshot
                | Action::Zoom { .. }
                | Action::Wait { .. }
                | Action::CursorPosition
        )
    }
}

/// An action mapped to global points, ready for the platform to execute.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResolvedAction {
    MouseMove {
        at: PointP,
        mods: Mods,
    },
    /// `at: None` → current cursor position.
    Click {
        at: Option<PointP>,
        button: Button,
        count: u8,
        mods: Mods,
    },
    MouseButton {
        button: Button,
        down: bool,
    },
    Drag {
        path: Vec<PointP>,
        mods: Mods,
    },
    /// Pixel deltas are in points here (converted from image space).
    Scroll {
        at: Option<PointP>,
        delta: ScrollDelta,
        mods: Mods,
    },
    Type {
        text: String,
    },
    Key {
        combo: KeyCombo,
        repeat: u8,
    },
    HoldKey {
        combo: KeyCombo,
        duration_ms: u64,
    },
}

impl ResolvedAction {
    /// The point a pointer action targets (for AX hit tests and policy checks).
    pub fn target(&self) -> Option<PointP> {
        match self {
            ResolvedAction::MouseMove { at, .. } => Some(*at),
            ResolvedAction::Click { at, .. } | ResolvedAction::Scroll { at, .. } => *at,
            ResolvedAction::Drag { path, .. } => path.first().copied(),
            _ => None,
        }
    }

    /// Where the cursor ends up after this action, if it moves it.
    pub fn final_cursor(&self) -> Option<PointP> {
        match self {
            ResolvedAction::Drag { path, .. } => path.last().copied(),
            other => other.target(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_json_defaults() {
        let a: Action = serde_json::from_str(r#"{"type":"click","at":{"x":10,"y":20}}"#).unwrap();
        assert_eq!(
            a,
            Action::Click {
                at: Some(PointI { x: 10.0, y: 20.0 }),
                button: Button::Left,
                count: 1,
                mods: Mods::NONE
            }
        );
        let k: Action = serde_json::from_str(r#"{"type":"key","combo":"cmd+shift+t"}"#).unwrap();
        assert!(matches!(k, Action::Key { repeat: 1, .. }));
    }

    #[test]
    fn mods_roundtrip() {
        let m = Mods::CMD | Mods::SHIFT;
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, r#"["shift","cmd"]"#);
        assert_eq!(serde_json::from_str::<Mods>(&json).unwrap(), m);
        assert!(serde_json::from_str::<Mods>(r#"["hyper"]"#).is_err());
    }
}
