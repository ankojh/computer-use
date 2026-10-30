//! Live integration tests against tools/CuTarget (HANDOFF.md §16, milestones M2/M3).
//!
//! These post real input events on this Mac. They are ignored by default and additionally require
//! `CU_LIVE_TESTS=1`:
//!
//! ```sh
//! CU_LIVE_TESTS=1 cargo test -p cu-mac --test live_cutarget -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Needs Accessibility + Screen Recording for the terminal running cargo. Don't touch the mouse or
//! keyboard while they run.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use cu_core::agent::{Agent, AgentOptions, ConfirmRequest, PauseChoice, RunStatus, Supervisor};
use cu_core::coords::{fit_size, CoordMapping};
use cu_core::platform::Platform;
use cu_core::policy::{Mode, Policy, PolicyConfig};
use cu_core::recorder::RunRecorder;
use cu_core::{Button, KeyCombo, Mods, PointP, RectP, SafetyCheck};
use cu_mac::{input, MacPlatform};
use cu_providers::mock::{MockProvider, Script};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

struct Target {
    child: Child,
    dir: PathBuf,
    layout: Value,
    seen: usize,
}

impl Drop for Target {
    fn drop(&mut self) {
        let _ = input::release_all();
        let _ = self.child.kill();
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

impl Target {
    fn launch() -> Target {
        let pkg = repo_root().join("tools/CuTarget");
        let status = Command::new("swift")
            .args(["build", "--package-path"])
            .arg(&pkg)
            .status()
            .unwrap();
        assert!(status.success(), "building CuTarget failed");
        let dir = std::env::temp_dir().join(format!("cutarget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let child = Command::new(pkg.join(".build/debug/CuTarget"))
            .env("CUTARGET_DIR", &dir)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let layout = loop {
            if let Ok(s) = std::fs::read_to_string(dir.join("layout.json")) {
                if let Ok(v) = serde_json::from_str::<Value>(&s) {
                    break v;
                }
            }
            assert!(Instant::now() < deadline, "CuTarget did not start");
            std::thread::sleep(Duration::from_millis(100));
        };
        std::thread::sleep(Duration::from_millis(700)); // let the window become key
        let mut t = Target {
            child,
            dir,
            layout,
            seen: 0,
        };
        t.drain();
        t
    }

    fn rect(&self, id: &str) -> RectP {
        serde_json::from_value(self.layout["targets"][id].clone())
            .unwrap_or_else(|_| panic!("no target {id}"))
    }

    fn all_events(&self) -> Vec<Value> {
        std::fs::read_to_string(self.dir.join("events.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    /// Events received since the last call (after a short settle).
    fn drain(&mut self) -> Vec<Value> {
        std::thread::sleep(Duration::from_millis(250));
        let all = self.all_events();
        let new = all[self.seen.min(all.len())..].to_vec();
        self.seen = all.len();
        new
    }
}

fn of_type<'a>(events: &'a [Value], ty: &str) -> Vec<&'a Value> {
    events.iter().filter(|e| e["type"] == ty).collect()
}

fn live() -> bool {
    if std::env::var("CU_LIVE_TESTS").as_deref() != Ok("1") {
        eprintln!("skipping: set CU_LIVE_TESTS=1 to post real input events");
        return false;
    }
    let p = cu_mac::permissions().unwrap();
    assert!(
        p.accessibility || p.post_events,
        "grant Accessibility to the terminal running the tests"
    );
    true
}

#[test]
#[ignore = "posts real input events; run with CU_LIVE_TESTS=1 -- --ignored"]
fn input_primitives_against_cutarget() {
    if !live() {
        return;
    }
    let mut t = Target::launch();

    // Click accuracy (≤ 1 pt) on every button size, all events tagged.
    for size in [8, 12, 16, 24, 32, 48, 64] {
        let id = format!("btn-{size}");
        let c = t.rect(&id).center();
        input::click(c, Button::Left, 1, Mods::NONE).unwrap();
        let ev = t.drain();
        let down = of_type(&ev, "leftMouseDown");
        assert_eq!(down.len(), 1, "{id}: {ev:?}");
        let d = down[0];
        assert_eq!(d["target"], id.as_str(), "{id}: {d}");
        assert!(
            (d["x"].as_f64().unwrap() - c.x).abs() <= 1.0
                && (d["y"].as_f64().unwrap() - c.y).abs() <= 1.0,
            "{id}: {d}"
        );
        assert_eq!(d["tagged"], true);
    }

    // Double / triple click counts.
    let big = t.rect("btn-64").center();
    for count in [2u8, 3] {
        input::click(big, Button::Left, count, Mods::NONE).unwrap();
        let ev = t.drain();
        let downs = of_type(&ev, "leftMouseDown");
        assert_eq!(downs.len(), count as usize);
        assert_eq!(downs.last().unwrap()["click_count"], count as i64, "{ev:?}");
    }

    // Modifiers and other buttons.
    input::click(big, Button::Left, 1, Mods::CMD | Mods::SHIFT).unwrap();
    let ev = t.drain();
    let mods = of_type(&ev, "leftMouseDown")[0]["mods"].clone();
    assert_eq!(mods, json!(["shift", "cmd"]), "{ev:?}");
    input::click(big, Button::Right, 1, Mods::NONE).unwrap();
    assert_eq!(of_type(&t.drain(), "rightMouseDown")[0]["button"], 1);
    input::click(big, Button::Middle, 1, Mods::NONE).unwrap();
    assert_eq!(of_type(&t.drain(), "otherMouseDown")[0]["button"], 2);

    // Unicode typing and newline handling.
    input::click(t.rect("text").center(), Button::Left, 1, Mods::NONE).unwrap();
    t.drain();
    let text = "héllo wörld 世界 🎉 ñ";
    input::type_text(text).unwrap();
    let ev = t.drain();
    let last = ev
        .iter()
        .rfind(|e| e["kind"] == "text")
        .expect("text events");
    assert_eq!(last["value"], text);
    input::type_text("\n").unwrap();
    assert!(
        of_type(&t.drain(), "keyDown")
            .iter()
            .any(|e| e["key_code"] == 36),
        "newline → Return"
    );

    // Key combos + repeat.
    input::key(&KeyCombo::parse("cmd+a").unwrap(), 1).unwrap();
    let ev = t.drain();
    let k = of_type(&ev, "keyDown");
    assert!(
        k.iter()
            .any(|e| e["chars_ignoring_mods"] == "a" && e["mods"] == json!(["cmd"])),
        "{ev:?}"
    );
    input::key(&KeyCombo::parse("Left").unwrap(), 3).unwrap();
    let lefts = of_type(&t.drain(), "keyDown")
        .iter()
        .filter(|e| e["key_code"] == 123)
        .count();
    assert_eq!(lefts, 3);

    // Scroll sign conventions: positive dy reveals content below (scroll position y grows).
    let sc = t.rect("scroll").center();
    for pixels in [false, true] {
        let before = t
            .all_events()
            .iter()
            .rfind(|e| e["kind"] == "scroll_position")
            .map(|e| e["y"].as_f64().unwrap())
            .unwrap_or(0.0);
        input::scroll(
            sc,
            0.0,
            if pixels { 120.0 } else { 3.0 },
            pixels,
            Mods::NONE,
        )
        .unwrap();
        let ev = t.drain();
        let after = ev
            .iter()
            .rfind(|e| e["kind"] == "scroll_position")
            .map(|e| e["y"].as_f64().unwrap());
        let after = after.unwrap_or_else(|| panic!("no scroll (pixels={pixels}): {ev:?}"));
        assert!(
            after > before,
            "positive dy should scroll down (pixels={pixels}): {before} → {after}"
        );
    }

    // Drag path delivery.
    let pad = t.rect("drag");
    let (a, b) = (
        PointP {
            x: pad.x + 20.0,
            y: pad.y + 20.0,
        },
        PointP {
            x: pad.x + pad.w - 20.0,
            y: pad.y + pad.h - 20.0,
        },
    );
    input::drag(&[a, b], Mods::NONE).unwrap();
    let ev = t.drain();
    assert_eq!(of_type(&ev, "leftMouseDown").len(), 1);
    assert!(of_type(&ev, "leftMouseDragged").len() >= 10);
    let up = of_type(&ev, "leftMouseUp")[0];
    assert!(
        (up["x"].as_f64().unwrap() - b.x).abs() <= 1.0
            && (up["y"].as_f64().unwrap() - b.y).abs() <= 1.0
    );
}

struct AutoYes;

impl Supervisor for AutoYes {
    fn confirm_action(&self, _req: &ConfirmRequest) -> bool {
        true
    }
    fn confirm_safety_checks(&self, _checks: &[SafetyCheck]) -> bool {
        false
    }
    fn human_input_paused(&self) -> PauseChoice {
        PauseChoice::Abort
    }
    fn notify(&self, m: &str) {
        eprintln!("» {m}");
    }
}

/// M3 acceptance: a scripted 10-action run executes in order, logs all spaces, and stops at the
/// first injected failure with NotExecuted for the rest.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "posts real input events; run with CU_LIVE_TESTS=1 -- --ignored"]
async fn scripted_mock_run_against_cutarget() {
    if !live() {
        return;
    }
    let mut t = Target::launch();
    let platform = MacPlatform::default();
    let opts = AgentOptions::default();
    let d = platform.display().unwrap();
    let (w, h) = fit_size(d.pixel_w, d.pixel_h, opts.observe.max_long_edge);
    let m = CoordMapping::full_display(d.bounds, w, h, d.scale);
    let i = |r: RectP| {
        let p = m.points_to_image(r.center());
        json!({"x": p.x, "y": p.y})
    };

    let script: Script = serde_json::from_value(json!({
        "turns": [{"actions": [
            {"type": "click", "at": i(t.rect("btn-64"))},
            {"type": "click", "at": i(t.rect("btn-48")), "count": 2},
            {"type": "click", "at": i(t.rect("text"))},
            {"type": "type", "text": "scripted"},
            {"type": "key", "combo": "Left", "repeat": 2},
            {"type": "scroll", "at": i(t.rect("scroll")), "delta": {"unit": "clicks", "dx": 0, "dy": 2}},
            {"type": "click", "at": {"x": w as f64 + 50.0, "y": 10}},
            {"type": "click", "at": i(t.rect("btn-32"))},
            {"type": "type", "text": "never"},
            {"type": "screenshot"}
        ]}],
        "final_text": "done"
    }))
    .unwrap();
    let mut provider = MockProvider::new(script);
    let received = provider.received.clone();
    let policy = Policy::new(Mode::Auto, PolicyConfig::default()).unwrap();
    let recorder = RunRecorder::create(&std::env::temp_dir().join("cu-live-runs")).unwrap();
    let summary = Agent {
        provider: &mut provider,
        platform: &platform,
        policy: &policy,
        supervisor: &AutoYes,
        recorder: &recorder,
        opts,
        cancel: CancellationToken::new(),
    }
    .run("click things in CuTarget")
    .await;
    eprintln!("run folder: {}", recorder.dir().display());

    assert_eq!(summary.status, RunStatus::Done, "{:?}", summary.message);
    let r = received.lock().unwrap();
    let kinds: Vec<&str> = r[0].outcomes.iter().map(|(_, k, _)| k.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "ok",
            "ok",
            "ok",
            "ok",
            "ok",
            "ok",
            "failed",
            "not_executed",
            "not_executed",
            "not_executed"
        ]
    );

    let ev = t.drain();
    let downs = of_type(&ev, "leftMouseDown");
    assert_eq!(downs.iter().filter(|e| e["target"] == "btn-64").count(), 1);
    assert_eq!(downs.iter().filter(|e| e["target"] == "btn-48").count(), 2);
    assert_eq!(
        downs.iter().filter(|e| e["target"] == "btn-32").count(),
        0,
        "not executed after the failure"
    );
    let last_text = ev.iter().rfind(|e| e["kind"] == "text").unwrap();
    assert_eq!(last_text["value"], "scripted");

    let log = std::fs::read_to_string(recorder.dir().join("events.jsonl")).unwrap();
    let first_end: Value = log
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|e| e["kind"] == "action_end")
        .unwrap();
    for space in ["i", "p", "d"] {
        assert!(
            first_end[space].is_object(),
            "action_end logs {space}: {first_end}"
        );
    }
}
