# wondertrader_rs

Rust implementation prototype inspired by WonderTrader, focused on Binance USDⓈ-M perpetual futures.

## Current status

Phase 0 through Phase 6 are implemented.

### Phase 0

The workspace contains the project skeleton and shared core domain types:

- `wt-core`: symbols, intervals, market data, orders, positions, events, config, errors, and logging helpers.
- `wt-market`: placeholder crate for Binance REST/WebSocket market data.
- `wt-storage`: Arrow IPC/Feather v2 storage crate.
- `wt-engine`: CTA/SEL/HFT/UFT engine crate; CTA is implemented first.
- `wt-execution`: placeholder crate for execution adapters and routers.
- `wt-backtest`: placeholder crate for event replay and matching.
- `wt-report`: placeholder crate for metrics and reports.
- `apps/*`: CLI entry-point placeholders.

### Phase 1

`wt-storage` provides:

- Stable tick and kline column schemas.
- Conversion from `wt-core::Tick` / `wt-core::Kline` batches into Polars `DataFrame`s.
- Arrow IPC/Feather v2 write/read helpers.
- Lazy IPC scan helpers with symbol/time/interval filters.
- Partition path generation for `ticks` and `klines` datasets.

### Phase 2

`wt-market` provides:

- Binance USDⓈ-M REST client for `exchangeInfo`, `klines`, and `aggTrades`.
- REST kline and aggregate trade normalization into `wt-core::Kline` and `wt-core::Tick`.
- WebSocket combined-stream URL and stream-name generation.
- WebSocket payload parsing for `aggTrade`, `kline`, and `bookTicker` events.
- A WebSocket reader wrapper that yields normalized `wt-core::MarketEvent`s.

### Phase 3

`wt-engine` provides:

- `CtaStrategy` callback trait.
- `CtaContext` APIs for subscriptions, recent bar access, positions, and target-position signals.
- `CtaEngine` event dispatch and rolling bar cache.
- `MaCrossCtaStrategy` example with configurable symbol, interval, MA windows, and target quantity.
- Unit tests for subscription registration, MA cross-up long signal, and MA cross-down short signal.

### Phase 4

`wt-engine` also provides:

- `SelStrategy` callback trait.
- `SelContext` APIs for multi-symbol bar subscriptions, recent bar access, positions, and batch target-position signals.
- `SelEngine` event dispatch and rolling bar cache.
- `EveryIntervalScheduler` for simple interval-based schedule generation.
- `MomentumRankSelStrategy` example for cross-sectional long/short momentum ranking.
- Unit tests for SEL subscriptions and long/short/flat target generation.

### Phase 5

`wt-backtest` provides:

- `run_cta_backtest` to replay sorted `MarketEvent`s through `CtaEngine`.
- Target-position matching with configurable taker fee rate and slippage bps.
- Linear futures-style portfolio accounting with realized/unrealized PnL and equity snapshots.
- Trade and final-position outputs.
- Unit tests for portfolio PnL and MA-cross CTA backtest execution.

### Phase 6

`wt-report` provides:

- `compute_metrics` for equity/trade report metrics.
- Total return, annualized return, annualized volatility, Sharpe ratio, max drawdown, Calmar ratio.
- Win rate, profit factor, payoff ratio, average trade PnL, trade count, and total fees.
- Serializable `MetricsSummary` and `ReportConfig`.

```bash
cargo test
```
