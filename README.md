# wondertrader_rs

Rust implementation prototype inspired by WonderTrader, focused on Binance USDⓈ-M perpetual futures.

## Phase 0 status

This workspace currently contains the project skeleton and shared core domain types:

- `wt-core`: symbols, intervals, market data, orders, positions, events, config, errors, and logging helpers.
- `wt-market`: placeholder crate for Binance REST/WebSocket market data.
- `wt-storage`: placeholder crate for Arrow IPC/Feather v2 storage.
- `wt-engine`: placeholder crate for CTA/SEL/HFT/UFT engines.
- `wt-execution`: placeholder crate for execution adapters and routers.
- `wt-backtest`: placeholder crate for event replay and matching.
- `wt-report`: placeholder crate for metrics and reports.
- `apps/*`: CLI entry-point placeholders.

```bash
cargo test
```
