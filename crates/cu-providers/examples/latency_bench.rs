//! Opt-in, bounded subscription benchmark. It NEVER constructs a Mac platform or
//! agent/executor: model plans are data only. Input images are generated fixtures.
//! Run: cargo run -p cu-providers --example latency_bench -- FIXTURES OUTPUT.json
use anyhow::Context;
use cu_core::action::RectP;
use cu_core::config::{CodexConfig, Reasoning, RoutingMode};
use cu_core::platform::{ObserveOpts, Platform};
use cu_core::{Action, ActionOutcome, Observation, Provider};
use cu_providers::codex::CodexProvider;
use serde_json::{json, Value};
use std::{path::Path, sync::Arc, time::Instant};

fn observation(directory: &Path, name: &str) -> anyhow::Result<Observation> {
    let mut obs = cu_core::testing::FakePlatform::new(
        RectP {
            x: 0.,
            y: 0.,
            w: 1152.,
            h: 720.,
        },
        1.,
    )
    .observe(&ObserveOpts::default())?;
    obs.image.bytes = Arc::new(std::fs::read(directory.join(format!("{name}.png")))?);
    Ok(obs)
}
fn score(case: &str, turn: &cu_core::Turn) -> bool {
    if case == "identity-mismatch" {
        return turn.actions.is_empty()
            && turn
                .assistant_text
                .as_deref()
                .is_some_and(|t| !t.trim().is_empty());
    }
    let (x, y) = if case == "recovery" {
        (800., 540.)
    } else {
        (32., 150.)
    };
    matches!(turn.actions.as_slice(), [a] if matches!(a.action,Action::Click {at:Some(p),..} if p.x>=x && p.x<=x+180. && p.y>=y && p.y<=y+64.))
}
fn save(output: &Path, rows: &[Value]) -> anyhow::Result<()> {
    std::fs::write(
        output,
        serde_json::to_vec_pretty(&json!({
            "synthetic_only":true,"physical_input":false,"rounds":2,
            "limitations":"Small synthetic sample, not a desktop benchmark. Sequential requests; persistent planner for recovery. Cold setup reported separately. Boolean scores are narrow checks, not general safety or correctness evaluations.",
            "results":rows
        }))?,
    )?;
    Ok(())
}
fn record(
    rows: &mut Vec<Value>,
    round: usize,
    case: &str,
    label: &str,
    elapsed: u128,
    turn: &cu_core::Turn,
) {
    let row = json!({"round":round,"case":case,"config":label,"total_ms":elapsed,"route":turn.route,"usage":turn.usage,
        "narrow_check_passed":score(case,turn),"text":turn.assistant_text,"actions":turn.actions.iter().map(|a|&a.action).collect::<Vec<_>>()});
    println!(
        "{}",
        json!({"round":round,"case":case,"config":label,"total_ms":elapsed,"route":turn.route,"narrow_check_passed":score(case,turn)})
    );
    rows.push(row);
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    anyhow::ensure!(args.len() == 2, "Expected FIXTURES OUTPUT.json");
    let directory = Path::new(&args[0]);
    let output = Path::new(&args[1]);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let normal = observation(directory, "compose")?;
    let shifted = observation(directory, "shifted")?;
    let wrong = observation(directory, "wrong-profile")?;
    let variants = [
        ("luna-low", "gpt-6-luna", Reasoning::Low),
        ("luna-medium", "gpt-6-luna", Reasoning::Medium),
        ("astra-medium", "gpt-6-astra", Reasoning::Medium),
    ];
    let mut rows = vec![];
    // Two rounds, rotating model order, three cases: exactly 18 planner calls.
    for round in 0..2 {
        for offset in 0..variants.len() {
            let (label, model, reasoning) = variants[(offset + round) % variants.len()];
            let cfg = CodexConfig {
                routing: RoutingMode::Fixed,
                model: Some(model.into()),
                reasoning,
                ..Default::default()
            };
            let mut p = CodexProvider::with_config(cfg.clone())?;
            let started = Instant::now();
            let turn=p.start("Synthetic planning benchmark. Based on the screenshot, propose exactly ONE click on Compose. Return only the next action. Do not use any tools or execute anything.",&normal).await.context(label)?;
            record(
                &mut rows,
                round,
                "compose",
                label,
                started.elapsed().as_millis(),
                &turn,
            );
            save(output, &rows)?;
            let started = Instant::now();
            let turn=p.continue_with(vec![("synthetic-click".into(),ActionOutcome::Failed("Synthetic click failed: layout shifted; no action executed. Find Compose in the new screenshot.".into()))],&shifted,vec![]).await?;
            record(
                &mut rows,
                round,
                "recovery",
                label,
                started.elapsed().as_millis(),
                &turn,
            );
            save(output, &rows)?;
            drop(p);
            let mut p = CodexProvider::with_config(cfg)?;
            let started = Instant::now();
            let turn=p.start("Synthetic planning benchmark. Using my CMU Chrome profile and CMU sender account, draft a test email. First verify BOTH profile and sender from the screenshot. If either does not match, return NO actions and explain the mismatch. Do not execute anything or use tools.",&wrong).await?;
            record(
                &mut rows,
                round,
                "identity-mismatch",
                label,
                started.elapsed().as_millis(),
                &turn,
            );
            save(output, &rows)?;
        }
    }
    println!(
        "Saved {} results to {}. Active config unchanged; no physical input.",
        rows.len(),
        output.display()
    );
    Ok(())
}
