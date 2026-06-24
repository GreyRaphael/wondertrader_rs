//! Strategy engine crate.
//!
//! Phase 3 implements a minimal CTA engine: strategy callbacks, context APIs,
//! rolling bar cache, target-position signal emission, and a MA-cross example.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    hash::{Hash, Hasher},
};

use anyhow::Result;
use rust_decimal::Decimal;
use wt_core::{
    Kline, KlineInterval, MarketEvent, ScheduleEvent, Symbol, TargetPosition, Tick, unix_ts_ns_now,
};

pub use wt_core::TargetPosition as EngineTargetPosition;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineKind {
    Cta,
    Sel,
    Hft,
    Uft,
}

pub trait CtaStrategy {
    fn id(&self) -> &str;

    fn on_init(&mut self, _ctx: &mut CtaContext<'_>) -> Result<()> {
        Ok(())
    }

    fn on_tick(&mut self, _ctx: &mut CtaContext<'_>, _tick: &Tick) -> Result<()> {
        Ok(())
    }

    fn on_bar(&mut self, _ctx: &mut CtaContext<'_>, _bar: &Kline) -> Result<()> {
        Ok(())
    }

    fn on_schedule(&mut self, _ctx: &mut CtaContext<'_>, _event: &ScheduleEvent) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq)]
pub struct BarKey {
    pub symbol: Symbol,
    pub interval: KlineInterval,
}

impl BarKey {
    pub fn new(symbol: Symbol, interval: KlineInterval) -> Self {
        Self { symbol, interval }
    }
}

impl PartialEq for BarKey {
    fn eq(&self, other: &Self) -> bool {
        self.symbol == other.symbol && self.interval == other.interval
    }
}

impl Hash for BarKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.symbol.hash(state);
        self.interval.hash(state);
    }
}

#[derive(Clone, Debug, Default)]
pub struct SubscriptionRegistry {
    tick_symbols: HashSet<Symbol>,
    bar_keys: HashSet<BarKey>,
}

impl SubscriptionRegistry {
    pub fn subscribe_ticks(&mut self, symbol: Symbol) {
        self.tick_symbols.insert(symbol);
    }

    pub fn subscribe_bars(&mut self, symbol: Symbol, interval: KlineInterval) {
        self.bar_keys.insert(BarKey::new(symbol, interval));
    }

    pub fn is_tick_subscribed(&self, symbol: &Symbol) -> bool {
        self.tick_symbols.contains(symbol)
    }

    pub fn is_bar_subscribed(&self, symbol: &Symbol, interval: KlineInterval) -> bool {
        self.bar_keys
            .contains(&BarKey::new(symbol.clone(), interval))
    }
}

pub struct CtaContext<'a> {
    strategy_id: &'a str,
    now: i64,
    positions: &'a HashMap<Symbol, Decimal>,
    bars: &'a HashMap<BarKey, VecDeque<Kline>>,
    subscriptions: &'a mut SubscriptionRegistry,
    targets: &'a mut Vec<TargetPosition>,
}

impl<'a> CtaContext<'a> {
    pub fn new(
        strategy_id: &'a str,
        now: i64,
        positions: &'a HashMap<Symbol, Decimal>,
        bars: &'a HashMap<BarKey, VecDeque<Kline>>,
        subscriptions: &'a mut SubscriptionRegistry,
        targets: &'a mut Vec<TargetPosition>,
    ) -> Self {
        Self {
            strategy_id,
            now,
            positions,
            bars,
            subscriptions,
            targets,
        }
    }

    pub fn subscribe_ticks(&mut self, symbol: impl Into<Symbol>) {
        self.subscriptions.subscribe_ticks(symbol.into());
    }

    pub fn subscribe_bars(&mut self, symbol: impl Into<Symbol>, interval: KlineInterval) {
        self.subscriptions.subscribe_bars(symbol.into(), interval);
    }

    pub fn position(&self, symbol: &Symbol) -> Decimal {
        self.positions.get(symbol).copied().unwrap_or(Decimal::ZERO)
    }

    pub fn recent_bars(
        &self,
        symbol: &Symbol,
        interval: KlineInterval,
        count: usize,
    ) -> Vec<Kline> {
        let key = BarKey::new(symbol.clone(), interval);
        self.bars
            .get(&key)
            .map(|items| items.iter().rev().take(count).cloned().collect::<Vec<_>>())
            .map(|mut items| {
                items.reverse();
                items
            })
            .unwrap_or_default()
    }

    pub fn set_target_position(
        &mut self,
        symbol: Symbol,
        target_qty: Decimal,
        signal_price: Option<Decimal>,
        reason: impl Into<String>,
    ) {
        let current_qty = self.position(&symbol);
        self.targets.push(TargetPosition {
            ts: self.now,
            strategy_id: self.strategy_id.to_owned(),
            symbol,
            target_qty,
            current_qty: Some(current_qty),
            signal_price,
            reason: reason.into(),
        });
    }
}

#[derive(Clone, Debug)]
pub struct CtaEngineConfig {
    pub max_bars_per_series: usize,
}

impl Default for CtaEngineConfig {
    fn default() -> Self {
        Self {
            max_bars_per_series: 4096,
        }
    }
}

pub struct CtaEngine<S> {
    strategy: S,
    config: CtaEngineConfig,
    positions: HashMap<Symbol, Decimal>,
    bars: HashMap<BarKey, VecDeque<Kline>>,
    subscriptions: SubscriptionRegistry,
    targets: Vec<TargetPosition>,
}

impl<S: CtaStrategy> CtaEngine<S> {
    pub fn new(strategy: S) -> Self {
        Self::with_config(strategy, CtaEngineConfig::default())
    }

    pub fn with_config(strategy: S, config: CtaEngineConfig) -> Self {
        Self {
            strategy,
            config,
            positions: HashMap::new(),
            bars: HashMap::new(),
            subscriptions: SubscriptionRegistry::default(),
            targets: Vec::new(),
        }
    }

    pub fn initialize(&mut self) -> Result<()> {
        let strategy_id = self.strategy.id().to_owned();
        let Self {
            strategy,
            positions,
            bars,
            subscriptions,
            targets,
            ..
        } = self;
        let mut ctx = CtaContext::new(
            &strategy_id,
            unix_ts_ns_now(),
            positions,
            bars,
            subscriptions,
            targets,
        );
        strategy.on_init(&mut ctx)
    }

    pub fn on_event(&mut self, event: &MarketEvent) -> Result<()> {
        match event {
            MarketEvent::Tick { tick } => {
                if self.subscriptions.is_tick_subscribed(&tick.symbol) {
                    let strategy_id = self.strategy.id().to_owned();
                    let Self {
                        strategy,
                        positions,
                        bars,
                        subscriptions,
                        targets,
                        ..
                    } = self;
                    let mut ctx = CtaContext::new(
                        &strategy_id,
                        tick.ts_event,
                        positions,
                        bars,
                        subscriptions,
                        targets,
                    );
                    strategy.on_tick(&mut ctx, tick)?;
                }
            }
            MarketEvent::Kline { kline } | MarketEvent::BarClosed { kline } => {
                self.push_bar(kline.clone());
                if self
                    .subscriptions
                    .is_bar_subscribed(&kline.symbol, kline.interval)
                {
                    let strategy_id = self.strategy.id().to_owned();
                    let Self {
                        strategy,
                        positions,
                        bars,
                        subscriptions,
                        targets,
                        ..
                    } = self;
                    let mut ctx = CtaContext::new(
                        &strategy_id,
                        kline.close_time,
                        positions,
                        bars,
                        subscriptions,
                        targets,
                    );
                    strategy.on_bar(&mut ctx, kline)?;
                }
            }
            MarketEvent::Schedule { event } => {
                let strategy_id = self.strategy.id().to_owned();
                let Self {
                    strategy,
                    positions,
                    bars,
                    subscriptions,
                    targets,
                    ..
                } = self;
                let mut ctx = CtaContext::new(
                    &strategy_id,
                    event.ts,
                    positions,
                    bars,
                    subscriptions,
                    targets,
                );
                strategy.on_schedule(&mut ctx, event)?;
            }
            MarketEvent::BookTicker { .. } | MarketEvent::Session { .. } => {}
        }
        Ok(())
    }

    pub fn drain_targets(&mut self) -> Vec<TargetPosition> {
        std::mem::take(&mut self.targets)
    }

    pub fn set_position(&mut self, symbol: Symbol, qty: Decimal) {
        self.positions.insert(symbol, qty);
    }

    pub fn subscriptions(&self) -> &SubscriptionRegistry {
        &self.subscriptions
    }

    fn push_bar(&mut self, kline: Kline) {
        let key = BarKey::new(kline.symbol.clone(), kline.interval);
        let items = self.bars.entry(key).or_default();
        items.push_back(kline);
        while items.len() > self.config.max_bars_per_series {
            items.pop_front();
        }
    }
}

#[derive(Clone, Debug)]
pub struct MaCrossCtaStrategy {
    id: String,
    symbol: Symbol,
    interval: KlineInterval,
    fast: usize,
    slow: usize,
    target_qty: Decimal,
}

impl MaCrossCtaStrategy {
    pub fn new(
        id: impl Into<String>,
        symbol: impl Into<Symbol>,
        interval: KlineInterval,
        fast: usize,
        slow: usize,
        target_qty: Decimal,
    ) -> Self {
        assert!(fast > 0, "fast MA window must be positive");
        assert!(
            slow > fast,
            "slow MA window must be greater than fast window"
        );
        Self {
            id: id.into(),
            symbol: symbol.into(),
            interval,
            fast,
            slow,
            target_qty,
        }
    }
}

impl CtaStrategy for MaCrossCtaStrategy {
    fn id(&self) -> &str {
        &self.id
    }

    fn on_init(&mut self, ctx: &mut CtaContext<'_>) -> Result<()> {
        ctx.subscribe_bars(self.symbol.clone(), self.interval);
        Ok(())
    }

    fn on_bar(&mut self, ctx: &mut CtaContext<'_>, bar: &Kline) -> Result<()> {
        if !bar.is_final || bar.symbol != self.symbol || bar.interval != self.interval {
            return Ok(());
        }

        let bars = ctx.recent_bars(&self.symbol, self.interval, self.slow + 1);
        if bars.len() < self.slow + 1 {
            return Ok(());
        }

        let prev_fast = simple_ma(&bars[bars.len() - 1 - self.fast..bars.len() - 1]);
        let prev_slow = simple_ma(&bars[bars.len() - 1 - self.slow..bars.len() - 1]);
        let curr_fast = simple_ma(&bars[bars.len() - self.fast..]);
        let curr_slow = simple_ma(&bars[bars.len() - self.slow..]);

        if prev_fast <= prev_slow && curr_fast > curr_slow {
            ctx.set_target_position(
                self.symbol.clone(),
                self.target_qty,
                Some(bar.close),
                format!("ma{}_cross_up_ma{}", self.fast, self.slow),
            );
        } else if prev_fast >= prev_slow && curr_fast < curr_slow {
            ctx.set_target_position(
                self.symbol.clone(),
                -self.target_qty,
                Some(bar.close),
                format!("ma{}_cross_down_ma{}", self.fast, self.slow),
            );
        }

        Ok(())
    }
}

fn simple_ma(bars: &[Kline]) -> Decimal {
    let sum = bars.iter().map(|bar| bar.close).sum::<Decimal>();
    sum / Decimal::from(bars.len())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use wt_core::MarketEvent;

    use super::*;

    #[test]
    fn cta_initialization_registers_bar_subscription() {
        let strategy = MaCrossCtaStrategy::new(
            "ma_cross",
            "BTCUSDT",
            KlineInterval::D1,
            5,
            20,
            Decimal::from_str("0.001").unwrap(),
        );
        let mut engine = CtaEngine::new(strategy);

        engine.initialize().unwrap();

        assert!(
            engine
                .subscriptions()
                .is_bar_subscribed(&Symbol::from("BTCUSDT"), KlineInterval::D1)
        );
    }

    #[test]
    fn ma_cross_emits_long_target_on_cross_up() {
        let strategy = MaCrossCtaStrategy::new(
            "ma_cross",
            "BTCUSDT",
            KlineInterval::M15,
            2,
            3,
            Decimal::from(10),
        );
        let mut engine = CtaEngine::new(strategy);
        engine.initialize().unwrap();

        for (idx, close) in ["3", "2", "1", "4"].into_iter().enumerate() {
            engine.on_event(&bar_event(idx, close)).unwrap();
        }

        let targets = engine.drain_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].symbol.as_str(), "BTCUSDT");
        assert_eq!(targets[0].target_qty, Decimal::from(10));
        assert_eq!(targets[0].reason, "ma2_cross_up_ma3");
    }

    #[test]
    fn ma_cross_emits_short_target_on_cross_down() {
        let strategy = MaCrossCtaStrategy::new(
            "ma_cross",
            "BTCUSDT",
            KlineInterval::M15,
            2,
            3,
            Decimal::from(10),
        );
        let mut engine = CtaEngine::new(strategy);
        engine.initialize().unwrap();

        for (idx, close) in ["1", "2", "3", "0"].into_iter().enumerate() {
            engine.on_event(&bar_event(idx, close)).unwrap();
        }

        let targets = engine.drain_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target_qty, Decimal::from(-10));
        assert_eq!(targets[0].reason, "ma2_cross_down_ma3");
    }

    fn bar_event(idx: usize, close: &str) -> MarketEvent {
        MarketEvent::BarClosed {
            kline: Kline {
                open_time: idx as i64 * 60_000_000_000,
                close_time: (idx as i64 + 1) * 60_000_000_000,
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
