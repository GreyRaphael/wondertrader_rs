use std::{fs, path::PathBuf};

use anyhow::{Context, bail};
use clap::Parser;
use rust_decimal::{Decimal, prelude::FromPrimitive};
use wt_backtest::{BacktestConfig, run_cta_backtest};
use wt_core::{AccountId, AppConfig, CtaMaCrossConfig, EngineMode, Kline, MarketEvent};
use wt_engine::MaCrossCtaStrategy;
use wt_report::{ReportConfig, compute_metrics};
use wt_storage::read_klines_ipc;

const CTA_MA_CROSS: &str = "cta-ma-cross";

#[derive(Debug, Parser)]
#[command(version, about = "Run WonderTrader Rust backtests")]
struct Args {
    #[arg(short, long, default_value = "configs/backtest.toml")]
    config: PathBuf,

    #[arg(long)]
    strategy: Option<String>,

    #[arg(long)]
    input: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = load_config(&args.config)?;
    ensure_backtest_mode(&config)?;

    let strategy = args
        .strategy
        .as_deref()
        .unwrap_or_else(|| default_strategy(&config));

    println!("wt-backtest-cli config: {}", config.summary());
    println!("selected strategy: {strategy}");

    match strategy {
        CTA_MA_CROSS => run_cta_ma_cross(&config, args.input.as_ref()),
        "sel-momentum" => bail!("SEL backtest loop is planned for Phase 11"),
        "none" => bail!("no strategy configured"),
        other => bail!("unsupported strategy {other}"),
    }
}

fn load_config(path: &PathBuf) -> anyhow::Result<AppConfig> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("failed to parse config {}", path.display()))
}

fn ensure_backtest_mode(config: &AppConfig) -> anyhow::Result<()> {
    if config.mode != EngineMode::Backtest {
        bail!("wt-backtest-cli requires mode = \"backtest\"");
    }
    Ok(())
}

fn run_cta_ma_cross(config: &AppConfig, input_override: Option<&PathBuf>) -> anyhow::Result<()> {
    let cta = config
        .strategies
        .cta_ma_cross
        .as_ref()
        .context("missing [strategies.cta_ma_cross] config")?;
    let backtest = config
        .backtest
        .as_ref()
        .context("missing [backtest] config")?;
    let input_path = input_path(backtest.input_path.as_deref(), input_override)?;

    let symbols = [cta.symbol.as_str()];
    let klines = read_klines_ipc(
        &input_path,
        &symbols,
        Some(cta.interval),
        backtest.start_ts,
        backtest.end_ts,
    )
    .with_context(|| {
        format!(
            "failed to load kline IPC data from {}",
            input_path.display()
        )
    })?;
    if klines.is_empty() {
        bail!(
            "no klines found in {} for {} {}",
            input_path.display(),
            cta.symbol,
            cta.interval
        );
    }

    let strategy = build_cta_strategy(cta);
    let result = run_cta_backtest(
        strategy,
        bar_closed_events(klines),
        backtest_config(config)?,
    )?;
    let metrics = compute_metrics(
        &result.equity,
        &result.trades,
        &ReportConfig {
            periods_per_year: config.report.periods_per_year,
            risk_free_rate: config.report.risk_free_rate,
        },
    );

    fs::create_dir_all(&backtest.output_dir)
        .with_context(|| format!("failed to create output dir {}", backtest.output_dir))?;
    let metrics_path = PathBuf::from(&backtest.output_dir).join("metrics.json");
    let metrics_json = serde_json::to_string_pretty(&metrics)?;
    fs::write(&metrics_path, metrics_json)
        .with_context(|| format!("failed to write {}", metrics_path.display()))?;

    println!(
        "backtest completed: bars={}, trades={}, equity_points={}, metrics={}",
        result.equity.len(),
        result.trades.len(),
        result.equity.len(),
        metrics_path.display()
    );
    println!(
        "total_return={:.6}, max_drawdown={:.6}, sharpe={:?}",
        metrics.total_return, metrics.max_drawdown, metrics.sharpe_ratio
    );
    Ok(())
}

fn input_path(
    configured: Option<&str>,
    input_override: Option<&PathBuf>,
) -> anyhow::Result<PathBuf> {
    if let Some(path) = input_override {
        return Ok(path.clone());
    }
    configured
        .map(PathBuf::from)
        .context("missing backtest input_path; set [backtest].input_path or pass --input")
}

fn build_cta_strategy(config: &CtaMaCrossConfig) -> MaCrossCtaStrategy {
    MaCrossCtaStrategy::new(
        CTA_MA_CROSS,
        config.symbol.clone(),
        config.interval,
        config.fast,
        config.slow,
        config.target_qty,
    )
}

fn backtest_config(config: &AppConfig) -> anyhow::Result<BacktestConfig> {
    let run = config
        .backtest
        .as_ref()
        .context("missing [backtest] config")?;
    let taker_fee_rate =
        decimal_from_f64(config.execution.taker_fee_bps, "taker_fee_bps")? / Decimal::from(10_000);
    let slippage_bps = decimal_from_f64(config.execution.slippage_bps, "slippage_bps")?;
    Ok(BacktestConfig {
        account_id: AccountId(config.execution.account_id.clone()),
        initial_balance: run.initial_balance,
        taker_fee_rate,
        slippage_bps,
    })
}

fn decimal_from_f64(value: f64, field: &'static str) -> anyhow::Result<Decimal> {
    Decimal::from_f64(value).with_context(|| format!("invalid decimal value for {field}: {value}"))
}

fn bar_closed_events(klines: Vec<Kline>) -> impl Iterator<Item = MarketEvent> {
    klines
        .into_iter()
        .map(|kline| MarketEvent::BarClosed { kline })
}

fn default_strategy(config: &AppConfig) -> &'static str {
    if config.strategies.cta_ma_cross.is_some() {
        CTA_MA_CROSS
    } else if config.strategies.sel_momentum.is_some() {
        "sel-momentum"
    } else {
        "none"
    }
}
