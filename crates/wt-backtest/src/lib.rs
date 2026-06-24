//! Backtesting crate.
//!
//! Phase 5 implements a minimal CTA backtest loop with event replay,
//! target-position matching, portfolio accounting, fees, slippage, and equity
//! snapshots. The design deliberately keeps the strategy interface shared with
//! live engines by driving `wt-engine::CtaEngine` with `wt-core::MarketEvent`s.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use rust_decimal::{Decimal, prelude::Signed};
use wt_core::{
    AccountId, Kline, MarketEvent, OrderId, PositionSide, Side, Symbol, TargetPosition, Tick, Trade,
};
use wt_engine::{CtaEngine, CtaStrategy};

pub use wt_core::{Order, Position};

pub const BACKTEST_CLOCK: &str = "event_time";

#[derive(Clone, Debug)]
pub struct BacktestConfig {
    pub account_id: AccountId,
    pub initial_balance: Decimal,
    pub taker_fee_rate: Decimal,
    pub slippage_bps: Decimal,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            account_id: AccountId("backtest".to_owned()),
            initial_balance: Decimal::from(10_000),
            taker_fee_rate: Decimal::from(4) / Decimal::from(10_000),
            slippage_bps: Decimal::ZERO,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EquityPoint {
    pub ts: i64,
    pub balance: Decimal,
    pub equity: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub fee_total: Decimal,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BacktestResult {
    pub trades: Vec<Trade>,
    pub equity: Vec<EquityPoint>,
    pub final_positions: HashMap<Symbol, Decimal>,
}

#[derive(Clone, Debug, Default)]
struct PositionState {
    qty: Decimal,
    avg_entry_price: Option<Decimal>,
}

#[derive(Clone, Debug)]
pub struct BacktestPortfolio {
    account_id: AccountId,
    balance: Decimal,
    realized_pnl: Decimal,
    fee_total: Decimal,
    positions: HashMap<Symbol, PositionState>,
    trades: Vec<Trade>,
    equity: Vec<EquityPoint>,
    order_seq: u64,
    taker_fee_rate: Decimal,
    slippage_bps: Decimal,
}

impl BacktestPortfolio {
    pub fn new(config: &BacktestConfig) -> Self {
        Self {
            account_id: config.account_id.clone(),
            balance: config.initial_balance,
            realized_pnl: Decimal::ZERO,
            fee_total: Decimal::ZERO,
            positions: HashMap::new(),
            trades: Vec::new(),
            equity: Vec::new(),
            order_seq: 0,
            taker_fee_rate: config.taker_fee_rate,
            slippage_bps: config.slippage_bps,
        }
    }

    pub fn execute_target(
        &mut self,
        target: &TargetPosition,
        reference_price: Decimal,
    ) -> Option<Trade> {
        let current_qty = self
            .positions
            .get(&target.symbol)
            .map(|state| state.qty)
            .unwrap_or(Decimal::ZERO);
        let diff_qty = target.target_qty - current_qty;
        if diff_qty == Decimal::ZERO {
            return None;
        }

        let side = if diff_qty > Decimal::ZERO {
            Side::Buy
        } else {
            Side::Sell
        };
        let fill_price = self.apply_slippage(reference_price, side);
        let abs_qty = diff_qty.abs();
        let fee = abs_qty * fill_price * self.taker_fee_rate;
        let state = self.positions.entry(target.symbol.clone()).or_default();
        let realized = update_position_state(state, diff_qty, fill_price);

        self.realized_pnl += realized;
        self.fee_total += fee;
        self.balance += realized - fee;
        self.order_seq += 1;

        let trade = Trade {
            ts_trade: target.ts,
            strategy_id: Some(target.strategy_id.clone()),
            account_id: self.account_id.clone(),
            symbol: target.symbol.clone(),
            side,
            position_side: PositionSide::Both,
            price: fill_price,
            qty: abs_qty,
            fee,
            fee_asset: "USDT".to_owned(),
            realized_pnl: realized,
            order_id: Some(OrderId(format!("bt-{}", self.order_seq))),
        };
        self.trades.push(trade.clone());
        Some(trade)
    }

    pub fn mark_to_market(&mut self, ts: i64, prices: &HashMap<Symbol, Decimal>) {
        let unrealized = self
            .positions
            .iter()
            .filter_map(|(symbol, state)| {
                let mark = prices.get(symbol)?;
                let entry = state.avg_entry_price?;
                Some((mark - entry) * state.qty)
            })
            .sum::<Decimal>();
        self.equity.push(EquityPoint {
            ts,
            balance: self.balance,
            equity: self.balance + unrealized,
            realized_pnl: self.realized_pnl,
            unrealized_pnl: unrealized,
            fee_total: self.fee_total,
        });
    }

    pub fn into_result(self) -> BacktestResult {
        BacktestResult {
            trades: self.trades,
            equity: self.equity,
            final_positions: self
                .positions
                .into_iter()
                .map(|(symbol, state)| (symbol, state.qty))
                .collect(),
        }
    }

    fn apply_slippage(&self, price: Decimal, side: Side) -> Decimal {
        let slip = self.slippage_bps / Decimal::from(10_000);
        match side {
            Side::Buy => price * (Decimal::ONE + slip),
            Side::Sell => price * (Decimal::ONE - slip),
        }
    }
}

pub fn run_cta_backtest<S>(
    strategy: S,
    events: impl IntoIterator<Item = MarketEvent>,
    config: BacktestConfig,
) -> Result<BacktestResult>
where
    S: CtaStrategy,
{
    let mut engine = CtaEngine::new(strategy);
    engine.initialize()?;

    let mut portfolio = BacktestPortfolio::new(&config);
    let mut last_prices = HashMap::<Symbol, Decimal>::new();

    let mut events = events.into_iter().collect::<Vec<_>>();
    events.sort_by_key(event_ts);

    for event in events {
        update_last_price(&event, &mut last_prices);
        engine.on_event(&event)?;

        for target in engine.drain_targets() {
            let price = target
                .signal_price
                .or_else(|| last_prices.get(&target.symbol).copied())
                .ok_or_else(|| anyhow!("missing fill price for {}", target.symbol))?;
            portfolio.execute_target(&target, price);
            engine.set_position(target.symbol.clone(), target.target_qty);
        }

        if let Some(ts) = event_ts(&event) {
            portfolio.mark_to_market(ts, &last_prices);
        }
    }

    Ok(portfolio.into_result())
}

fn update_position_state(
    state: &mut PositionState,
    diff_qty: Decimal,
    fill_price: Decimal,
) -> Decimal {
    let current = state.qty;
    if current == Decimal::ZERO || current.signum() == diff_qty.signum() {
        let new_qty = current + diff_qty;
        let current_notional = state.avg_entry_price.unwrap_or(fill_price) * current.abs();
        let added_notional = fill_price * diff_qty.abs();
        state.qty = new_qty;
        state.avg_entry_price = Some((current_notional + added_notional) / new_qty.abs());
        return Decimal::ZERO;
    }

    let closed_qty = current.abs().min(diff_qty.abs());
    let entry = state.avg_entry_price.unwrap_or(fill_price);
    let realized = (fill_price - entry) * closed_qty * current.signum();
    let new_qty = current + diff_qty;
    state.qty = new_qty;
    if new_qty == Decimal::ZERO {
        state.avg_entry_price = None;
    } else if new_qty.signum() == current.signum() {
        state.avg_entry_price = Some(entry);
    } else {
        state.avg_entry_price = Some(fill_price);
    }
    realized
}

fn update_last_price(event: &MarketEvent, last_prices: &mut HashMap<Symbol, Decimal>) {
    match event {
        MarketEvent::Tick { tick } => {
            last_prices.insert(tick.symbol.clone(), tick.price);
        }
        MarketEvent::Kline { kline } | MarketEvent::BarClosed { kline } => {
            last_prices.insert(kline.symbol.clone(), kline.close);
        }
        MarketEvent::BookTicker { book_ticker } => {
            let mid = (book_ticker.bid_price + book_ticker.ask_price) / Decimal::from(2);
            last_prices.insert(book_ticker.symbol.clone(), mid);
        }
        MarketEvent::Schedule { .. } | MarketEvent::Session { .. } => {}
    }
}

fn event_ts(event: &MarketEvent) -> Option<i64> {
    match event {
        MarketEvent::Tick { tick } => Some(tick.ts_event),
        MarketEvent::Kline { kline } | MarketEvent::BarClosed { kline } => Some(kline.close_time),
        MarketEvent::BookTicker { book_ticker } => Some(book_ticker.ts_event),
        MarketEvent::Schedule { event } => Some(event.ts),
        MarketEvent::Session { event } => Some(event.ts),
    }
}

#[allow(dead_code)]
fn _event_price(event: &MarketEvent) -> Option<Decimal> {
    match event {
        MarketEvent::Tick {
            tick: Tick { price, .. },
        } => Some(*price),
        MarketEvent::Kline {
            kline: Kline { close, .. },
        }
        | MarketEvent::BarClosed {
            kline: Kline { close, .. },
        } => Some(*close),
        MarketEvent::BookTicker { book_ticker } => {
            Some((book_ticker.bid_price + book_ticker.ask_price) / Decimal::from(2))
        }
        MarketEvent::Schedule { .. } | MarketEvent::Session { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use wt_core::KlineInterval;
    use wt_engine::MaCrossCtaStrategy;

    use super::*;

    #[test]
    fn portfolio_opens_and_closes_long_with_realized_pnl() {
        let config = BacktestConfig {
            taker_fee_rate: Decimal::ZERO,
            slippage_bps: Decimal::ZERO,
            ..BacktestConfig::default()
        };
        let mut portfolio = BacktestPortfolio::new(&config);
        let symbol = Symbol::from("BTCUSDT");

        portfolio.execute_target(
            &target(symbol.clone(), Decimal::from(1), Decimal::from(100)),
            Decimal::from(100),
        );
        portfolio.execute_target(
            &target(symbol.clone(), Decimal::ZERO, Decimal::from(110)),
            Decimal::from(110),
        );

        let result = portfolio.into_result();
        assert_eq!(result.trades.len(), 2);
        assert_eq!(result.trades[1].realized_pnl, Decimal::from(10));
        assert_eq!(result.final_positions[&symbol], Decimal::ZERO);
    }

    #[test]
    fn cta_backtest_runs_ma_cross_to_trade() {
        let strategy = MaCrossCtaStrategy::new(
            "ma_cross",
            "BTCUSDT",
            KlineInterval::M15,
            2,
            3,
            Decimal::from(10),
        );
        let events = ["3", "2", "1", "4"]
            .into_iter()
            .enumerate()
            .map(|(idx, close)| bar_event(idx, close))
            .collect::<Vec<_>>();

        let result = run_cta_backtest(
            strategy,
            events,
            BacktestConfig {
                taker_fee_rate: Decimal::ZERO,
                slippage_bps: Decimal::ZERO,
                ..BacktestConfig::default()
            },
        )
        .unwrap();

        assert_eq!(result.trades.len(), 1);
        assert_eq!(result.trades[0].side, Side::Buy);
        assert_eq!(result.trades[0].qty, Decimal::from(10));
        assert_eq!(
            result.final_positions[&Symbol::from("BTCUSDT")],
            Decimal::from(10)
        );
        assert!(!result.equity.is_empty());
    }

    fn target(symbol: Symbol, qty: Decimal, price: Decimal) -> TargetPosition {
        TargetPosition {
            ts: 1,
            strategy_id: "unit".to_owned(),
            symbol,
            target_qty: qty,
            current_qty: None,
            signal_price: Some(price),
            reason: "unit".to_owned(),
        }
    }

    fn bar_event(idx: usize, close: &str) -> MarketEvent {
        MarketEvent::BarClosed {
            kline: Kline {
                open_time: idx as i64 * 15 * 60_000_000_000,
                close_time: (idx as i64 + 1) * 15 * 60_000_000_000,
                symbol: Symbol::from("BTCUSDT"),
                interval: KlineInterval::M15,
                open: Decimal::ONE,
                high: Decimal::ONE,
                low: Decimal::ONE,
                close: Decimal::from_str(close).unwrap(),
                volume: Decimal::ONE,
                quote_volume: Decimal::ONE,
                trade_count: 1,
                taker_buy_volume: Decimal::ONE,
                taker_buy_quote_volume: Decimal::ONE,
                is_final: true,
                source: "unit_test".to_owned(),
            },
        }
    }
}
