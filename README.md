# wondertrader_rs

Rust implementation prototype inspired by WonderTrader, focused on Binance USDⓈ-M perpetual futures.

## Current status

Phase 0 through Phase 3 are implemented.

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

```bash
cargo test
```
