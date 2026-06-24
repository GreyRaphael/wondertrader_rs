use std::{fs, path::PathBuf};

use anyhow::Context;
use clap::Parser;
use wt_core::AppConfig;

#[derive(Debug, Parser)]
#[command(version, about = "Run WonderTrader Rust backtests")]
struct Args {
    #[arg(short, long, default_value = "configs/backtest.toml")]
    config: PathBuf,

    #[arg(long)]
    strategy: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = load_config(&args.config)?;
    let strategy = args
        .strategy
        .as_deref()
        .unwrap_or_else(|| default_strategy(&config));

    println!("wt-backtest-cli config: {}", config.summary());
    println!("selected strategy: {strategy}");
    if let Some(backtest) = &config.backtest {
        println!(
            "backtest output_dir={}, initial_balance={}",
            backtest.output_dir, backtest.initial_balance
        );
    }
    Ok(())
}

fn load_config(path: &PathBuf) -> anyhow::Result<AppConfig> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("failed to parse config {}", path.display()))
}

fn default_strategy(config: &AppConfig) -> &'static str {
    if config.strategies.cta_ma_cross.is_some() {
        "cta-ma-cross"
    } else if config.strategies.sel_momentum.is_some() {
        "sel-momentum"
    } else {
        "none"
    }
}
