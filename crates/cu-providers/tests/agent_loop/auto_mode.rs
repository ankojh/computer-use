//! No real desktop input or inference: opt-in automatic policy regression tests.
use super::*;
#[tokio::test]
async fn auto_executes_risky_input_and_acknowledges_provider_checks_without_prompts() {
    let mut h = Harness::new();
    h.mode = Mode::Auto;
    h.supervisor.approve = false;
    h.supervisor.approve_safety = false;
    let ran=h.run(json!({"turns":[{"actions":[{"type":"key","combo":"cmd+return"}],"safety_checks":[{"id":"sc_auto","message":"Synthetic check"}]}]})).await;
    assert_eq!(h.platform.executed().len(), 1);
    assert_eq!(ran.summary.confirmations, 0);
    assert!(h.supervisor.prompts.lock().unwrap().is_empty());
    assert_eq!(ran.received[0].acknowledged, vec!["sc_auto"]);
    assert!(ran
        .events
        .iter()
        .any(|e| e["kind"] == "safety_checks" && e["automatic"] == true && e["approved"] == true));
    assert!(ran.events.iter().any(|e| e["kind"] == "action_end"
        && e["consequential_attempted"] == true
        && e["approved"].is_null()));
}
#[tokio::test]
async fn automatic_consequential_attempt_never_enables_completion_or_format_replay() {
    for fail in [false, true] {
        for malformed in [false, true] {
            let mut h = Harness::new();
            h.mode = Mode::Auto;
            if fail {
                h.platform
                    .fail_on
                    .lock()
                    .unwrap()
                    .insert(0, "uncertain synthetic post".into());
            }
            let next = if malformed {
                json!({"stop":"invalid_plan","plan_rejection":{"code":"invalid_action_plan","summary":"Rejected before execution"}})
            } else {
                json!({"stop":"verify_completion","evidence_frames":[2],"verification":review("incomplete",2)})
            };
            let ran=h.run(json!({"turns":[{"actions":[{"type":"key","combo":"cmd+return"}]},next,{"actions":[click(4.,4.)]}]})).await;
            assert_eq!(ran.summary.status, RunStatus::NeedsInput);
            assert!(ran.repair_calls.is_empty());
            assert!(!ran
                .events
                .iter()
                .any(|e| e["reason"] == "completion_recovery"
                    || e["reason"] == "rejected_plan_repair"));
            assert!(h.platform.executed().len() <= 1);
            assert_eq!(ran.summary.confirmations, 0);
        }
    }
}
#[tokio::test]
async fn auto_keeps_hard_denials_and_exact_typing_focus_guard() {
    for own_controls in [false, true] {
        let mut h = Harness::new();
        h.mode = Mode::Auto;
        *h.platform.focused.lock().unwrap() = Some(cu_core::AxSummary {
            app: Some(cu_core::AppInfo {
                pid: 99,
                bundle_id: Some(
                    if own_controls {
                        "dev.ankojh.cu"
                    } else {
                        "example.editor"
                    }
                    .into(),
                ),
                name: None,
            }),
            element: Some(AxElement {
                role: Some("AXTextArea".into()),
                title: Some("Body".into()),
                ..Default::default()
            }),
            ..Default::default()
        });
        let ran=h.run(json!({"turns":[{"actions":[{"type":"type","text":"Synthetic text","expected_focus":{"role":"AXTextField","label":"Subject"}}]}]})).await;
        assert!(h.platform.executed().is_empty());
        assert_eq!(ran.summary.confirmations, 0);
        if own_controls {
            assert_eq!(ran.summary.actions_denied, 1);
        } else {
            assert_eq!(ran.summary.actions_failed, 1);
        }
    }
}
#[tokio::test]
async fn auto_still_pauses_on_human_interference() {
    let mut h = Harness::new();
    h.mode = Mode::Auto;
    h.supervisor.pause = PauseChoice::Abort;
    h.platform.human_input_before.lock().unwrap().push(1);
    let ran = h
        .run(json!({"turns":[{"actions":[click(1.,1.),click(2.,2.)]}]}))
        .await;
    assert_eq!(ran.summary.status, RunStatus::Aborted);
    assert_eq!(h.platform.executed().len(), 1);
    assert_eq!(*h.supervisor.prompts.lock().unwrap(), vec!["paused"]);
}
#[tokio::test]
async fn auto_stop_posts_no_input() {
    let mut h = Harness::new();
    h.mode = Mode::Auto;
    h.cancel.cancel();
    let ran = h
        .run(json!({"turns":[{"actions":[{"type":"key","combo":"cmd+return"}]}]}))
        .await;
    assert_eq!(ran.summary.status, RunStatus::Aborted);
    assert!(h.platform.executed().is_empty());
}
