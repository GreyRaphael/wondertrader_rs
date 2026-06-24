use std::{fs, path::PathBuf};

use anyhow::{Context, bail};
use clap::Parser;
use wt_core::{AppConfig, EngineMode};

#[derive(Debug, Parser)]
#[command(version, about = "Run WonderTrader Rust live/dry-run strategies")]
struct Args {
    #[arg(short, long, default_value = "configs/live.dryrun.toml")]
    config: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = load_config(&args.config)?;
    ensure_safe_mode(&config)?;

    println!("wt-live-runner config: {}", config.summary());
    println!(
        "execution account={}, dry_run={}",
        config.execution.account_id, config.execution.dry_run
    );
    Ok(())
}

fn load_config(path: &PathBuf) -> anyhow::Result<AppConfig> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("failed to parse config {}", path.display()))
}

fn ensure_safe_mode(config: &AppConfig) -> anyhow::Result<()> {
    if config.mode == EngineMode::Live && !config.execution.dry_run {
        bail!("live non-dry-run mode is not implemented in Phase 9.1");
    }
    Ok(())
}
