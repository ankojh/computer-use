//! Provider adapters, including subscription-backed Codex over its local app-server.

pub mod codex;
mod completion;
pub mod hybrid;
pub mod mock;
mod native_apps;
mod router;

pub use codex::CodexProvider;
pub use mock::{MockProvider, Script};
