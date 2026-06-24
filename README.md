# wondertrader_rs

Rust implementation prototype inspired by WonderTrader, focused on Binance USDⓈ-M perpetual futures.

## Current status

Phase 0 and Phase 1 are implemented.

### Phase 0

The workspace contains the project skeleton and shared core domain types:

- `wt-core`: symbols, intervals, market data, orders, positions, events, config, errors, and logging helpers.
- `wt-market`: placeholder crate for Binance REST/WebSocket market data.
- `wt-storage`: Arrow IPC/Feather v2 storage crate.
- `wt-engine`: placeholder crate for CTA/SEL/HFT/UFT engines.
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

```bash
cargo test
```
