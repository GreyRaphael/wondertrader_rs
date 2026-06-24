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

pub trait SelStrategy {
    fn id(&self) -> &str;

    fn on_init(&mut self, _ctx: &mut SelContext<'_>) -> Result<()> {
        Ok(())
    }

    fn on_schedule(&mut self, ctx: &mut SelContext<'_>, event: &ScheduleEvent) -> Result<()>;

    fn on_bar(&mut self, _ctx: &mut SelContext<'_>, _bar: &Kline) -> Result<()> {
        Ok(())
    }
}

pub struct SelContext<'a> {
    strategy_id: &'a str,
    now: i64,
    positions: &'a HashMap<Symbol, Decimal>,
    bars: &'a HashMap<BarKey, VecDeque<Kline>>,
    subscriptions: &'a mut SubscriptionRegistry,
    targets: &'a mut Vec<TargetPosition>,
}

impl<'a> SelContext<'a> {
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

    pub fn set_target_positions<I>(&mut self, targets: I)
    where
        I: IntoIterator<Item = (Symbol, Decimal, Option<Decimal>, String)>,
    {
        for (symbol, qty, price, reason) in targets {
            self.set_target_position(symbol, qty, price, reason);
        }
    }
}

#[derive(Clone, Debug)]
pub struct SelEngineConfig {
    pub max_bars_per_series: usize,
}

impl Default for SelEngineConfig {
    fn default() -> Self {
        Self {
            max_bars_per_series: 4096,
        }
    }
}

pub struct SelEngine<S> {
    strategy: S,
    config: SelEngineConfig,
    positions: HashMap<Symbol, Decimal>,
    bars: HashMap<BarKey, VecDeque<Kline>>,
    subscriptions: SubscriptionRegistry,
    targets: Vec<TargetPosition>,
}

impl<S: SelStrategy> SelEngine<S> {
    pub fn new(strategy: S) -> Self {
        Self::with_config(strategy, SelEngineConfig::default())
    }

    pub fn with_config(strategy: S, config: SelEngineConfig) -> Self {
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
        let mut ctx = SelContext::new(
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
                    let mut ctx = SelContext::new(
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
                let mut ctx = SelContext::new(
                    &strategy_id,
                    event.ts,
                    positions,
                    bars,
                    subscriptions,
                    targets,
                );
                strategy.on_schedule(&mut ctx, event)?;
            }
            MarketEvent::Tick { .. }
            | MarketEvent::BookTicker { .. }
            | MarketEvent::Session { .. } => {}
        }
        Ok(())
    }

    pub fn drain_targets(&mut self) -> Vec<TargetPosition> {
        std::mem::take(&mut self.targets)
    }

    pub fn subscriptions(&self) -> &SubscriptionRegistry {
        &self.subscriptions
    }

    pub fn set_position(&mut self, symbol: Symbol, qty: Decimal) {
        self.positions.insert(symbol, qty);
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EveryIntervalScheduler {
    interval_ns: i64,
    next_fire: Option<i64>,
}

impl EveryIntervalScheduler {
    pub fn every_hours(hours: i64) -> Self {
        Self {
            interval_ns: hours * 60 * 60 * 1_000_000_000,
            next_fire: None,
        }
    }

    pub fn next_due(&mut self, now: i64, name: impl Into<String>) -> Option<ScheduleEvent> {
        match self.next_fire {
            None => {
                self.next_fire = Some(now + self.interval_ns);
                Some(ScheduleEvent {
                    ts: now,
                    name: name.into(),
                    fire_time: now,
                })
            }
            Some(next) if now >= next => {
                while self.next_fire.is_some_and(|fire| fire <= now) {
                    self.next_fire = self.next_fire.map(|fire| fire + self.interval_ns);
                }
                Some(ScheduleEvent {
                    ts: now,
                    name: name.into(),
                    fire_time: next,
                })
            }
            Some(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MomentumRankSelStrategy {
    id: String,
    symbols: Vec<Symbol>,
    interval: KlineInterval,
    lookback_bars: usize,
    long_count: usize,
    short_count: usize,
    notional_per_leg: Decimal,
}

impl MomentumRankSelStrategy {
    pub fn new(
        id: impl Into<String>,
        symbols: Vec<Symbol>,
        interval: KlineInterval,
        lookback_bars: usize,
        long_count: usize,
        short_count: usize,
        notional_per_leg: Decimal,
    ) -> Self {
        assert!(lookback_bars > 0, "lookback_bars must be positive");
        Self {
            id: id.into(),
            symbols,
            interval,
            lookback_bars,
            long_count,
            short_count,
            notional_per_leg,
        }
    }
}

impl SelStrategy for MomentumRankSelStrategy {
    fn id(&self) -> &str {
        &self.id
    }

    fn on_init(&mut self, ctx: &mut SelContext<'_>) -> Result<()> {
        for symbol in &self.symbols {
            ctx.subscribe_bars(symbol.clone(), self.interval);
        }
        Ok(())
    }

    fn on_schedule(&mut self, ctx: &mut SelContext<'_>, event: &ScheduleEvent) -> Result<()> {
        let mut scores = Vec::new();
        for symbol in &self.symbols {
            let bars = ctx.recent_bars(symbol, self.interval, self.lookback_bars + 1);
            if bars.len() < self.lookback_bars + 1 {
                continue;
            }
            let first = bars.first().expect("checked non-empty");
            let last = bars.last().expect("checked non-empty");
            if first.close == Decimal::ZERO || last.close == Decimal::ZERO {
                continue;
            }
            let score = last.close / first.close - Decimal::ONE;
            scores.push((symbol.clone(), score, last.close));
        }

        scores.sort_by(|left, right| right.1.cmp(&left.1));

        let mut selected: HashMap<Symbol, Decimal> = HashMap::new();
        for (symbol, _score, price) in scores.iter().take(self.long_count) {
            selected.insert(symbol.clone(), self.notional_per_leg / *price);
        }
        for (symbol, _score, price) in scores.iter().rev().take(self.short_count) {
            selected.insert(symbol.clone(), -self.notional_per_leg / *price);
        }

        let reason = format!("momentum_rank:{}", event.name);
        let targets = scores.into_iter().map(|(symbol, _score, price)| {
            let qty = selected.get(&symbol).copied().unwrap_or(Decimal::ZERO);
            (symbol, qty, Some(price), reason.clone())
        });
        ctx.set_target_positions(targets);
        Ok(())
    }
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

    #[test]
    fn sel_initialization_registers_all_bar_subscriptions() {
        let symbols = vec![Symbol::from("AAAUSDT"), Symbol::from("BBBUSDT")];
        let strategy = MomentumRankSelStrategy::new(
            "momentum",
            symbols.clone(),
            KlineInterval::M15,
            16,
            3,
            3,
            Decimal::from(100),
        );
        let mut engine = SelEngine::new(strategy);

        engine.initialize().unwrap();

        for symbol in symbols {
            assert!(
                engine
                    .subscriptions()
                    .is_bar_subscribed(&symbol, KlineInterval::M15)
            );
        }
    }

    #[test]
    fn momentum_rank_emits_long_short_and_flat_targets() {
        let symbols = vec![
            Symbol::from("AAAUSDT"),
            Symbol::from("BBBUSDT"),
            Symbol::from("CCCUSDT"),
            Symbol::from("DDDUSDT"),
        ];
        let strategy = MomentumRankSelStrategy::new(
            "momentum",
            symbols.clone(),
            KlineInterval::M15,
            2,
            1,
            1,
            Decimal::from(100),
        );
        let mut engine = SelEngine::new(strategy);
        engine.initialize().unwrap();

        let closes = [
            ("AAAUSDT", ["100", "110", "120"]),
            ("BBBUSDT", ["100", "100", "100"]),
            ("CCCUSDT", ["100", "90", "80"]),
            ("DDDUSDT", ["100", "105", "90"]),
        ];
        for idx in 0..3 {
            for (symbol, series) in &closes {
                engine
                    .on_event(&sel_bar_event(symbol, idx, series[idx]))
                    .unwrap();
            }
        }

        engine
            .on_event(&MarketEvent::Schedule {
                event: ScheduleEvent {
                    ts: 3 * 15 * 60 * 1_000_000_000,
                    name: "4h_rebalance".to_owned(),
                    fire_time: 3 * 15 * 60 * 1_000_000_000,
                },
            })
            .unwrap();

        let targets = engine.drain_targets();
        assert_eq!(targets.len(), 4);

        let target_by_symbol = targets
            .iter()
            .map(|target| (target.symbol.as_str().to_owned(), target.target_qty))
            .collect::<HashMap<_, _>>();
        assert!(target_by_symbol["AAAUSDT"] > Decimal::ZERO);
        assert_eq!(target_by_symbol["BBBUSDT"], Decimal::ZERO);
        assert!(target_by_symbol["CCCUSDT"] < Decimal::ZERO);
        assert_eq!(target_by_symbol["DDDUSDT"], Decimal::ZERO);
    }

    fn bar_event(idx: usize, close: &str) -> MarketEvent {
        sel_bar_event("BTCUSDT", idx, close)
    }

    fn sel_bar_event(symbol: &str, idx: usize, close: &str) -> MarketEvent {
        MarketEvent::BarClosed {
            kline: Kline {
                open_time: idx as i64 * 15 * 60_000_000_000,
                close_time: (idx as i64 + 1) * 15 * 60_000_000_000,
                symbol: Symbol::from(symbol),
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
