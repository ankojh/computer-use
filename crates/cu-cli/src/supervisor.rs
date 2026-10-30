//! Terminal prompts for confirmations, safety checks, and pauses.

use std::io::{self, BufRead, Write};

use cu_core::agent::{ConfirmRequest, PauseChoice, Supervisor};
use cu_core::SafetyCheck;

pub fn emit(value: serde_json::Value) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

/// The UI frontend owns stdin and closes it on abort, releasing a pending prompt.
pub struct UiSupervisor;
impl UiSupervisor {
    fn ask(&self, kind: &str, message: String, restore_app_pid: Option<i32>) -> bool {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        emit(
            serde_json::json!({"type":"confirmation","id":id,"kind":kind,"message":message,"restore_app_pid":restore_app_pid}),
        );
        let mut line = String::new();
        if io::stdin().lock().read_line(&mut line).unwrap_or(0) == 0 {
            return false;
        }
        serde_json::from_str::<serde_json::Value>(&line)
            .is_ok_and(|v| v["id"] == id && v["approved"] == true)
    }
}
impl Supervisor for UiSupervisor {
    fn confirm_action(&self, req: &ConfirmRequest) -> bool {
        self.ask(
            "action",
            format!(
                "{}\n{}{}",
                req.reason,
                req.describe(),
                req.element
                    .map(|e| format!("\n{}", e.label()))
                    .unwrap_or_default()
            ),
            req.focused_app.map(|app| app.pid),
        )
    }
    fn confirm_safety_checks(&self, checks: &[SafetyCheck]) -> bool {
        self.ask(
            "safety",
            checks
                .iter()
                .map(|c| {
                    format!(
                        "[{}] {}",
                        c.code.as_deref().unwrap_or("unknown"),
                        c.message.as_deref().unwrap_or("(no message)")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
            None,
        )
    }
    fn human_input_paused(&self) -> PauseChoice {
        if self.ask(
            "pause",
            "Someone moved the mouse on the Mac. Resume this task?".into(),
            None,
        ) {
            PauseChoice::Resume
        } else {
            PauseChoice::Abort
        }
    }
    fn notify(&self, message: &str) {
        emit(serde_json::json!({"type":"progress","message":message}));
    }
    fn task_progress(&self, progress: cu_core::agent::TaskProgress) {
        emit(serde_json::json!({"type":"task_progress","progress":progress}));
    }
    fn advisor_report(&self, report: &cu_core::types::AdvisorReport) {
        emit(serde_json::json!({"type":"advisor","advisor":report}));
    }
    fn completion_review(&self, review: &cu_core::CompletionReview) {
        emit(serde_json::json!({"type":"verification","verification":review}));
    }
    fn model_route(&self, route: &cu_core::ModelRoute) {
        emit(serde_json::json!({"type":"route","route":route}));
    }
    fn pointer_feedback(&self, feedback: Option<cu_core::PointerFeedback>) {
        emit(serde_json::json!({"type":"cursor","cursor":feedback}));
    }
}

pub struct TerminalSupervisor;

fn ask(prompt: &str) -> String {
    print!("{prompt}");
    let _ = io::stdout().flush();
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line).unwrap_or(0) == 0 {
        return String::new(); // EOF ⇒ treat as "no"
    }
    line.trim().to_lowercase()
}

impl Supervisor for TerminalSupervisor {
    fn confirm_action(&self, req: &ConfirmRequest) -> bool {
        println!();
        println!("⚠  Confirmation needed: {}", req.reason);
        println!("   action:  {}", req.describe());
        if let Some(el) = req.element {
            println!("   target:  {}", el.label());
        }
        matches!(ask("   Allow? [y/N] ").as_str(), "y" | "yes")
    }

    fn confirm_safety_checks(&self, checks: &[SafetyCheck]) -> bool {
        println!();
        println!("⚠  The provider flagged {} safety check(s):", checks.len());
        for c in checks {
            println!(
                "   [{}] {}",
                c.code.as_deref().unwrap_or("unknown"),
                c.message.as_deref().unwrap_or("(no message)")
            );
        }
        matches!(
            ask("   Acknowledge and continue? [y/N] ").as_str(),
            "y" | "yes"
        )
    }

    fn human_input_paused(&self) -> PauseChoice {
        println!();
        println!(
            "⏸  Human input detected; the agent is paused and all keys/buttons were released."
        );
        loop {
            match ask("   [r]esume or [a]bort? ").as_str() {
                "r" | "resume" => return PauseChoice::Resume,
                "a" | "abort" | "" => return PauseChoice::Abort,
                _ => {}
            }
        }
    }

    fn notify(&self, message: &str) {
        println!("» {message}");
    }
}
