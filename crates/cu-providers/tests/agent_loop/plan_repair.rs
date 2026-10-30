use super::*;
fn rejected() -> Value {
    json!({"stop":"invalid_plan","plan_rejection":{"code":"invalid_focus_identity","summary":"Rejected before execution"},"usage":cu_core::Usage{input_tokens:13,..Default::default()}})
}
#[tokio::test]
async fn one_repair_uses_fresh_frame_retains_usage_and_still_requires_verification() {
    let h = Harness::new();
    let ran=h.run(json!({"turns":[rejected(),{"stop":"continue","actions":[{"type":"screenshot"}],"usage":cu_core::Usage{input_tokens:17,..Default::default()}},
        {"stop":"verify_completion","text":"Read the synthetic desktop","evidence_frames":[2],"verification":{"verdict":"verified","summary":"Synthetic frame observed","checks":[{"requirement":"Observe synthetic frame","verdict":"verified","evidence":"Supplied synthetic screenshot","frames":[2],"advisor_ids":[]}],"model":"mock","reasoning":"medium","latency_ms":1,"usage":cu_core::Usage::default()}}]})).await;
    assert_eq!(ran.summary.status, RunStatus::Done);
    assert_eq!(ran.summary.plans_rejected, 1);
    assert_eq!(ran.summary.steps, 3);
    assert_eq!(ran.summary.usage.input_tokens, 30);
    assert_eq!(ran.summary.model_latency_ms.count, 4); // three replies plus verifier
    assert_eq!(ran.repair_calls, vec![("invalid_focus_identity".into(), 2)]);
    assert!(h.platform.executed().is_empty());
    assert!(ran
        .events
        .iter()
        .any(|e| e["kind"] == "plan_rejected" && e["executed_from_reply"] == 0));
    assert!(ran.events.iter().any(|e| e["kind"] == "model_response"
        && e["stop"] == "invalid_plan"
        && e["usage"]["input_tokens"] == 13));
}
#[tokio::test]
async fn second_rejection_stops_without_repair_storm_or_guard_bypass() {
    let h = Harness::new();
    let ran = h
        .run(json!({"turns":[rejected(),rejected(),{"actions":[click(3.,3.)]}]}))
        .await;
    assert_eq!(ran.summary.status, RunStatus::Error);
    assert_eq!(ran.summary.plans_rejected, 2);
    assert_eq!(ran.summary.usage.input_tokens, 26);
    assert_eq!(ran.repair_calls.len(), 1);
    assert!(h.platform.executed().is_empty());
}
#[tokio::test]
async fn denied_or_approved_action_blocks_any_repair_and_replay() {
    for approve in [false, true] {
        let mut h = Harness::new();
        h.supervisor.approve = approve;
        let ran=h.run(json!({"turns":[{"actions":[{"type":"key","combo":"cmd+q"}]},rejected(),{"actions":[click(3.,3.)]}]})).await;
        assert_eq!(ran.summary.status, RunStatus::NeedsInput);
        assert_eq!(ran.summary.confirmations, 1);
        assert!(ran.repair_calls.is_empty());
        assert_eq!(h.platform.executed().len(), usize::from(approve));
        assert!(!ran
            .events
            .iter()
            .any(|e| e["reason"] == "rejected_plan_repair"));
    }
}
#[tokio::test]
async fn repaired_legacy_empty_plan_cannot_bypass_completion_verification() {
    let h = Harness::new();
    let ran = h.run(json!({"turns":[rejected(),{}]})).await;
    assert_eq!(ran.summary.status, RunStatus::NeedsInput);
    assert!(h.platform.executed().is_empty());
}
#[tokio::test]
async fn step_cost_and_elapsed_wall_limits_prevent_repair_requests() {
    for which in 0..3 {
        let mut h = Harness::new();
        let mut bad = rejected();
        match which {
            0 => h.opts.limits.max_steps = 1,
            1 => {
                h.opts.limits.max_cost_usd = 0.5;
                bad["usage"]["cost_usd"] = json!(1.0);
            }
            _ => h.opts.limits.max_wall_time = Duration::ZERO,
        }
        let ran = h.run(json!({"turns":[bad,{}]})).await;
        assert_eq!(ran.summary.status, RunStatus::LimitHit);
        assert!(ran.repair_calls.is_empty());
        assert!(h.platform.executed().is_empty());
    }
}
#[tokio::test]
async fn cancellation_wins_before_polling_repair_provider() {
    let mut h = Harness::new();
    let token = h.cancel.clone();
    h.supervisor.on_recovering = Some(Box::new(move || token.cancel()));
    let ran = h
        .run(json!({"turns":[rejected(),{"actions":[click(3.,3.)]}]}))
        .await;
    assert_eq!(ran.summary.status, RunStatus::Aborted);
    assert!(ran.repair_calls.is_empty());
    assert!(h.platform.executed().is_empty());
}
#[tokio::test]
async fn pending_repair_is_bounded_by_remaining_wall_budget() {
    let mut h = Harness::new();
    h.opts.limits.max_wall_time = Duration::from_millis(100);
    let ran = h
        .run(json!({"repair_delay_ms":1000,"turns":[rejected(),{"actions":[click(3.,3.)]}]}))
        .await;
    assert_eq!(ran.summary.status, RunStatus::Error);
    assert!(ran.summary.message.as_ref().unwrap().contains("deadline"));
    assert_eq!(ran.repair_calls.len(), 1);
    assert!(h.platform.executed().is_empty());
}
#[tokio::test]
async fn malformed_provider_rejection_with_actions_is_never_executed() {
    let h = Harness::new();
    let mut bad = rejected();
    bad["actions"] = json!([click(3., 3.)]);
    let ran = h.run(json!({"turns":[bad]})).await;
    assert_eq!(ran.summary.status, RunStatus::Error);
    assert!(ran.repair_calls.is_empty());
    assert!(h.platform.executed().is_empty());
}
