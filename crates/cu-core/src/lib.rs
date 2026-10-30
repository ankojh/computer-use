//! Platform-agnostic core of the computer-use agent: actions, coordinate mapping, key names,
//! safety policy, run recording, and the agent loop.

pub mod action;
pub mod agent;
pub mod config;
pub mod coords;
pub mod keymap;
pub mod platform;
pub mod policy;
pub mod provider;
pub mod recorder;
pub mod testing;
pub mod types;

pub use action::{
    Action, Button, Mods, PointD, PointI, PointP, RectI, RectP, ResolvedAction, ScrollDelta,
};
pub use coords::CoordMapping;
pub use keymap::KeyCombo;
pub use platform::Platform;
pub use provider::Provider;
pub use types::*;
