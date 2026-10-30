//! `~/.config/cu/config.toml` (HANDOFF.md §14). Every field has a default; the file is optional.

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::platform::{ObserveOpts, SettleOpts};
use crate::policy::{Mode, PolicyConfig};
use crate::types::ImageFormat;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub provider: ProviderSection,
    pub anthropic: ModelConfig,
    pub openai: ModelConfig,
    pub codex: CodexConfig,
    pub hybrid: HybridConfig,
    pub capture: CaptureConfig,
    pub settle: SettleConfig,
    pub safety: SafetyConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderSection {
    pub default: String,
}

impl Default for ProviderSection {
    fn default() -> Self {
        ProviderSection {
            default: "codex".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    pub model: Option<String>,
    pub max_tokens: u32,
    pub screenshot_max_long_edge: u32,
}

impl Default for ModelConfig {
    fn default() -> Self {
        ModelConfig {
            model: None,
            max_tokens: 4096,
            screenshot_max_long_edge: 1440,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutingMode {
    #[default]
    Dynamic,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reasoning {
    Low,
    #[default]
    Medium,
    High,
}

impl Reasoning {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CodexConfig {
    /// Optional planner pin; dynamic routing still varies its reasoning.
    pub model: Option<String>,
    pub max_tokens: u32,
    pub screenshot_max_long_edge: u32,
    pub routing: RoutingMode,
    /// Used only in fixed mode.
    pub reasoning: Reasoning,
    /// Optional classifier pin; otherwise use the available fast vision model.
    pub router_model: Option<String>,
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            model: None,
            max_tokens: 4096,
            screenshot_max_long_edge: 1440,
            routing: RoutingMode::Dynamic,
            reasoning: Reasoning::Medium,
            router_model: None,
        }
    }
}

/// Helpers are enabled explicitly per run, not by readiness checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HybridConfig {
    /// Read only TYPESAFE_API_KEY from this explicitly configured dotenv file.
    /// Never exported into Codex's environment or sent to a planner.
    pub jev_env_file: Option<String>,
    pub max_requests: u32,
    pub timeout_ms: u64,
}
impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            jev_env_file: None,
            max_requests: 12,
            timeout_ms: 10000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureConfig {
    pub fps: u32,
    pub format: ImageFormat,
    pub jpeg_quality: f32,
    pub shows_cursor: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        CaptureConfig {
            fps: 30,
            format: ImageFormat::Jpeg,
            jpeg_quality: 0.78,
            shows_cursor: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SettleConfig {
    pub quiet_ms: u32,
    pub timeout_ms: u32,
    pub threshold: f32,
    /// Fixed wait while change detection (M7) is not available.
    pub fallback_ms: u32,
}

impl Default for SettleConfig {
    fn default() -> Self {
        let d = SettleOpts::default();
        SettleConfig {
            quiet_ms: d.quiet_ms,
            timeout_ms: d.timeout_ms,
            threshold: d.threshold,
            fallback_ms: d.fallback_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SafetyConfig {
    pub mode: Mode,
    pub max_steps: u32,
    pub max_minutes: f64,
    pub max_cost_usd: f64,
    /// Cap for a single `wait` / `hold_key`.
    pub max_wait_seconds: f64,
    pub sensitive_apps: Vec<String>,
    pub terminal_apps: Vec<String>,
    pub deny_element_patterns: Vec<String>,
    pub cmd_w_is_risky: bool,
    pub panic_hotkey: String,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        let p = PolicyConfig::default();
        SafetyConfig {
            mode: Mode::ConfirmRisky,
            max_steps: 50,
            max_minutes: 15.0,
            max_cost_usd: 5.0,
            max_wait_seconds: 30.0,
            sensitive_apps: p.sensitive_apps,
            terminal_apps: p.terminal_apps,
            deny_element_patterns: p.deny_element_patterns,
            cmd_w_is_risky: p.cmd_w_is_risky,
            panic_hotkey: "ctrl+opt+cmd+period".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub runs_dir: String,
    pub record_video: bool,
    pub log_requests: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        LoggingConfig {
            runs_dir: "~/.cu/runs".into(),
            record_video: false,
            log_requests: false,
        }
    }
}

pub fn expand_tilde(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ if p == "~" => std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| p.into()),
        _ => PathBuf::from(p),
    }
}

impl Config {
    pub fn default_path() -> PathBuf {
        expand_tilde("~/.config/cu/config.toml")
    }

    /// Loads `path`, or the default path if it exists, or built-in defaults.
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let (path, required) = match path {
            Some(p) => (p.to_path_buf(), true),
            None => (Self::default_path(), false),
        };
        match std::fs::read_to_string(&path) {
            Ok(s) => toml::from_str(&s).with_context(|| format!("parsing {}", path.display())),
            Err(e) if !required && e.kind() == std::io::ErrorKind::NotFound => {
                Ok(Config::default())
            }
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn runs_dir(&self) -> PathBuf {
        expand_tilde(&self.logging.runs_dir)
    }

    pub fn policy(&self) -> PolicyConfig {
        PolicyConfig {
            sensitive_apps: self.safety.sensitive_apps.clone(),
            terminal_apps: self.safety.terminal_apps.clone(),
            deny_element_patterns: self.safety.deny_element_patterns.clone(),
            cmd_w_is_risky: self.safety.cmd_w_is_risky,
        }
    }

    pub fn observe_opts(&self, max_long_edge: u32) -> ObserveOpts {
        ObserveOpts {
            max_long_edge,
            format: self.capture.format,
            jpeg_quality: self.capture.jpeg_quality,
            include_focused: true,
        }
    }

    pub fn settle_opts(&self) -> SettleOpts {
        SettleOpts {
            quiet_ms: self.settle.quiet_ms,
            timeout_ms: self.settle.timeout_ms,
            threshold: self.settle.threshold,
            after_frame_seq: 0,
            fallback_ms: self.settle.fallback_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_handoff_example() {
        let cfg: Config = toml::from_str(
            r#"
            [provider]
            default = "anthropic"
            [anthropic]
            model = "claude-opus-5-5"
            max_tokens = 4096
            screenshot_max_long_edge = 1440
            [openai]
            model = "gpt-6.1-sol"
            [capture]
            fps = 30
            format = "jpeg"
            jpeg_quality = 0.78
            shows_cursor = true
            [settle]
            quiet_ms = 150
            timeout_ms = 2000
            threshold = 0.002
            [safety]
            mode = "confirm-risky"
            max_steps = 50
            max_minutes = 15
            max_cost_usd = 5.0
            sensitive_apps = ["com.apple.keychainaccess"]
            panic_hotkey = "ctrl+opt+cmd+period"
            [logging]
            runs_dir = "~/.cu/runs"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.anthropic.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(cfg.safety.mode, Mode::ConfirmRisky);
        assert_eq!(cfg.safety.sensitive_apps, vec!["com.apple.keychainaccess"]);
        // Unspecified fields keep defaults.
        assert!(cfg
            .safety
            .terminal_apps
            .contains(&"com.apple.Terminal".to_string()));
        assert_eq!(cfg.settle.fallback_ms, 300);
    }

    #[test]
    fn codex_routing_defaults_and_fixed_override() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.codex.routing, RoutingMode::Dynamic);
        let cfg: Config =
            toml::from_str("[codex]\nrouting = 'fixed'\nmodel = 'gpt-6-astra'\nreasoning = 'high'")
                .unwrap();
        assert_eq!(cfg.codex.routing, RoutingMode::Fixed);
        assert_eq!(cfg.codex.reasoning, Reasoning::High);
        assert!(toml::from_str::<Config>("[codex]\nreasoning = 'ultra'").is_err());
        assert!(toml::from_str::<Config>("[codex]\nrouting = 'magic'").is_err());
    }

    #[test]
    fn empty_file_is_defaults() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.safety.max_steps, 50);
        assert!(cfg.runs_dir().ends_with(".cu/runs"));
    }
}
