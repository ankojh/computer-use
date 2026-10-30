//! Real stdin/stdout protocol against a deterministic fake app-server. No
//! network, screenshots of the user, or physical input. Python is a test fixture.
use super::*;
use cu_core::config::Reasoning;
use cu_core::platform::{ObserveOpts, Platform};

const SERVER: &str = r#"
import json, sys
trace, judgments = sys.argv[1], json.loads(sys.argv[2])
script = json.loads(sys.argv[3])
threads, count, turns = {}, 0, 0
for line in sys.stdin:
    request=json.loads(line)
    if 'method' not in request: continue
    method, params, rid=request['method'],request.get('params',{}),request.get('id')
    with open(trace,'a') as out: out.write(json.dumps(request)+'\n')
    if method=='account/read':
        result=script['accounts'].pop(0)
        if 'error' in result:
            print(json.dumps({'id':rid,'error':result['error']}),flush=True)
            continue
    elif method=='thread/start':
        count+=1
        thread=('router' if 'routing classifier' in params['baseInstructions'] else 'verifier' if 'completion verifier' in params['baseInstructions'] else 'planner')+str(count)
        threads[thread]=params
        result={'thread':{'id':thread}}
    elif method=='thread/unsubscribe': result={'status':'unsubscribed'}
    elif method=='turn/start':
        turns+=1
        thread=params['threadId']; turn='turn'+str(turns)
        print(json.dumps({'id':rid,'result':{'turn':{'id':turn}}}),flush=True)
        if thread.startswith('router'):
            answer=judgments.pop(0)
            if answer=='"TOOL"':
                print(json.dumps({'id':900,'method':'item/tool/call','params':{}}),flush=True)
                continue
        elif thread.startswith('verifier'): answer=json.dumps(script['reviews'].pop(0))
        else: answer=json.dumps(script['plans'].pop(0) if script['plans'] else {'status':'needs_input','message':'Synthetic response; no input.','actions':[],'evidence_frames':[]})
        print(json.dumps({'method':'thread/tokenUsage/updated','params':{'threadId':thread,'turnId':turn,'tokenUsage':{'last':{'inputTokens':10,'outputTokens':5,'cachedInputTokens':2}}}}),flush=True)
        print(json.dumps({'method':'item/completed','params':{'threadId':thread,'turnId':turn,'item':{'type':'agentMessage','text':answer}}}),flush=True)
        print(json.dumps({'method':'turn/completed','params':{'threadId':thread,'turn':{'id':turn,'status':'completed'}}}),flush=True)
        continue
    else: result={}
    print(json.dumps({'id':rid,'result':result}),flush=True)
"#;

fn fixture(config: CodexConfig, judgments: Vec<Value>) -> CodexProvider {
    fixture_script(config, judgments, json!({"plans":[],"reviews":[]}))
}
fn fixture_script(config: CodexConfig, judgments: Vec<Value>, script: Value) -> CodexProvider {
    let mut provider = CodexProvider::with_config(config).unwrap();
    let mut child = Command::new("/usr/bin/python3")
        .args(["-u", "-c", SERVER])
        .arg(provider.cwd.join("trace.jsonl"))
        .arg(json!(judgments.iter().map(Value::to_string).collect::<Vec<_>>()).to_string())
        .arg(script.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    provider.connection = Some(Connection {
        stdin: child.stdin.take().unwrap(),
        lines: BufReader::new(child.stdout.take().unwrap()).lines(),
        child,
        next_id: 0,
    });
    provider.catalog = Some(Catalog::from_pages(["gpt-6-luna", "gpt-6-astra"].iter().map(|name| json!({
        "id":name,"model":name,"inputModalities":["text","image"],
        "supportedReasoningEfforts":[{"reasoningEffort":"low"},{"reasoningEffort":"medium"},{"reasoningEffort":"high"}]
    })).collect()).unwrap());
    provider
}
fn observation() -> Observation {
    cu_core::testing::FakePlatform::default()
        .observe(&ObserveOpts::default())
        .unwrap()
}
fn trace(provider: &CodexProvider) -> Vec<Value> {
    std::fs::read_to_string(provider.cwd.join("trace.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn judgment(tier: &str) -> Value {
    json!({"tier":tier,"reason":"Synthetic next-step assessment."})
}

#[tokio::test]
async fn effective_auto_mode_is_labelled_for_planner_and_independent_verifier() {
    let cfg = CodexConfig {
        routing: RoutingMode::Fixed,
        model: Some("gpt-6-luna".into()),
        reasoning: Reasoning::Medium,
        ..Default::default()
    };
    let mut p = fixture_script(
        cfg,
        vec![],
        json!({"plans":[{"status":"needs_input","message":"Synthetic ambiguity","actions":[],"evidence_frames":[1]}],"reviews":[{"verdict":"incomplete","summary":"Synthetic missing work","checks":[{"requirement":"Inspect","verdict":"incomplete","evidence":"Synthetic frame only","frames":[1],"advisor_ids":[]}]}]}),
    );
    p.set_control_mode(cu_core::policy::Mode::Auto);
    let obs = observation();
    let claim = p.start("Synthetic task", &obs).await.unwrap();
    p.verify_completion(&claim, &obs, &[], &[]).await.unwrap();
    let requests = trace(&p);
    for turn in requests.iter().filter(|e| e["method"] == "turn/start") {
        let text = turn["params"]["input"][0]["text"].as_str().unwrap();
        assert!(text.contains("auto / Allow all actions"));
        assert!(text.contains("without asking for separate Send/delete/pay/command permission"));
        assert!(text.contains("replaying an attempted consequential action"));
    }
}

#[tokio::test]
async fn invalid_plan_preserves_accounting_and_repair_is_explicit_not_automatic() {
    let cfg = CodexConfig {
        routing: RoutingMode::Fixed,
        model: Some("gpt-6-luna".into()),
        reasoning: Reasoning::Medium,
        ..Default::default()
    };
    let mut p = fixture_script(
        cfg,
        vec![],
        json!({"plans":[
        {"status":"continue","message":"PRIVATE_REPLY_CANARY","evidence_frames":[],"actions":[{"type":"key","key":"cmd+space"},{"type":"click","x":1,"y":1,"expected_focus":{"role":"AXTextField","label":"PRIVATE_LABEL_CANARY"}}]},
        {"status":"continue","message":"Inspect first","evidence_frames":[],"actions":[{"type":"screenshot"}]}
    ],"reviews":[]}),
    );
    p.native_apps = vec![cu_core::types::NativeAppInfo {
        name: "WhatsApp".into(),
        bundle_id: "net.whatsapp.WhatsApp".into(),
    }];
    let bad = p
        .start("Synthetic app discovery", &observation())
        .await
        .unwrap();
    assert_eq!(bad.stop, cu_core::StopKind::InvalidPlan);
    assert!(bad.actions.is_empty());
    assert_eq!(bad.usage.input_tokens, 10);
    assert_eq!(bad.usage.output_tokens, 5);
    assert_eq!(bad.route.as_ref().unwrap().reasoning, "medium");
    assert!(!format!("{bad:?}").contains("CANARY"));
    assert_eq!(
        trace(&p)
            .iter()
            .filter(|e| e["method"] == "turn/start")
            .count(),
        1
    );
    let repaired = p
        .replan_after_rejection(bad.plan_rejection.as_ref().unwrap(), &observation())
        .await
        .unwrap();
    assert_eq!(repaired.stop, cu_core::StopKind::Continue);
    let requests = trace(&p);
    let starts: Vec<_> = requests
        .iter()
        .filter(|e| e["method"] == "turn/start")
        .collect();
    assert_eq!(starts.len(), 2);
    let text = starts[1]["params"]["input"][0]["text"].as_str().unwrap();
    assert!(text.contains("NO actions"));
    assert!(text.contains("net.whatsapp.WhatsApp"));
    assert!(!text.contains("CANARY"));
    assert_eq!(
        starts[0]["params"]["threadId"],
        starts[1]["params"]["threadId"]
    );
    assert!(requests
        .iter()
        .filter(|e| e["method"] == "thread/start")
        .all(|e| e["params"]["baseInstructions"]
            .as_str()
            .unwrap()
            .contains("expected_focus must be null")));
}
#[tokio::test]
async fn web_login_review_gets_native_inventory_and_requests_native_discovery() {
    let cfg = CodexConfig {
        routing: RoutingMode::Fixed,
        model: Some("gpt-6-luna".into()),
        reasoning: Reasoning::Medium,
        ..Default::default()
    };
    let mut p = fixture_script(
        cfg,
        vec![],
        json!({"plans":[
        {"status":"needs_input","message":"Web requires login","actions":[],"evidence_frames":[1]},
        {"status":"continue","message":"Inspect native app","actions":[{"type":"key","key":"cmd+space"}],"evidence_frames":[]}
    ],"reviews":[{"verdict":"incomplete","summary":"Native app remains untried","checks":[{"requirement":"Inspect native route","verdict":"incomplete","evidence":"Only web login was inspected","frames":[1],"advisor_ids":[]}]}]}),
    );
    p.native_apps = vec![cu_core::types::NativeAppInfo {
        name: "WhatsApp".into(),
        bundle_id: "net.whatsapp.WhatsApp".into(),
    }];
    let obs = observation();
    let claim = p
        .start(
            "Use WhatsApp, no real messaging in this synthetic test",
            &obs,
        )
        .await
        .unwrap();
    let review = p
        .verify_completion(&claim, &obs, &[], &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(review.verdict, cu_core::CompletionVerdict::Incomplete);
    let turn = p.replan_after_review(&review, &obs).await.unwrap();
    assert_eq!(turn.actions.len(), 1);
    let requests = trace(&p);
    let verifier = requests
        .iter()
        .find(|e| {
            e["method"] == "turn/start"
                && e["params"]["threadId"]
                    .as_str()
                    .unwrap()
                    .starts_with("verifier")
        })
        .unwrap();
    let text = verifier["params"]["input"][0]["text"].as_str().unwrap();
    assert!(text.contains("installed_native_apps"));
    assert!(text.contains("net.whatsapp.WhatsApp"));
    assert!(text.contains("untried native app route"));
    assert!(text.contains("explicitly required the browser"));
    let last = requests
        .iter()
        .rfind(|e| e["method"] == "turn/start")
        .unwrap();
    assert!(last["params"]["input"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Check the installed native app candidates"));
}

#[tokio::test]
async fn authentication_rejection_refreshes_once_without_a_model_turn() {
    let mut p = fixture_script(
        CodexConfig::default(),
        vec![],
        json!({"accounts":[
            {"error":{"code":-32603,"message":"workspace routing discovery unauthorized (401)"}},
            {"account":{"type":"chatgpt"}}
        ]}),
    );
    p.connection
        .as_mut()
        .unwrap()
        .subscription_account()
        .await
        .unwrap();
    let requests = trace(&p);
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["params"]["refreshToken"], false);
    assert_eq!(requests[1]["params"]["refreshToken"], true);
    assert!(requests.iter().all(|r| r["method"] == "account/read"));
}

#[tokio::test]
async fn failed_refresh_is_bounded_and_requires_chatgpt_sign_in() {
    for second in [
        json!({"account":null,"requiresOpenaiAuth":true}),
        json!({"error":{"code":401,"message":"unauthorized"}}),
    ] {
        let mut p = fixture_script(
            CodexConfig::default(),
            vec![],
            json!({"accounts":[
                {"error":{"code":401,"message":"unauthorized"}}, second
            ]}),
        );
        let error = p
            .connection
            .as_mut()
            .unwrap()
            .subscription_account()
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("codex login"));
        assert_eq!(trace(&p).len(), 2);
    }
}

#[tokio::test]
async fn non_auth_errors_and_api_key_accounts_do_not_trigger_refresh_or_billing_fallback() {
    for account in [
        json!({"error":{"code":500,"message":"service unavailable"}}),
        json!({"account":{"type":"apiKey"}}),
    ] {
        let mut p = fixture_script(
            CodexConfig::default(),
            vec![],
            json!({"accounts":[account]}),
        );
        assert!(p
            .connection
            .as_mut()
            .unwrap()
            .subscription_account()
            .await
            .is_err());
        assert_eq!(trace(&p).len(), 1);
    }
}

#[tokio::test]
async fn reclassifies_every_turn_and_preserves_planner_history_with_explicit_overrides() {
    let mut p = fixture(
        CodexConfig::default(),
        vec![
            judgment("routine"),
            judgment("deliberate"),
            judgment("routine"),
            judgment("routine"),
        ],
    );
    let obs = observation();
    for (index, (model, reasoning)) in [
        ("gpt-6-luna", "low"),
        ("gpt-6-astra", "medium"),
        ("gpt-6-luna", "low"),
        ("gpt-6-astra", "high"),
    ]
    .iter()
    .enumerate()
    {
        let turn = if index == 0 {
            p.start("Use CMU profile, never send without verification.", &obs)
                .await
                .unwrap()
        } else {
            p.continue_with(
                if index == 3 {
                    vec![(
                        "x".into(),
                        ActionOutcome::Failed("Synthetic failure".into()),
                    )]
                } else {
                    vec![]
                },
                &obs,
                vec![],
            )
            .await
            .unwrap()
        };
        let route = turn.route.unwrap();
        assert_eq!(&route.model, model);
        assert_eq!(&route.reasoning, reasoning);
        assert_eq!(route.router_model.as_deref(), Some("gpt-6-luna"));
        assert_eq!(turn.usage.input_tokens, 20);
        assert_eq!(turn.usage.output_tokens, 10);
    }
    let trace = trace(&p);
    let starts: Vec<_> = trace
        .iter()
        .filter(|r| r["method"] == "thread/start")
        .collect();
    assert_eq!(starts.len(), 5); // four fresh routers, ONE persistent planner
    assert_eq!(
        trace
            .iter()
            .filter(|r| r["method"] == "thread/unsubscribe")
            .count(),
        4
    );
    let plans: Vec<_> = trace
        .iter()
        .filter(|r| {
            r["method"] == "turn/start"
                && r["params"]["threadId"]
                    .as_str()
                    .unwrap()
                    .starts_with("planner")
        })
        .collect();
    assert_eq!(plans.len(), 4);
    for plan in &plans {
        assert_eq!(plan["params"]["threadId"], plans[0]["params"]["threadId"]);
    }
    assert_eq!(plans[1]["params"]["model"], "gpt-6-astra");
    assert_eq!(plans[1]["params"]["effort"], "medium");
    assert_eq!(plans[2]["params"]["model"], "gpt-6-luna");
    assert_eq!(plans[2]["params"]["effort"], "low");
    assert!(plans[3]["params"]["input"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Use CMU profile"));
    assert!(p.recent.len() <= 3);
    for start in starts {
        assert_eq!(start["params"]["approvalPolicy"], "never");
        assert_eq!(start["params"]["sandbox"], "read-only");
        assert_eq!(start["params"]["config"]["features.multi_agent"], false);
    }
}

#[tokio::test]
async fn invalid_router_output_stops_before_any_planner_call() {
    let mut p = fixture(
        CodexConfig::default(),
        vec![json!({"tier":"routine","reason":"x","approved":true})],
    );
    assert!(p.start("Task", &observation()).await.is_err());
    assert!(p.thread_id.is_empty());
    assert_eq!(
        trace(&p)
            .iter()
            .filter(|r| r["method"] == "turn/start")
            .count(),
        1
    );
}

#[tokio::test]
async fn fixed_mode_skips_classifier_and_uses_explicit_effort() {
    let mut p = fixture(
        CodexConfig {
            routing: RoutingMode::Fixed,
            model: Some("gpt-6-astra".into()),
            reasoning: Reasoning::High,
            ..Default::default()
        },
        vec![],
    );
    let turn = p.start("Task", &observation()).await.unwrap();
    let route = turn.route.unwrap();
    assert_eq!(route.tier, "fixed");
    assert_eq!(route.reasoning, "high");
    assert!(route.router_model.is_none());
    assert_eq!(turn.usage.input_tokens, 10);
    assert_eq!(
        trace(&p)
            .iter()
            .filter(|r| r["method"] == "turn/start")
            .count(),
        1
    );
}

#[tokio::test]
async fn router_cannot_request_tools_or_bypass_the_policy() {
    let mut p = fixture(CodexConfig::default(), vec![json!("TOOL")]);
    let error = p.start("Task", &observation()).await.unwrap_err();
    assert!(error.to_string().contains("refusing to bypass"));
    assert!(p.thread_id.is_empty());
}

#[tokio::test]
async fn completion_uses_fresh_tool_free_context_and_medium_reasoning_without_router() {
    let cfg = CodexConfig {
        routing: RoutingMode::Fixed,
        model: Some("gpt-6-luna".into()),
        reasoning: Reasoning::Low,
        ..Default::default()
    };
    let wire = json!({"verdict":"verified","summary":"Observed task outcome.","checks":[{"requirement":"Outcome","verdict":"verified","evidence":"Host screenshots and successful action results.","frames":[1,2]}]});
    let mut p = fixture_script(
        cfg,
        vec![],
        json!({"plans":[{"status":"complete","message":"Claim only","actions":[],"evidence_frames":[1]}],"reviews":[wire]}),
    );
    let obs = observation();
    let claim = p.start("Task", &obs).await.unwrap();
    assert_eq!(claim.stop, cu_core::StopKind::VerifyCompletion);
    let mut latest = obs.clone();
    latest.frame_seq = 2;
    let report = p
        .verify_completion(
            &claim,
            &latest,
            &[obs],
            &[json!({"outcome":"ok","element":"AXButton 'Send'","approved":true})],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(report.model, "gpt-6-luna");
    assert_eq!(report.reasoning, "medium");
    let trace = trace(&p);
    let turns: Vec<_> = trace
        .iter()
        .filter(|r| r["method"] == "turn/start")
        .collect();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0]["params"]["effort"], "low");
    assert_eq!(turns[1]["params"]["effort"], "medium");
    assert_ne!(
        turns[0]["params"]["threadId"],
        turns[1]["params"]["threadId"]
    );
    assert_eq!(turns[1]["params"]["input"].as_array().unwrap().len(), 4);
    let context = turns[1]["params"]["input"][0]["text"].as_str().unwrap();
    assert!(context.contains("actual_action_outcomes"));
    assert!(context.contains("AXButton 'Send'"));
    assert_eq!(
        trace
            .iter()
            .filter(|r| r["method"] == "thread/unsubscribe")
            .count(),
        1
    );
    for start in trace.iter().filter(|r| r["method"] == "thread/start") {
        assert_eq!(start["params"]["sandbox"], "read-only");
        assert_eq!(start["params"]["config"]["features.shell_tool"], false);
    }
}

#[tokio::test]
async fn completion_verifier_cannot_return_executable_actions() {
    let cfg = CodexConfig {
        routing: RoutingMode::Fixed,
        model: Some("gpt-6-luna".into()),
        ..Default::default()
    };
    let wire = json!({"verdict":"verified","summary":"x","checks":[{"requirement":"x","verdict":"verified","evidence":"x","frames":[1]}],"actions":[{"type":"click","x":10,"y":20}]});
    let mut p = fixture_script(cfg, vec![], json!({"plans":[],"reviews":[wire]}));
    let obs = observation();
    let claim = p.start("Task", &obs).await.unwrap();
    assert!(p.verify_completion(&claim, &obs, &[], &[]).await.is_err());
}

#[tokio::test]
async fn pinned_planner_model_does_not_pin_router_or_effort() {
    let mut p = fixture(
        CodexConfig {
            model: Some("gpt-6-astra".into()),
            ..Default::default()
        },
        vec![judgment("routine"), judgment("complex")],
    );
    let obs = observation();
    let first = p.start("Task", &obs).await.unwrap().route.unwrap();
    let second = p
        .continue_with(vec![], &obs, vec![])
        .await
        .unwrap()
        .route
        .unwrap();
    assert_eq!(first.model, "gpt-6-astra");
    assert_eq!(first.reasoning, "low");
    assert_eq!(second.model, "gpt-6-astra");
    assert_eq!(second.reasoning, "high");
    assert_eq!(first.router_model.as_deref(), Some("gpt-6-luna"));
}
