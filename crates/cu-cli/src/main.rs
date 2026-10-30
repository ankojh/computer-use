//! `cu` — native macOS computer-use agent.

mod mobile;
mod pointer_test;
mod supervisor;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use cu_core::agent::{Agent, AgentLimits, AgentOptions, RunStatus};
use cu_core::config::{Config, Reasoning, RoutingMode};
use cu_core::coords::{fit_size, CoordMapping};
use cu_core::platform::{Platform, SettleOpts};
use cu_core::policy::{Mode, Policy};
use cu_core::recorder::RunRecorder;
use cu_core::{Button, ImageFormat, KeyCombo, Mods, PointI, PointP, Provider};
use cu_mac::{input, MacPlatform};
use cu_providers::{CodexProvider, MockProvider, Script};
use serde_json::json;
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(name = "cu", about = "Native macOS computer-use agent")]
struct Cli {
    /// Config file (default: ~/.config/cu/config.toml).
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Live Codex subscription/catalog readiness; no model calls, capture, or input.
    CodexStatus,
    /// Opt-in inert CuTarget move/click diagnostic; no capture, model, or typing.
    #[command(hide = true)]
    PointerTest {
        #[arg(long)]
        layout: PathBuf,
    },
    /// Check permissions, display, capture, and API keys.
    Doctor {
        /// Trigger the system permission prompts for anything missing.
        #[arg(long)]
        request: bool,
        /// Open System Settings at the pane for each missing permission.
        #[arg(long)]
        open: bool,
        /// Move the cursor 1 pt and back to verify event posting.
        #[arg(long)]
        input_test: bool,
        /// Machine-readable readiness for the mobile UI.
        #[arg(long)]
        json: bool,
    },
    /// Capture the main display.
    Screenshot {
        #[arg(long, default_value = "screenshot.jpg")]
        out: PathBuf,
        /// Max long edge in pixels (0 = native).
        #[arg(long, default_value_t = 1440)]
        max: u32,
        #[arg(long)]
        png: bool,
        #[arg(long, default_value_t = 0.78)]
        quality: f32,
    },
    /// Main display bounds, scale, pixels, generation.
    Display,
    /// Cursor position in global points.
    Cursor,
    /// Click at a point.
    Click {
        x: f64,
        y: f64,
        #[arg(long, value_enum, default_value_t = Space::Points)]
        space: Space,
        /// Screenshot long edge used to interpret `--space image`.
        #[arg(long, default_value_t = 1440)]
        image_max: u32,
        #[arg(long, value_enum, default_value_t = ButtonArg::Left)]
        button: ButtonArg,
        #[arg(long, default_value_t = 1)]
        count: u8,
        /// Modifiers, e.g. "cmd+shift".
        #[arg(long)]
        mods: Option<String>,
    },
    /// Move the cursor (points).
    Move { x: f64, y: f64 },
    /// Drag with the left button between two points (points).
    Drag { x1: f64, y1: f64, x2: f64, y2: f64 },
    /// Type text.
    Type { text: String },
    /// Press a key combo, e.g. "cmd+space" or "Return".
    Key {
        combo: String,
        #[arg(long, default_value_t = 1)]
        repeat: u8,
    },
    /// Scroll at a point (points). Positive dy scrolls content down.
    Scroll {
        x: f64,
        y: f64,
        #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
        dx: f64,
        #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
        dy: f64,
        /// Pixel deltas instead of wheel clicks.
        #[arg(long)]
        pixels: bool,
    },
    /// Accessibility queries.
    Ax {
        #[command(subcommand)]
        cmd: AxCmd,
    },
    /// Measure how long the screen takes to settle.
    Settle {
        #[arg(long, default_value_t = 2000)]
        timeout: u32,
    },
    /// Run a task with a model provider.
    Run(RunArgs),
    /// Open the mobile-friendly dashboard. Use --host 0.0.0.0 for your phone.
    Serve(mobile::ServeArgs),
}

#[derive(Subcommand)]
enum AxCmd {
    /// Focused app and element.
    Focused,
    /// Element at a point (global points).
    At { x: f64, y: f64 },
}

#[derive(Clone, Copy, ValueEnum)]
enum Space {
    Points,
    Image,
}

#[derive(Clone, Copy, ValueEnum)]
enum ButtonArg {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

impl From<ButtonArg> for Button {
    fn from(b: ButtonArg) -> Self {
        match b {
            ButtonArg::Left => Button::Left,
            ButtonArg::Right => Button::Right,
            ButtonArg::Middle => Button::Middle,
            ButtonArg::Back => Button::Back,
            ButtonArg::Forward => Button::Forward,
        }
    }
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
enum ProviderArg {
    Codex,
    Anthropic,
    Openai,
    Mock,
}

#[derive(clap::Args)]
struct RunArgs {
    task: String,
    /// Enable host-mediated public Parallel / content-free Jev helpers.
    #[arg(long)]
    hybrid: bool,
    /// Explicitly shared public-only brief, SEPARATE from the private task.
    #[arg(long, requires = "hybrid")]
    public_research: Option<String>,
    #[arg(long, value_enum)]
    provider: Option<ProviderArg>,
    /// Pin the planner model; dynamic mode may still vary its reasoning.
    #[arg(long)]
    model: Option<String>,
    /// Codex: classify every planning turn, or use a fixed model/reasoning pair.
    #[arg(long, value_parser = ["dynamic", "fixed"])]
    routing: Option<String>,
    /// Codex fixed reasoning (implies --routing fixed unless explicitly specified).
    #[arg(long, value_parser = ["low", "medium", "high"])]
    reasoning: Option<String>,
    /// Scripted turns for `--provider mock` (see crates/cu-providers/src/mock.rs).
    #[arg(long)]
    script: Option<PathBuf>,
    /// observe | confirm-all | confirm-risky | auto
    #[arg(long)]
    mode: Option<Mode>,
    #[arg(long)]
    max_steps: Option<u32>,
    #[arg(long)]
    max_minutes: Option<f64>,
    #[arg(long)]
    max_cost: Option<f64>,
    #[arg(long)]
    screenshot_max: Option<u32>,
    /// Stream structured progress/routes and accept confirmations on stdin (native/web UI).
    #[arg(long, hide = true)]
    ui: bool,
}

fn main() -> ExitCode {
    // `cu --version` reports the Swift layer's ABI, read through the FFI.
    let version: &'static str = Box::leak(
        format!(
            "{} (CuMac ABI {})",
            env!("CARGO_PKG_VERSION"),
            cu_mac::abi_version()
        )
        .into_boxed_str(),
    );
    let matches = Cli::command().version(version).get_matches();
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    // Must happen on the main thread (keyboard layout map).
    if !matches!(&cli.cmd, Cmd::CodexStatus) {
        if let Err(e) = cu_mac::init() {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    }
    match dispatch(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn parse_mods(s: Option<&str>) -> anyhow::Result<Mods> {
    let Some(s) = s else { return Ok(Mods::NONE) };
    let combo = KeyCombo::parse(s)?;
    anyhow::ensure!(
        combo.key.is_none(),
        "--mods takes modifiers only (e.g. \"cmd+shift\")"
    );
    Ok(combo.mods)
}

fn dispatch(cli: Cli) -> anyhow::Result<ExitCode> {
    let cfg = Config::load(cli.config.as_deref())?;
    match cli.cmd {
        Cmd::CodexStatus => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let result = rt.block_on(CodexProvider::check_subscription(cfg.codex));
            let ready = result.is_ok();
            let message = match result {
                Ok(()) => {
                    "Codex subscription connected (live account and model catalog verified)".into()
                }
                Err(e) => format!("{e:#}"),
            };
            println!("{}", json!({"ready":ready,"message":message}));
            return Ok(if ready {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            });
        }
        Cmd::PointerTest { layout } => return pointer_test::run(&layout, cfg.policy()),
        Cmd::Doctor {
            request,
            open,
            input_test,
            json: true,
        } => {
            anyhow::ensure!(
                !request && !open && !input_test,
                "--json cannot be combined with permission changes or input tests"
            );
            let p = cu_mac::permissions()?;
            println!(
                "{}",
                json!({"screen_recording":p.screen_recording,"accessibility":p.accessibility || p.post_events,"version":env!("CARGO_PKG_VERSION")})
            );
        }
        Cmd::Doctor {
            request,
            open,
            input_test,
            json: false,
        } => return doctor(&cfg, request, open, input_test),
        Cmd::Screenshot {
            out,
            max,
            png,
            quality,
        } => {
            let format = if png || out.extension().is_some_and(|e| e == "png") {
                ImageFormat::Png
            } else {
                ImageFormat::Jpeg
            };
            let t = Instant::now();
            let (info, bytes) = cu_mac::capture(max, format, quality, None)?;
            let ms = t.elapsed().as_millis();
            std::fs::write(&out, &bytes).with_context(|| format!("writing {}", out.display()))?;
            println!(
                "{} {}×{} ({} KB, {ms} ms) frame_seq={} source={:?}",
                out.display(),
                info.image_w,
                info.image_h,
                bytes.len() / 1024,
                info.frame_seq,
                info.source
            );
        }
        Cmd::Display => println!(
            "{}",
            serde_json::to_string_pretty(&cu_mac::display_info()?)?
        ),
        Cmd::Cursor => {
            let p = cu_mac::cursor()?;
            println!("{:.1}, {:.1}", p.x, p.y);
        }
        Cmd::Click {
            x,
            y,
            space,
            image_max,
            button,
            count,
            mods,
        } => {
            let p = match space {
                Space::Points => PointP { x, y },
                Space::Image => {
                    let d = cu_mac::display_info()?;
                    let (w, h) = fit_size(d.pixel_w, d.pixel_h, image_max);
                    CoordMapping::full_display(d.bounds, w, h, d.scale)
                        .image_to_points(PointI { x, y })?
                }
            };
            input::click(p, button.into(), count, parse_mods(mods.as_deref())?)?;
            println!("clicked at ({:.1}, {:.1}) pt", p.x, p.y);
        }
        Cmd::Move { x, y } => input::mouse_move(PointP { x, y })?,
        Cmd::Drag { x1, y1, x2, y2 } => input::drag(
            &[PointP { x: x1, y: y1 }, PointP { x: x2, y: y2 }],
            Mods::NONE,
        )?,
        Cmd::Type { text } => input::type_text(&text)?,
        Cmd::Key { combo, repeat } => input::key(&KeyCombo::parse(&combo)?, repeat)?,
        Cmd::Scroll {
            x,
            y,
            dx,
            dy,
            pixels,
        } => input::scroll(PointP { x, y }, dx, dy, pixels, Mods::NONE)?,
        Cmd::Ax { cmd } => {
            let v = match cmd {
                AxCmd::Focused => serde_json::to_value(cu_mac::ax_focused()?)?,
                AxCmd::At { x, y } => {
                    serde_json::to_value(cu_mac::ax_element_at(PointP { x, y })?)?
                }
            };
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Cmd::Settle { timeout } => {
            let opts = SettleOpts {
                timeout_ms: timeout,
                ..cfg.settle_opts()
            };
            let r = MacPlatform::default().wait_settled(&opts)?;
            println!("{}", serde_json::to_string(&r)?);
        }
        Cmd::Run(args) => return run(&cfg, args),
        Cmd::Serve(args) => return mobile::serve(args, cli.config.as_deref()),
    }
    Ok(ExitCode::SUCCESS)
}

fn doctor(cfg: &Config, request: bool, open: bool, input_test: bool) -> anyhow::Result<ExitCode> {
    let mark = |ok: bool| if ok { "✅" } else { "❌" };
    println!(
        "cu {} (CuMac ABI {})",
        env!("CARGO_PKG_VERSION"),
        cu_mac::abi_version()
    );

    let p = cu_mac::permissions()?;
    println!("\nPermissions (granted to the responsible process — usually your terminal app):");
    let rows = [
        (
            "Screen Recording",
            p.screen_recording,
            "screen_recording",
            true,
        ),
        ("Accessibility", p.accessibility, "accessibility", true),
        ("Post events", p.post_events, "accessibility", true),
        (
            "Input Monitoring",
            p.input_monitoring,
            "input_monitoring",
            false,
        ),
    ];
    let mut missing_mask = 0u32;
    for (name, ok, key, required) in rows {
        let why = p.details[key]["why"].as_str().unwrap_or("");
        let opt = if required { "" } else { " (optional until M5)" };
        println!("  {} {name}{opt} — {why}", mark(ok));
        if !ok {
            if let Some(url) = p.details[key]["settings_url"].as_str() {
                println!("       System Settings: {url}");
                if open {
                    let _ = std::process::Command::new("open").arg(url).status();
                }
            }
            missing_mask |= match key {
                "screen_recording" => cu_mac_sys::CU_PERM_SCREEN,
                "accessibility" => cu_mac_sys::CU_PERM_ACCESSIBILITY,
                _ => cu_mac_sys::CU_PERM_LISTEN,
            };
        }
    }
    if !p.screen_recording {
        println!("       Note: Screen Recording grants usually need a restart of the terminal app to take effect.");
    }
    if request && missing_mask != 0 {
        cu_mac::request_permissions(missing_mask)?;
        println!("  Requested missing permissions; re-run `cu doctor` after granting them.");
    }

    let d = cu_mac::display_info()?;
    let (w, h) = fit_size(d.pixel_w, d.pixel_h, cfg.anthropic.screenshot_max_long_edge);
    println!(
        "\nMain display: {}×{} pt @ {}x ({}×{} px), origin ({}, {}), generation {}",
        d.bounds.w, d.bounds.h, d.scale, d.pixel_w, d.pixel_h, d.bounds.x, d.bounds.y, d.generation
    );
    println!("  Screenshots sent to models: {w}×{h}");

    let mut ok = p.screen_recording && p.accessibility;
    if p.screen_recording {
        let t = Instant::now();
        match cu_mac::capture(w.max(h), cfg.capture.format, cfg.capture.jpeg_quality, None) {
            Ok((info, bytes)) => println!(
                "  {} Capture test: {}×{} in {} ms ({} KB)",
                mark(true),
                info.image_w,
                info.image_h,
                t.elapsed().as_millis(),
                bytes.len() / 1024
            ),
            Err(e) => {
                ok = false;
                println!("  {} Capture test failed: {e}", mark(false));
            }
        }
    }
    if input_test && p.post_events {
        let start = cu_mac::cursor()?;
        input::mouse_move(PointP {
            x: start.x + 1.0,
            y: start.y,
        })?;
        std::thread::sleep(Duration::from_millis(50));
        let moved = cu_mac::cursor()?;
        input::mouse_move(start)?;
        let good = (moved.x - start.x - 1.0).abs() < 0.6;
        ok &= good;
        println!(
            "  {} Input test: cursor moved {:.1} pt and back",
            mark(good),
            moved.x - start.x
        );
    }

    println!("\nAPI keys:");
    for k in ["ANTHROPIC_API_KEY", "OPENAI_API_KEY"] {
        println!(
            "  {} {k}",
            mark(std::env::var_os(k).is_some_and(|v| !v.is_empty()))
        );
    }
    println!("\nReminders:");
    println!("  • This drives your real Mac. Start with `--mode observe`, then `confirm-all`.");
    println!(
        "  • Enable a Focus / Do Not Disturb mode: notification banners leak into screenshots."
    );
    println!(
        "  • Move the mouse to pause a run; Ctrl-C aborts. Runs are logged under {}",
        cfg.runs_dir().display()
    );
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Best-effort PID of the terminal app that launched us: the top-most ancestor below launchd.
fn own_terminal_pid() -> Option<i32> {
    let mut pid = std::process::id() as i32;
    let mut top = None;
    for _ in 0..32 {
        let out = std::process::Command::new("ps")
            .args(["-o", "ppid=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let ppid: i32 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        if ppid <= 1 {
            return top;
        }
        top = Some(ppid);
        pid = ppid;
    }
    top
}

fn run(cfg: &Config, args: RunArgs) -> anyhow::Result<ExitCode> {
    let provider_kind = match args.provider {
        Some(p) => p,
        None if args.script.is_some() => ProviderArg::Mock,
        None => match cfg.provider.default.as_str() {
            "codex" => ProviderArg::Codex,
            "openai" => ProviderArg::Openai,
            "anthropic" => ProviderArg::Anthropic,
            other => bail!("unknown provider: {other}"),
        },
    };
    anyhow::ensure!(
        !args.hybrid || provider_kind == ProviderArg::Codex,
        "--hybrid currently requires the Codex planner"
    );
    let mode = args.mode.unwrap_or(cfg.safety.mode);
    let (mut provider, max_edge): (Box<dyn Provider>, u32) = match provider_kind {
        ProviderArg::Codex => {
            let mut codex = cfg.codex.clone();
            codex.model = args.model.clone().or(codex.model);
            if let Some(routing) = args.routing.as_deref() {
                codex.routing = if routing == "dynamic" {
                    RoutingMode::Dynamic
                } else {
                    RoutingMode::Fixed
                };
            }
            if let Some(reasoning) = args.reasoning.as_deref() {
                anyhow::ensure!(args.routing.as_deref() != Some("dynamic"), "--reasoning pins effort; use --routing fixed or omit --reasoning for dynamic routing");
                codex.routing = RoutingMode::Fixed;
                codex.reasoning = match reasoning {
                    "low" => Reasoning::Low,
                    "high" => Reasoning::High,
                    _ => Reasoning::Medium,
                };
            }
            let planner = CodexProvider::with_config(codex)?;
            let planner = if args.hybrid {
                planner.with_advisors(&cfg.hybrid, args.public_research.clone())?
            } else {
                planner
            };
            (Box::new(planner), cfg.codex.screenshot_max_long_edge)
        }
        ProviderArg::Mock => {
            let path = args
                .script
                .as_deref()
                .context("--provider mock needs --script <file.json>")?;
            (
                Box::new(MockProvider::new(Script::load(path)?)),
                cfg.anthropic.screenshot_max_long_edge,
            )
        }
        ProviderArg::Anthropic => {
            bail!("the Anthropic provider is not implemented yet (milestone M4)")
        }
        ProviderArg::Openai => bail!("the OpenAI provider is not implemented yet (milestone M6)"),
    };
    let max_edge = args.screenshot_max.unwrap_or(max_edge);

    let perms = cu_mac::permissions()?;
    if !perms.screen_recording {
        bail!("Screen Recording permission is missing; run `cu doctor`");
    }
    if mode != Mode::Observe && !(perms.accessibility || perms.post_events) {
        bail!("Accessibility permission is missing (needed to post input); run `cu doctor` or use --mode observe");
    }

    let policy = Policy::new(mode, cfg.policy())?;
    let platform = MacPlatform::default();
    let recorder = RunRecorder::create(&cfg.runs_dir())?;
    let display = cu_mac::display_info()?;
    recorder.write_json(
        "run.json",
        &json!({
            "task": args.task,
            "hybrid_enabled": args.hybrid,
            "public_research_brief": args.public_research,
            "provider": provider.name(),
            "model": provider.model(),
            "model_override": args.model,
            "routing_override": args.routing,
            "reasoning_override": args.reasoning,
            "model_selection_note": "Actual planner model, effort, router model and timings are logged per model_response event.",
            "mode": mode,
            "display": display,
            "config": cfg,
            "versions": { "cu": env!("CARGO_PKG_VERSION"), "cu_mac_abi": cu_mac::abi_version() },
        }),
    )?;

    let limits = AgentLimits {
        max_steps: args.max_steps.unwrap_or(cfg.safety.max_steps),
        max_wall_time: Duration::from_secs_f64(
            args.max_minutes.unwrap_or(cfg.safety.max_minutes) * 60.0,
        ),
        max_cost_usd: args.max_cost.unwrap_or(cfg.safety.max_cost_usd),
        max_wait: Duration::from_secs_f64(cfg.safety.max_wait_seconds),
    };
    let opts = AgentOptions {
        limits,
        observe: cfg.observe_opts(max_edge),
        zoom: None,
        settle: cfg.settle_opts(),
        own_terminal_pid: own_terminal_pid(),
    };

    if !args.ui {
        println!(
            "cu run — provider {} ({}), mode {mode:?}",
            provider.name(),
            provider.model()
        );
        println!("  run folder: {}", recorder.dir().display());
        println!("  move the mouse to pause · Ctrl-C to abort");
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let cancel = CancellationToken::new();
    let summary = rt.block_on(async {
        let c = cancel.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!("\nCtrl-C: aborting (press again to force quit)");
                c.cancel();
                let _ = input::release_all();
                if tokio::signal::ctrl_c().await.is_ok() {
                    let _ = input::release_all();
                    std::process::exit(130);
                }
            }
        });
        let terminal_supervisor = supervisor::TerminalSupervisor;
        let ui_supervisor = supervisor::UiSupervisor;
        let supervisor: &dyn cu_core::agent::Supervisor = if args.ui {
            &ui_supervisor
        } else {
            &terminal_supervisor
        };
        let agent = Agent {
            provider: provider.as_mut(),
            platform: &platform,
            policy: &policy,
            supervisor,
            recorder: &recorder,
            opts,
            cancel,
        };
        agent.run(&args.task).await
    });

    if args.ui {
        supervisor::emit(json!({"type":"done","summary":summary}));
        return Ok(if summary.status == RunStatus::Done {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }
    println!();
    println!(
        "{:?}: {} step(s), {} action(s) executed, {} failed, {} denied, {:.1}s",
        summary.status,
        summary.steps,
        summary.actions_executed,
        summary.actions_failed,
        summary.actions_denied,
        summary.wall_ms as f64 / 1000.0
    );
    if let Some(m) = &summary.message {
        println!("  {m}");
    }
    if let Some(t) = &summary.final_text {
        println!("\n{t}");
    }
    println!("  log: {}", recorder.dir().display());
    Ok(if summary.status == RunStatus::Done {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
