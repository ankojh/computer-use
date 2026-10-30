//! Explicit opt-in diagnostic: only inert CuTarget squares, no capture/model/typing.
use anyhow::{ensure, Context};
use cu_core::platform::Platform;
use cu_core::policy::{Decision, Mode, Policy, PolicyContext};
use cu_core::{Button, Mods, PointP, RectP, ResolvedAction};
use cu_mac::MacPlatform;
use serde::Deserialize;
use serde_json::json;
use std::os::unix::fs::MetadataExt;
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Layout {
    pid: i32,
    window_content: RectP,
    targets: std::collections::HashMap<String, RectP>,
}
impl Layout {
    fn points(&self) -> anyhow::Result<Vec<PointP>> {
        ensure!(self.pid > 1, "invalid CuTarget PID");
        let bounds = self.window_content;
        ensure!(
            [bounds.x, bounds.y, bounds.w, bounds.h]
                .iter()
                .all(|v| v.is_finite())
                && bounds.w == 900.
                && bounds.h == 640.,
            "unexpected test window geometry"
        );
        [32, 48, 64]
            .iter()
            .map(|size| {
                let r = self
                    .targets
                    .get(&format!("btn-{size}"))
                    .context("missing inert test square")?;
                ensure!(
                    [r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite())
                        && r.w == *size as f64
                        && r.h == *size as f64
                        && r.x >= bounds.x
                        && r.y >= bounds.y
                        && r.x + r.w <= bounds.x + bounds.w
                        && r.y + r.h <= bounds.y + bounds.h,
                    "invalid inert target geometry"
                );
                Ok(r.center())
            })
            .collect()
    }
}
fn load(path: &Path) -> anyhow::Result<Layout> {
    let meta = std::fs::symlink_metadata(path)?;
    // SAFETY: geteuid has no arguments or borrowed pointers.
    ensure!(
        meta.is_file() && meta.len() <= 65536 && meta.uid() == unsafe { libc::geteuid() },
        "test layout must be a small, owned regular file"
    );
    let layout: Layout = serde_json::from_slice(&std::fs::read(path)?)?;
    layout.points()?;
    Ok(layout)
}

pub fn run(
    path: &Path,
    policy: cu_core::policy::PolicyConfig,
) -> anyhow::Result<std::process::ExitCode> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(run_inner(path, policy))
}
async fn run_inner(
    path: &Path,
    policy: cu_core::policy::PolicyConfig,
) -> anyhow::Result<std::process::ExitCode> {
    let layout = load(path)?;
    let points = layout.points()?;
    let platform = MacPlatform::default();
    struct Release<'a>(&'a MacPlatform);
    impl Drop for Release<'_> {
        fn drop(&mut self) {
            let _ = self.0.release_all();
        }
    }
    let _release = Release(&platform);
    let policy = Policy::new(Mode::ConfirmRisky, policy)?;
    let cancel = tokio_util::sync::CancellationToken::new();
    let c = cancel.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        c.cancel();
    });
    platform.reset_human_input_baseline();
    let start = Instant::now();
    let mut executed = 0;
    for (i, point) in points.into_iter().enumerate() {
        let click = ResolvedAction::Click {
            at: Some(point),
            button: if i == 2 { Button::Right } else { Button::Left },
            count: if i == 1 { 2 } else { 1 },
            mods: Mods::NONE,
        };
        let mut actions = vec![ResolvedAction::MouseMove {
            at: point,
            mods: Mods::NONE,
        }];
        if i == 0 {
            actions.push(ResolvedAction::MouseMove {
                at: point,
                mods: Mods::NONE,
            });
        } // Repeated position must still pulse.
        actions.push(click);
        for action in actions {
            ensure!(
                !cancel.is_cancelled()
                    && start.elapsed() < Duration::from_secs(8)
                    && !platform.panic_requested()?,
                "pointer test interrupted"
            );
            ensure!(
                !platform.human_input_since(start)?,
                "human pointer movement interrupted the test"
            );
            let current = load(path)?;
            ensure!(
                current.pid == layout.pid
                    && current.points()? == layout.points()?
                    && current.window_content == layout.window_content,
                "test window changed"
            );
            let focus = platform.focused()?.context("test focus unavailable")?;
            ensure!(
                focus
                    .app
                    .as_ref()
                    .is_some_and(|a| a.pid == layout.pid && a.name.as_deref() == Some("CuTarget"))
                    && focus.window_title.as_deref() == Some("CuTarget"),
                "CuTarget must remain the frontmost test window"
            );
            let element = platform
                .element_at(point)?
                .context("test target AX identity unavailable")?;
            ensure!(
                element.app.as_ref().is_some_and(|a| a.pid == layout.pid),
                "test point no longer belongs to CuTarget"
            );
            let (decision, _) = policy.decide(
                &action,
                &PolicyContext {
                    element: Some(&element),
                    focused: Some(&focus),
                    own_terminal_pid: None,
                },
            );
            ensure!(
                matches!(decision, Decision::Allow),
                "policy did not allow inert test input"
            );
            platform.execute_with_feedback(&action, &|feedback| {
                super::supervisor::emit(json!({"type":"cursor","cursor":feedback}))
            })?;
            executed += 1;
            super::supervisor::emit(
                json!({"type":"task_progress","progress":{"stage":"executing","step":1,"actions_executed":executed}}),
            );
            // Test visibility spacing only. Production input gets no new sleeps.
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(300)) => {},
                _ = cancel.cancelled() => anyhow::bail!("pointer test interrupted"),
            }
        }
    }
    super::supervisor::emit(
        json!({"type":"done","summary":{"status":"done","message":"Only inert CuTarget move/click checks; no capture, model, or keyboard input.","steps":1,"actions_executed":executed,"wall_ms":start.elapsed().as_millis() as u64}}),
    );
    Ok(std::process::ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layout() -> Layout {
        serde_json::from_value(json!({"pid":42,"window_content":{"x":100,"y":200,"w":900,"h":640},"targets":{
            "btn-32":{"x":120,"y":240,"w":32,"h":32},"btn-48":{"x":220,"y":240,"w":48,"h":48},"btn-64":{"x":320,"y":240,"w":64,"h":64}}})).unwrap()
    }
    #[test]
    fn bounded_inert_targets_only() {
        assert_eq!(layout().points().unwrap().len(), 3);
        let mut l = layout();
        l.targets.get_mut("btn-64").unwrap().x = 2000.;
        assert!(l.points().is_err());
        let mut l = layout();
        l.targets.get_mut("btn-32").unwrap().w = 0.;
        assert!(l.points().is_err());
        let mut l = layout();
        l.targets.remove("btn-48");
        assert!(l.points().is_err());
        let mut l = layout();
        l.pid = 0;
        assert!(l.points().is_err());
    }
}
