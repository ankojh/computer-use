use std::path::Path;
use std::process::{Command, ExitCode};

#[derive(clap::Args)]
pub struct ServeArgs {
    /// Bind address. LAN access must be requested explicitly.
    #[arg(long, default_value = "127.0.0.1")]
    host: std::net::IpAddr,
    #[arg(long, default_value_t = 8787)]
    port: u16,
}

pub fn serve(args: ServeArgs, config: Option<&Path>) -> anyhow::Result<ExitCode> {
    let mut cmd = Command::new("node");
    cmd.arg("-e")
        .arg(include_str!("../web/server.cjs"))
        .env("CU_HOST", args.host.to_string())
        .env("CU_PORT", args.port.to_string())
        .env("CU_BINARY", std::env::current_exe()?)
        .env("CU_WEB_HTML", include_str!("../web/index.html"));
    if let Some(config) = config {
        cmd.env("CU_CONFIG", std::fs::canonicalize(config)?);
    }
    let status = cmd
        .status()
        .map_err(|e| anyhow::anyhow!("The dashboard requires Node.js on this Mac: {e}"))?;
    Ok(if status.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
