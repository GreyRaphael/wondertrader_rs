use std::{fs, path::PathBuf};

use anyhow::Context;
use clap::Parser;
use wt_core::AppConfig;

#[derive(Debug, Parser)]
#[command(version, about = "Record Binance USD-M market data into Feather files")]
struct Args {
    #[arg(short, long, default_value = "configs/md.toml")]
    config: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = load_config(&args.config)?;

    println!("wt-md-recorder config: {}", config.summary());
    println!(
        "recording streams: tick_stream={}, kline_source={}, intervals={:?}",
        config.market.tick_stream, config.market.kline_source, config.market.kline_intervals
    );
    Ok(())
}

fn load_config(path: &PathBuf) -> anyhow::Result<AppConfig> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("failed to parse config {}", path.display()))
}
