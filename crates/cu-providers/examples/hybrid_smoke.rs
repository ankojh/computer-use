//! Explicitly opted-in synthetic helper smoke. No Codex, desktop capture/input,
//! task/email/account data, browser, login, or retries. Never run implicitly.
use anyhow::{ensure, Context};
use cu_core::{config::HybridConfig, platform::Platform, types::AdvisorKind};
use cu_providers::hybrid::Hybrid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(args == ["--live-public-only"], "Requires --live-public-only: at most one Parallel web_search, one web_decisions and one Jev request; max five HTTP requests including MCP handshake. Requires fresh operator authorization.");
    let config = HybridConfig {
        max_requests: 5,
        jev_env_file: std::env::var("CU_JEV_ENV_FILE").ok(),
        ..Default::default()
    };
    let mut helper = Hybrid::new(
        &config,
        Some("Find official Rust documentation explaining ownership and borrowing".into()),
    )?;
    let platform = cu_core::testing::FakePlatform::default();
    let mut observation = platform.observe(&Default::default())?; // Generated image only; never transmitted.
    observation.focused = Some(cu_core::AxSummary {
        element: Some(cu_core::AxElement {
            role: Some("AXTextField".into()),
            enabled: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    });
    let mut failed = false;
    for kind in [AdvisorKind::PublicResearch, AdvisorKind::UiJudgment] {
        let report = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            helper.advise(kind, &observation),
        )
        .await
        .context("Synthetic helper reached 15-second total deadline; not retried")?;
        failed |= report.status != "ok";
        println!("{}", serde_json::to_string(&report)?); // Only public/synthetic report, never key/session/header.
    }
    ensure!(
        !failed,
        "At least one helper unavailable; do not treat fallback as live success"
    );
    Ok(())
}
