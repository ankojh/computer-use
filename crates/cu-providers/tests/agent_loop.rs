//! Agent loop with MockProvider + FakePlatform: ordering, stop-at-first-failure, NotExecuted results,
//! limits, pauses, policy, and release_all on abort (HANDOFF.md §16).

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use cu_core::agent::{
    Agent, AgentOptions, ConfirmRequest, PauseChoice, RunStatus, RunSummary, Supervisor,
};
use cu_core::policy::{Mode, Policy, PolicyConfig};
use cu_core::recorder::RunRecorder;
use cu_core::testing::FakePlatform;
use cu_core::{AxElement, RectP, ResolvedAction, SafetyCheck};
use cu_providers::mock::{MockProvider, Received, Script};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

struct TestSupervisor {
    approve: bool,
    approve_safety: bool,
    pause: PauseChoice,
    prompts: Mutex<Vec<String>>,
    pointer: Mutex<Vec<Option<cu_core::PointerFeedback>>>,
    progress: Mutex<Vec<cu_core::agent::TaskProgress>>,
    on_confirm: Option<Box<dyn Fn() + Send + Sync>>,
    on_safety: Option<Box<dyn Fn() + Send + Sync>>,
    on_recovering: Option<Box<dyn Fn() + Send + Sync>>,
}

impl TestSupervisor {
    fn new() -> Self {
        TestSupervisor {
            approve: true,
            approve_safety: true,
            pause: PauseChoice::Resume,
            prompts: Mutex::default(),
            pointer: Mutex::default(),
            progress: Mutex::default(),
            on_confirm: None,
            on_safety: None,
            on_recovering: None,
        }
    }
}

impl Supervisor for TestSupervisor {
    fn task_progress(&self, progress: cu_core::agent::TaskProgress) {
        if progress.stage == cu_core::agent::RunStage::Recovering {
            if let Some(callback) = &self.on_recovering {
                callback();
            }
        }
        self.progress.lock().unwrap().push(progress);
    }
    fn pointer_feedback(&self, feedback: Option<cu_core::PointerFeedback>) {
        self.pointer.lock().unwrap().push(feedback);
    }
    fn confirm_action(&self, req: &ConfirmRequest) -> bool {
        self.prompts
            .lock()
            .unwrap()
            .push(format!("{}: {}", req.reason, req.describe()));
        if let Some(callback) = &self.on_confirm {
            callback();
        }
        self.approve
    }
    fn confirm_safety_checks(&self, checks: &[SafetyCheck]) -> bool {
        self.prompts
            .lock()
            .unwrap()
            .push(format!("safety: {}", checks[0].id));
        if let Some(callback) = &self.on_safety {
            callback();
        }
        self.approve_safety
    }
    fn human_input_paused(&self) -> PauseChoice {
        self.prompts.lock().unwrap().push("paused".into());
        self.pause
    }
    fn notify(&self, _message: &str) {}
}

struct Harness {
    platform: Arc<FakePlatform>,
    supervisor: TestSupervisor,
    mode: Mode,
    opts: AgentOptions,
    cancel: CancellationToken,
}

struct Ran {
    summary: RunSummary,
    received: Vec<Received>,
    events: Vec<Value>,
    helper_calls: Vec<cu_core::types::AdvisorKind>,
    repair_calls: Vec<(String, u64)>,
}

impl Harness {
    fn new() -> Self {
        Harness {
            platform: Arc::new(FakePlatform::default()),
            supervisor: TestSupervisor::new(),
            mode: Mode::ConfirmRisky,
            opts: AgentOptions::default(),
            cancel: CancellationToken::new(),
        }
    }

    async fn run(&self, script: Value) -> Ran {
        let script: Script = serde_json::from_value(script).expect("valid script");
        let mut provider = MockProvider::new(script);
        let received = provider.received.clone();
        let helper_calls = provider.helper_calls.clone();
        let repair_calls = provider.repair_calls.clone();
        let policy = Policy::new(self.mode, PolicyConfig::default()).unwrap();
        let runs = std::env::temp_dir().join("cu-agent-tests");
        let recorder = RunRecorder::create(&runs).unwrap();
        let agent = Agent {
            provider: &mut provider,
            platform: self.platform.as_ref(),
            policy: &policy,
            supervisor: &self.supervisor,
            recorder: &recorder,
            opts: self.opts.clone(),
            cancel: self.cancel.clone(),
        };
        let summary = agent.run("test task").await;
        let events = std::fs::read_to_string(recorder.dir().join("events.jsonl"))
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert!(recorder.dir().join("summary.json").exists());
        std::fs::remove_dir_all(recorder.dir()).unwrap();
        let received = received.lock().unwrap().clone();
        let helper_calls = helper_calls.lock().unwrap().clone();
        let repair_calls = repair_calls.lock().unwrap().clone();
        Ran {
            summary,
            received,
            events,
            helper_calls,
            repair_calls,
        }
    }
}

fn click(x: f64, y: f64) -> Value {
    json!({"type": "click", "at": {"x": x, "y": y}})
}

#[path = "agent_loop/auto_mode.rs"]
mod auto_mode;
#[path = "agent_loop/plan_repair.rs"]
mod plan_repair;

fn kinds(r: &Received) -> Vec<&str> {
    r.outcomes.iter().map(|(_, k, _)| k.as_str()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn pointer_feedback_uses_resolved_points_and_drag_destination() {
    use cu_core::PointerPhase::{Active, Complete};
    let h = Harness::new();
    h.run(json!({"turns":[{"actions":[click(100.,150.),{"type":"drag","path":[{"x":200.,"y":200.},{"x":400.,"y":500.}]}]}]})).await;
    let events = h.supervisor.pointer.lock().unwrap();
    let points: Vec<_> = events.iter().flatten().collect();
    assert_eq!(
        points.iter().map(|e| e.phase).collect::<Vec<_>>(),
        vec![Active, Complete, Active, Complete]
    );
    let executed = h.platform.executed();
    assert_eq!(Some(points[0].point), executed[0].target());
    assert_eq!(Some(points[2].point), executed[1].target());
    assert_eq!(Some(points[3].point), executed[1].final_cursor());
}

#[tokio::test(flavor = "multi_thread")]
async fn declined_pointer_target_never_reports_execution() {
    let mut h = Harness::new();
    h.mode = Mode::ConfirmAll;
    h.supervisor.approve = false;
    h.run(json!({"turns":[{"actions":[click(100.,150.)]}]}))
        .await;
    let events = h.supervisor.pointer.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].unwrap().phase, cu_core::PointerPhase::Target);
    assert!(events[1].is_none());
    assert!(h.platform.executed().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn cu_overlay_hit_test_cannot_bypass_underlying_risky_action() {
    let h = Harness::new();
    h.platform.elements.lock().unwrap().push((
        RectP {
            x: 0.,
            y: 0.,
            w: 1000.,
            h: 1000.,
        },
        AxElement {
            role: Some("AXWindow".into()),
            app: Some(cu_core::AppInfo {
                pid: 42,
                name: Some("Cu".into()),
                bundle_id: Some("dev.ankojh.cu".into()),
            }),
            ..Default::default()
        },
    ));
    let ran = h
        .run(json!({"turns":[{"actions":[click(100.,150.)]}]}))
        .await;
    assert_eq!(ran.summary.actions_denied, 1);
    assert!(h.platform.executed().is_empty());
    assert!(h
        .supervisor
        .pointer
        .lock()
        .unwrap()
        .iter()
        .all(Option::is_none));
}

#[tokio::test(flavor = "multi_thread")]
async fn exact_local_focus_guard_prevents_wrong_field_typing_and_halts_batch() {
    for label in ["To", "Subject", "Subject to change", "", "subject"] {
        let h = Harness::new();
        *h.platform.focused.lock().unwrap() = Some(cu_core::AxSummary {
            element: Some(AxElement {
                role: Some("AXTextField".into()),
                description: Some(label.into()),
                ..Default::default()
            }),
            ..Default::default()
        });
        let ran = h.run(json!({"turns":[{"actions":[
            {"type":"type","text":"Synthetic subject","expected_focus":{"role":"AXTextField","label":"Subject"}},
            {"type":"type","text":"must not run after a mismatch"}
        ]}]})).await;
        if label.eq_ignore_ascii_case("subject") {
            assert_eq!(ran.summary.actions_executed, 2);
            assert_eq!(h.platform.executed().len(), 2);
        } else {
            assert_eq!(ran.summary.actions_executed, 0);
            assert_eq!(ran.summary.actions_failed, 1);
            assert!(h.platform.executed().is_empty());
            assert_eq!(kinds(&ran.received[0]), ["failed", "not_executed"]);
            assert!(ran.events.iter().any(|e| e["kind"] == "focus_guard"));
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn focus_guard_never_uses_value_or_help_as_field_identity() {
    let h = Harness::new();
    *h.platform.focused.lock().unwrap() = Some(cu_core::AxSummary {
        element: Some(AxElement {
            role: Some("AXTextField".into()),
            value: Some("Subject".into()),
            help: Some("Subject".into()),
            ..Default::default()
        }),
        ..Default::default()
    });
    let ran = h.run(json!({"turns":[{"actions":[{"type":"type","text":"Synthetic", "expected_focus":{"role":"AXTextField","label":"Subject"}}]}]})).await;
    assert_eq!(ran.summary.actions_failed, 1);
    assert!(h.platform.executed().is_empty());
}

fn helper_report() -> Value {
    json!({"id":"public-1","kind":"public_research","provider":"parallel","status":"ok","summary":"Public sources retrieved, not desktop effects.",
        "latency_ms":10,"requests_used":4,"data":{},"sources":[{"url":"https://doc.rust-lang.org/book/","title":"Rust","excerpts":["Public ownership documentation"]}],
        "usage":{"input_tokens":10,"output_tokens":0,"cache_read_tokens":0,"cache_write_tokens":0,"cost_usd":0.0}})
}
#[tokio::test(flavor = "multi_thread")]
async fn observe_and_declined_confirm_all_never_call_external_helpers() {
    for mode in [Mode::Observe, Mode::ConfirmAll] {
        let mut h = Harness::new();
        h.mode = mode;
        h.supervisor.approve_safety = false;
        let ran=h.run(json!({"turns":[{"actions":[{"type":"public_research"}],"advisor":helper_report()}]})).await;
        assert!(ran.helper_calls.is_empty());
        assert!(h.platform.executed().is_empty());
        if mode == Mode::ConfirmAll {
            assert_eq!(ran.summary.actions_denied, 1);
        }
    }
}
#[tokio::test(flavor = "multi_thread")]
async fn helper_approval_does_not_extend_wall_budget_or_override_stop() {
    for cancelled in [false, true] {
        let mut h = Harness::new();
        h.mode = Mode::ConfirmAll;
        if cancelled {
            let token = h.cancel.clone();
            h.supervisor.on_safety = Some(Box::new(move || token.cancel()));
        } else {
            h.opts.limits.max_wall_time = Duration::from_millis(200);
            h.supervisor.on_safety =
                Some(Box::new(|| std::thread::sleep(Duration::from_millis(250))));
        }
        let ran=h.run(json!({"turns":[{"actions":[{"type":"public_research"}],"advisor":helper_report()}]})).await;
        assert!(h
            .supervisor
            .prompts
            .lock()
            .unwrap()
            .iter()
            .any(|p| p.starts_with("safety:")));
        assert!(ran.helper_calls.is_empty());
        assert!(h.platform.executed().is_empty());
        assert_eq!(
            ran.summary.status,
            if cancelled {
                RunStatus::Aborted
            } else {
                RunStatus::LimitHit
            }
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn helper_batches_cannot_mix_research_with_desktop_input() {
    let h = Harness::new();
    let ran=h.run(json!({"turns":[{"actions":[{"type":"public_research"},click(100.,100.)],"advisor":helper_report()}]})).await;
    assert_eq!(ran.summary.status, RunStatus::Error);
    assert!(ran.helper_calls.is_empty());
    assert!(h.platform.executed().is_empty());
}
#[tokio::test(flavor = "multi_thread")]
async fn only_host_recorded_public_source_ids_can_support_research_completion() {
    for fake in [false, true] {
        let mut h = Harness::new();
        h.opts.limits.max_steps = 3;
        let mut verdict = review("verified", 1);
        verdict["checks"][0]["frames"] = json!([1]);
        verdict["checks"][0]["advisor_ids"] = json!([if fake { "fabricated" } else { "public-1" }]);
        let ran=h.run(json!({"turns":[
            {"actions":[{"type":"public_research"}],"advisor":helper_report()},
            {"stop":"verify_completion","text":"Public research claim","evidence_frames":[1],"verification":verdict}
        ]})).await;
        assert_eq!(ran.helper_calls.len(), 1);
        assert_eq!(ran.summary.actions_executed, 0);
        assert!(h.platform.executed().is_empty());
        assert_eq!(ran.summary.status == RunStatus::Done, !fake);
    }
}
#[tokio::test(flavor = "multi_thread")]
async fn positive_helper_judgment_does_not_authorize_send() {
    let mut h = Harness::new();
    h.supervisor.approve = false;
    h.platform.elements.lock().unwrap().push((
        RectP {
            x: 0.,
            y: 0.,
            w: 2000.,
            h: 2000.,
        },
        AxElement {
            role: Some("AXButton".into()),
            title: Some("Send".into()),
            ..Default::default()
        },
    ));
    let mut report = helper_report();
    report["kind"] = json!("ui_judgment");
    report["provider"] = json!("jev");
    report["data"] = json!({"focus":{"choice":"ready"}});
    let ran=h.run(json!({"turns":[{"actions":[{"type":"ui_judgment"}],"advisor":report},{"actions":[click(100.,100.)]}]})).await;
    assert_eq!(ran.helper_calls.len(), 1);
    assert_eq!(ran.summary.actions_denied, 1);
    assert_eq!(ran.summary.actions_executed, 0);
    assert!(h.platform.executed().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn jev_judgment_is_not_research_or_delivery_evidence() {
    let mut h = Harness::new();
    h.opts.limits.max_steps = 3;
    let mut report = helper_report();
    report["kind"] = json!("ui_judgment");
    report["provider"] = json!("jev");
    let mut verdict = review("verified", 1);
    verdict["checks"][0]["frames"] = json!([]);
    verdict["checks"][0]["advisor_ids"] = json!(["public-1"]);
    let ran=h.run(json!({"turns":[{"actions":[{"type":"ui_judgment"}],"advisor":report},
        {"stop":"verify_completion","text":"Claim only","evidence_frames":[1],"verification":verdict}]})).await;
    assert_ne!(ran.summary.status, RunStatus::Done);
    assert!(h.platform.executed().is_empty());
}

fn review(verdict: &str, frame: u64) -> Value {
    json!({"verdict":verdict,"summary":if verdict=="verified" {"Requested work verified."} else {"Missing evidence; inspect the profile menu."},
        "checks":[{"requirement":"Task outcome","verdict":verdict,"evidence":"Synthetic host observation.","frames":[frame]}],
        "model":"gpt-6-luna","reasoning":"medium","latency_ms":42,
        "usage":{"input_tokens":10,"output_tokens":5,"cache_read_tokens":0,"cache_write_tokens":0,"cost_usd":0.0}})
}

#[tokio::test(flavor = "multi_thread")]
async fn approved_action_is_not_transferable_to_changed_focus_or_target() {
    for change_focus in [true, false] {
        let mut h = Harness::new();
        h.mode = Mode::ConfirmAll;
        let platform = h.platform.clone();
        h.supervisor.on_confirm = Some(Box::new(move || {
            if change_focus {
                *platform.focused.lock().unwrap() = Some(cu_core::AxSummary {
                    app: Some(cu_core::AppInfo {
                        pid: 777,
                        name: Some("Different app".into()),
                        bundle_id: None,
                    }),
                    ..Default::default()
                });
            } else {
                platform.elements.lock().unwrap().push((
                    RectP {
                        x: 0.,
                        y: 0.,
                        w: 500.,
                        h: 500.,
                    },
                    AxElement {
                        title: Some("Changed target".into()),
                        ..Default::default()
                    },
                ));
            }
        }));
        let ran = h.run(json!({"turns":[{"actions":[click(100.,100.)]}, {"stop":"needs_input","text":"Target changed."}]})).await;
        assert_eq!(ran.summary.actions_executed, 0);
        assert_eq!(ran.summary.actions_denied, 1);
        assert!(h.platform.executed().is_empty());
        assert!(ran.events.iter().any(|e| e["kind"] == "action_end"
            && e["message"]
                .as_str()
                .unwrap_or("")
                .contains("changed during approval")));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn progress_reports_observed_stages_and_posted_inputs_not_percentages() {
    use cu_core::agent::RunStage;
    let h = Harness::new();
    let ran = h
        .run(json!({"turns":[{"actions":[click(100.,100.)]},
        {"stop":"verify_completion","evidence_frames":[2],"verification":review("verified",2)}]}))
        .await;
    let progress = h.supervisor.progress.lock().unwrap();
    assert_eq!(progress[0].stage, RunStage::Planning);
    assert_eq!(progress[0].actions_executed, 0);
    assert!(progress
        .iter()
        .any(|p| p.stage == RunStage::Executing && p.actions_executed == 1));
    let last = progress.last().unwrap();
    assert_eq!(last.stage, RunStage::Verifying);
    assert_eq!(last.step, 2);
    assert_eq!(last.actions_executed, ran.summary.actions_executed);
    assert_eq!(
        progress.len(),
        ran.events
            .iter()
            .filter(|e| e["kind"] == "task_progress")
            .count()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unverified_or_blocked_empty_actions_never_mean_success() {
    for stop in ["verify_completion", "needs_input"] {
        let h = Harness::new();
        let ran = h
            .run(json!({"turns":[{"stop":stop,"text":"Cannot verify CMU profile.","actions":[]}]}))
            .await;
        assert_eq!(ran.summary.status, RunStatus::NeedsInput);
        assert_eq!(ran.summary.final_text, ran.summary.message);
        assert!(h.platform.executed().is_empty());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn completion_requires_reasoned_evidence_and_a_fresh_screenshot() {
    let h = Harness::new();
    let ran=h.run(json!({"turns":[{"stop":"verify_completion","evidence_frames":[1],"verification":review("verified",1)}]})).await;
    assert_eq!(ran.summary.status, RunStatus::Done);
    assert_eq!(h.platform.observations.load(Ordering::SeqCst), 2);
    assert_eq!(ran.summary.usage.input_tokens, 10);
    let event = ran
        .events
        .iter()
        .find(|e| e["kind"] == "completion_verification")
        .unwrap();
    assert_eq!(event["reasoning"], "medium");
    assert_eq!(event["model"], "gpt-6-luna");
}

#[tokio::test(flavor = "multi_thread")]
async fn recoverable_profile_block_gets_one_bounded_replan() {
    let h = Harness::new();
    let ran=h.run(json!({"turns":[
        {"stop":"needs_input","text":"Switch profiles for me.","verification":review("incomplete",1)},
        {"actions":[click(100.,100.)]},
        {"stop":"verify_completion","evidence_frames":[3],"verification":review("verified",3)}
    ]})).await;
    assert_eq!(ran.summary.status, RunStatus::Done);
    assert_eq!(h.platform.executed().len(), 1);
    assert_eq!(
        ran.events
            .iter()
            .filter(|e| e["kind"] == "model_request" && e["reason"] == "completion_recovery")
            .count(),
        1
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_completion_recovery_cannot_loop_or_fall_back_to_legacy_done() {
    for next in [
        json!({"stop":"verify_completion","verification":review("incomplete",2)}),
        json!({"actions":[]}),
    ] {
        let h = Harness::new();
        let ran=h.run(json!({"turns":[{"stop":"verify_completion","verification":review("incomplete",1)},next,{"actions":[click(100.,100.)]}]})).await;
        assert_eq!(ran.summary.status, RunStatus::NeedsInput);
        assert!(h.platform.executed().is_empty());
        assert_eq!(
            ran.events
                .iter()
                .filter(|e| e["reason"] == "completion_recovery")
                .count(),
            1
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn unavailable_completion_evidence_is_not_accepted() {
    let h = Harness::new();
    let ran=h.run(json!({"turns":[{"stop":"verify_completion","evidence_frames":[999],"verification":review("verified",999)}]})).await;
    assert_eq!(ran.summary.status, RunStatus::NeedsInput);
    assert!(!ran
        .events
        .iter()
        .any(|e| e["kind"] == "completion_verification"));
}

#[tokio::test(flavor = "multi_thread")]
async fn completion_recovery_never_replays_an_approved_failed_or_denied_send() {
    for (approve, fail) in [(true, false), (true, true), (false, false)] {
        let mut h = Harness::new();
        h.supervisor.approve = approve;
        h.platform.elements.lock().unwrap().push((
            RectP {
                x: 0.,
                y: 0.,
                w: 1000.,
                h: 1000.,
            },
            AxElement {
                role: Some("AXButton".into()),
                title: Some("Send".into()),
                ..Default::default()
            },
        ));
        if fail {
            h.platform
                .fail_on
                .lock()
                .unwrap()
                .insert(0, "Synthetic uncertain click outcome".into());
        }
        let ran = h
            .run(json!({"turns":[{"actions":[click(100.,100.)]},
                {"stop":"verify_completion","verification":review("incomplete",2)},
                {"actions":[click(100.,100.)]}
            ]}))
            .await;
        assert_eq!(ran.summary.status, RunStatus::NeedsInput);
        assert_eq!(h.supervisor.prompts.lock().unwrap().len(), 1);
        assert!(!ran
            .events
            .iter()
            .any(|e| e["reason"] == "completion_recovery"));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn leaving_settings_does_not_prompt_in_confirm_risky_mode() {
    let h = Harness::new();
    *h.platform.focused.lock().unwrap() = Some(cu_core::AxSummary {
        app: Some(cu_core::AppInfo {
            pid: 42,
            name: Some("System Settings".into()),
            bundle_id: Some("com.apple.systempreferences".into()),
        }),
        ..Default::default()
    });
    let ran=h.run(json!({"turns":[{"actions":[{"type":"key","combo":"cmd+tab"},{"type":"key","combo":"cmd+space"}]}]})).await;
    assert_eq!(ran.summary.confirmations, 0);
    assert_eq!(h.platform.executed().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn model_route_is_logged_but_cannot_override_action_approval() {
    let mut h = Harness::new();
    h.supervisor.approve = false;
    let ran = h.run(json!({"turns":[{
        "route":{"model":"gpt-6-astra","reasoning":"high","tier":"complex","reason":"Synthetic route.",
            "router_model":"gpt-6-luna","router_reasoning":"low","router_latency_ms":500,"planner_latency_ms":2000},
        "actions":[{"type":"key","combo":"cmd+q"}]
    }]})).await;
    assert_eq!(ran.summary.confirmations, 1);
    assert_eq!(ran.summary.actions_denied, 1);
    assert!(h.platform.executed().is_empty());
    let response = ran
        .events
        .iter()
        .find(|e| e["kind"] == "model_response")
        .unwrap();
    assert_eq!(response["model"], "gpt-6-astra");
    assert_eq!(response["route"]["reasoning"], "high");
    assert_eq!(response["route"]["router_model"], "gpt-6-luna");
}

#[tokio::test(flavor = "multi_thread")]
async fn ten_actions_in_order_with_mapping() {
    let h = Harness::new();
    let ran = h
        .run(json!({
            "turns": [{
                "text": "doing ten things",
                "actions": [
                    click(720.0, 465.5),
                    {"type": "click", "at": {"x": 10, "y": 10}, "count": 2},
                    {"type": "click", "button": "right"},
                    {"type": "mouse_move", "at": {"x": 100, "y": 100}},
                    {"type": "drag", "path": [{"x": 0, "y": 0}, {"x": 1440, "y": 931}]},
                    {"type": "scroll", "at": {"x": 50, "y": 50}, "delta": {"unit": "pixels", "dx": 0, "dy": 120}},
                    {"type": "type", "text": "hello"},
                    {"type": "key", "combo": "cmd+a", "repeat": 2},
                    {"type": "cursor_position"},
                    {"type": "wait", "duration_ms": 1}
                ]
            }],
            "final_text": "finished"
        }))
        .await;

    assert_eq!(
        ran.summary.status,
        RunStatus::Done,
        "{:?}",
        ran.summary.message
    );
    assert_eq!(ran.summary.final_text.as_deref(), Some("finished"));
    assert_eq!(ran.summary.steps, 2);
    assert_eq!(ran.summary.actions_executed, 8);
    assert_eq!(ran.received.len(), 1);
    assert_eq!(kinds(&ran.received[0]), vec!["ok"; 10]);
    // cursor_position reports image space; the scroll left the cursor at its target.
    assert_eq!(ran.received[0].outcomes[8].2.as_deref(), Some("X=50, Y=50"));

    let executed = h.platform.executed();
    let ResolvedAction::Click {
        at: Some(p),
        count: 1,
        ..
    } = executed[0]
    else {
        panic!("{:?}", executed[0])
    };
    assert!(
        (p.x - 864.0).abs() < 1e-9 && (p.y - 558.5).abs() < 0.1,
        "{p:?}"
    );
    assert!(matches!(
        executed[1],
        ResolvedAction::Click { count: 2, .. }
    ));
    assert!(matches!(
        executed[2],
        ResolvedAction::Click { at: None, .. }
    ));
    let ResolvedAction::Drag { path, .. } = &executed[4] else {
        panic!()
    };
    assert!(
        path[1].x < 1728.0 && path[1].y < 1117.0,
        "edge coordinates stay on the display"
    );
    let ResolvedAction::Scroll {
        delta: cu_core::ScrollDelta::Pixels { dy, .. },
        ..
    } = executed[5]
    else {
        panic!()
    };
    assert!(
        (dy - 144.0).abs() < 0.1,
        "pixel deltas are scaled to points (1117/931): {dy}"
    );
    assert_eq!(executed.len(), 8);

    // Logged in all coordinate spaces.
    let first_end = ran
        .events
        .iter()
        .find(|e| e["kind"] == "action_end")
        .unwrap();
    assert_eq!(first_end["i"]["x"], 720.0);
    assert_eq!(first_end["p"]["x"], 864.0);
    assert_eq!(first_end["d"]["x"], 1728.0);
    // One settle per state-changing action.
    assert_eq!(h.platform.settles.load(Ordering::SeqCst), 8);
}

#[tokio::test(flavor = "multi_thread")]
async fn stops_at_first_failure() {
    let h = Harness::new();
    h.platform.fail_on.lock().unwrap().insert(2, "boom".into());
    let ran = h
        .run(json!({"turns": [{"actions": [
            click(1.0, 1.0), click(2.0, 2.0), click(3.0, 3.0), click(4.0, 4.0), click(5.0, 5.0)
        ]}]}))
        .await;
    assert_eq!(ran.summary.status, RunStatus::Done);
    assert_eq!(
        kinds(&ran.received[0]),
        vec!["ok", "ok", "failed", "not_executed", "not_executed"]
    );
    assert!(ran.received[0].outcomes[2]
        .2
        .as_deref()
        .unwrap()
        .contains("boom"));
    assert_eq!(h.platform.executed().len(), 2);
    assert_eq!(ran.summary.actions_failed, 1);
    // Every call gets a result, matched by call ref, in order.
    let refs: Vec<_> = ran.received[0]
        .outcomes
        .iter()
        .map(|(r, _, _)| r.as_str())
        .collect();
    assert_eq!(
        refs,
        vec!["mock-0-0", "mock-0-1", "mock-0-2", "mock-0-3", "mock-0-4"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn out_of_bounds_and_bad_input_fail_without_executing() {
    let h = Harness::new();
    let ran = h
        .run(json!({"turns": [
            {"actions": [click(5000.0, 10.0), click(1.0, 1.0)]},
            {"actions": [{"type": "click", "at": {"x": 1, "y": 1}, "count": 4}]},
            {"actions": [{"type": "drag", "path": [{"x": 1, "y": 1}]}]}
        ]}))
        .await;
    assert_eq!(kinds(&ran.received[0]), vec!["failed", "not_executed"]);
    assert!(ran.received[0].outcomes[0]
        .2
        .as_deref()
        .unwrap()
        .contains("outside"));
    assert_eq!(kinds(&ran.received[1]), vec!["failed"]);
    assert_eq!(kinds(&ran.received[2]), vec!["failed"]);
    assert!(h.platform.executed().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn screenshot_mid_batch_and_zoom() {
    let h = Harness::new();
    let ran = h
        .run(json!({"turns": [{"actions": [
            {"type": "zoom", "region": {"x0": 0, "y0": 0, "x1": 720, "y1": 465.5}},
            {"type": "screenshot"}
        ]}]}))
        .await;
    assert_eq!(kinds(&ran.received[0]), vec!["ok", "ok"]);
    // initial + mid-batch screenshot; the trailing screenshot is reused as the latest observation.
    assert_eq!(h.platform.observations.load(Ordering::SeqCst), 2);
    assert_eq!(ran.received[0].latest_frame_seq, 2);
    assert!(ran.events.iter().any(|e| e["kind"] == "zoom"));
}

#[tokio::test(flavor = "multi_thread")]
async fn display_change_invalidates_mapping() {
    let h = Harness::new();
    // The display is reconfigured right after the first click.
    *h.platform.bump_generation_after.lock().unwrap() = Some(0);
    let ran = h
        .run(json!({"turns": [
            {"actions": [click(1.0, 1.0), click(2.0, 2.0)]},
            {"actions": [click(3.0, 3.0)]}
        ]}))
        .await;
    assert_eq!(kinds(&ran.received[0]), vec!["ok", "failed"]);
    assert!(ran.received[0].outcomes[1]
        .2
        .as_deref()
        .unwrap()
        .contains("display configuration changed"));
    // The fresh observation after the batch carries the new generation, so the next click works.
    assert_eq!(kinds(&ran.received[1]), vec!["ok"]);
    assert_eq!(h.platform.executed().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn limits_stop_cleanly() {
    let mut h = Harness::new();
    h.opts.limits.max_steps = 2;
    let ran = h
        .run(json!({"turns": [
            {"actions": [click(1.0, 1.0)]}, {"actions": [click(1.0, 1.0)]}, {"actions": [click(1.0, 1.0)]}
        ]}))
        .await;
    assert_eq!(ran.summary.status, RunStatus::LimitHit);
    assert_eq!(h.platform.executed().len(), 2);
    assert!(h.platform.releases.load(Ordering::SeqCst) >= 1);
    assert!(ran.events.iter().any(|e| e["kind"] == "limit_hit"));
}

#[tokio::test(flavor = "multi_thread")]
async fn human_input_pauses_and_abort_releases() {
    let mut h = Harness::new();
    h.supervisor.pause = PauseChoice::Abort;
    h.platform.human_input_before.lock().unwrap().push(1);
    let ran = h
        .run(json!({"turns": [{"actions": [click(1.0, 1.0), click(2.0, 2.0), click(3.0, 3.0)]}]}))
        .await;
    assert_eq!(ran.summary.status, RunStatus::Aborted);
    assert_eq!(h.platform.executed().len(), 1, "paused within one action");
    assert!(
        h.platform.releases.load(Ordering::SeqCst) >= 2,
        "released on pause and on abort"
    );
    assert_eq!(
        *h.supervisor.prompts.lock().unwrap(),
        vec!["paused".to_string()]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn human_input_resume_continues() {
    let h = Harness::new();
    h.platform.human_input_before.lock().unwrap().push(1);
    let ran = h
        .run(json!({"turns": [{"actions": [click(1.0, 1.0), click(2.0, 2.0)]}]}))
        .await;
    assert_eq!(ran.summary.status, RunStatus::Done);
    assert_eq!(h.platform.executed().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn panic_hotkey_aborts() {
    let h = Harness::new();
    *h.platform.panic.lock().unwrap() = true;
    let ran = h
        .run(json!({"turns": [{"actions": [click(1.0, 1.0)]}]}))
        .await;
    assert_eq!(ran.summary.status, RunStatus::Aborted);
    assert!(h.platform.executed().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn observe_mode_posts_nothing() {
    let mut h = Harness::new();
    h.mode = Mode::Observe;
    let ran = h
        .run(json!({"turns": [{"actions": [click(1.0, 1.0), {"type": "type", "text": "x"}, {"type": "key", "combo": "cmd+q"}]}]}))
        .await;
    assert_eq!(ran.summary.status, RunStatus::Done);
    assert!(h.platform.executed().is_empty());
    assert_eq!(kinds(&ran.received[0]), vec!["ok", "ok", "ok"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn risky_actions_prompt_and_decline_halts_batch() {
    let mut h = Harness::new();
    h.supervisor.approve = false;
    h.platform.elements.lock().unwrap().push((
        RectP {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        },
        AxElement {
            role: Some("AXButton".into()),
            title: Some("Delete".into()),
            ..Default::default()
        },
    ));
    let ran = h
        .run(json!({"turns": [{"actions": [click(10.0, 10.0), click(1000.0, 800.0)]}]}))
        .await;
    assert_eq!(kinds(&ran.received[0]), vec!["denied", "not_executed"]);
    assert!(h.platform.executed().is_empty());
    let prompts = h.supervisor.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    assert!(prompts[0].contains("AXButton 'Delete'"), "{prompts:?}");
    assert_eq!(ran.summary.confirmations, 1);
    assert_eq!(ran.summary.actions_denied, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn risky_action_approved_runs() {
    let h = Harness::new();
    let ran = h
        .run(json!({"turns": [{"actions": [{"type": "key", "combo": "cmd+q"}]}]}))
        .await;
    assert_eq!(kinds(&ran.received[0]), vec!["ok"]);
    assert_eq!(h.platform.executed().len(), 1);
    assert_eq!(ran.summary.confirmations, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn safety_checks_need_approval() {
    let script = json!({"turns": [
        {"actions": [click(1.0, 1.0)], "safety_checks": [{"id": "sc_1", "code": "malicious_instructions", "message": "?"}]}
    ]});
    let h = Harness::new();
    let ran = h.run(script.clone()).await;
    assert_eq!(ran.received[0].acknowledged, vec!["sc_1".to_string()]);

    let mut h = Harness::new();
    h.supervisor.approve_safety = false;
    let ran = h.run(script).await;
    assert_eq!(ran.summary.status, RunStatus::SafetyDeclined);
    assert!(h.platform.executed().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn refusal_stops() {
    let h = Harness::new();
    let ran = h
        .run(json!({"turns": [{"text": "I can't help with that.", "refusal": true}]}))
        .await;
    assert_eq!(ran.summary.status, RunStatus::Refused);
    assert_eq!(
        ran.summary.message.as_deref(),
        Some("I can't help with that.")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cancellation_aborts_and_releases() {
    let h = Harness::new();
    h.cancel.cancel();
    let ran = h
        .run(json!({"turns": [{"actions": [click(1.0, 1.0)]}]}))
        .await;
    assert_eq!(ran.summary.status, RunStatus::Aborted);
    assert!(h.platform.executed().is_empty());
    assert!(h.platform.releases.load(Ordering::SeqCst) >= 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn long_waits_and_holds_are_capped() {
    let mut h = Harness::new();
    h.opts.limits.max_wait = std::time::Duration::from_millis(1000);
    let ran = h
        .run(json!({"turns": [{"actions": [
            {"type": "hold_key", "combo": "shift", "duration_ms": 60000},
        ]}]}))
        .await;
    assert_eq!(
        ran.received[0].outcomes[0].2.as_deref(),
        Some("OK (hold capped to 1s)")
    );
    assert!(matches!(
        h.platform.executed()[0],
        ResolvedAction::HoldKey {
            duration_ms: 1000,
            ..
        }
    ));
}
