//! Execution crate.
//!
//! Phase 7 adds a safety-first execution MVP: target-position diffing, exchange
//! precision guards, and a dry-run adapter that records orders without touching
//! real Binance accounts. Signed REST/WebSocket user-stream implementations can
//! later implement the same adapter trait.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use rust_decimal::Decimal;
use wt_core::{
    AccountId, ClientOrderId, Order, OrderId, OrderStatus, OrderType, PositionSide, Side, Symbol,
    TargetPosition, TimeInForce,
};

pub const DEFAULT_EXECUTOR_ID: &str = "default";

#[derive(Clone, Debug, PartialEq)]
pub struct SymbolExecutionRules {
    pub min_qty: Decimal,
    pub step_size: Decimal,
    pub min_notional: Decimal,
}

impl SymbolExecutionRules {
    pub fn permissive() -> Self {
        Self {
            min_qty: Decimal::ZERO,
            step_size: Decimal::ZERO,
            min_notional: Decimal::ZERO,
        }
    }

    pub fn normalize_qty(&self, qty: Decimal) -> Decimal {
        let sign = if qty < Decimal::ZERO {
            Decimal::NEGATIVE_ONE
        } else {
            Decimal::ONE
        };
        let abs = qty.abs();
        if self.step_size <= Decimal::ZERO {
            return qty;
        }
        let steps = (abs / self.step_size).floor();
        sign * steps * self.step_size
    }

    pub fn is_tradeable(&self, qty: Decimal, price: Decimal) -> bool {
        let abs = qty.abs();
        abs >= self.min_qty && abs * price >= self.min_notional
    }
}

#[derive(Clone, Debug)]
pub struct ExecutionRequest {
    pub target: TargetPosition,
    pub current_qty: Decimal,
    pub reference_price: Decimal,
}

pub trait ExecutionAdapter {
    fn submit_order(&mut self, order: Order) -> Result<Order>;
    fn cancel_order(&mut self, _order_id: &OrderId) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct TargetPositionExecutor<A> {
    adapter: A,
    account_id: AccountId,
    default_rules: SymbolExecutionRules,
    symbol_rules: HashMap<Symbol, SymbolExecutionRules>,
    order_seq: u64,
}

impl<A: ExecutionAdapter> TargetPositionExecutor<A> {
    pub fn new(adapter: A, account_id: AccountId) -> Self {
        Self {
            adapter,
            account_id,
            default_rules: SymbolExecutionRules::permissive(),
            symbol_rules: HashMap::new(),
            order_seq: 0,
        }
    }

    pub fn with_default_rules(mut self, rules: SymbolExecutionRules) -> Self {
        self.default_rules = rules;
        self
    }

    pub fn set_symbol_rules(&mut self, symbol: Symbol, rules: SymbolExecutionRules) {
        self.symbol_rules.insert(symbol, rules);
    }

    pub fn adapter(&self) -> &A {
        &self.adapter
    }

    pub fn adapter_mut(&mut self) -> &mut A {
        &mut self.adapter
    }

    pub fn execute_target(&mut self, request: ExecutionRequest) -> Result<Option<Order>> {
        let rules = self
            .symbol_rules
            .get(&request.target.symbol)
            .unwrap_or(&self.default_rules);
        let raw_diff = request.target.target_qty - request.current_qty;
        let order_qty = rules.normalize_qty(raw_diff);
        if order_qty == Decimal::ZERO {
            return Ok(None);
        }
        if !rules.is_tradeable(order_qty, request.reference_price) {
            return Ok(None);
        }

        self.order_seq += 1;
        let side = if order_qty > Decimal::ZERO {
            Side::Buy
        } else {
            Side::Sell
        };
        let order = Order {
            ts_create: request.target.ts,
            ts_update: request.target.ts,
            strategy_id: Some(request.target.strategy_id.clone()),
            account_id: self.account_id.clone(),
            symbol: request.target.symbol.clone(),
            side,
            position_side: PositionSide::Both,
            order_type: OrderType::Market,
            time_in_force: Some(TimeInForce::Ioc),
            price: Some(request.reference_price),
            qty: order_qty.abs(),
            status: OrderStatus::New,
            client_order_id: Some(ClientOrderId(format!("wt-{}", self.order_seq))),
            exchange_order_id: None,
            reason: Some(request.target.reason.clone()),
        };
        self.adapter.submit_order(order).map(Some)
    }
}

#[derive(Clone, Debug, Default)]
pub struct DryRunExecutionAdapter {
    submitted_orders: Vec<Order>,
}

impl DryRunExecutionAdapter {
    pub fn submitted_orders(&self) -> &[Order] {
        &self.submitted_orders
    }
}

impl ExecutionAdapter for DryRunExecutionAdapter {
    fn submit_order(&mut self, mut order: Order) -> Result<Order> {
        if order.qty <= Decimal::ZERO {
            return Err(anyhow!("order quantity must be positive"));
        }
        order.status = OrderStatus::Filled;
        order.exchange_order_id = Some(OrderId(format!(
            "dry-run-{}",
            self.submitted_orders.len() + 1
        )));
        order.ts_update = order.ts_create;
        self.submitted_orders.push(order.clone());
        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use wt_core::TargetPosition;

    use super::*;

    #[test]
    fn normalizes_quantity_by_step_size() {
        let rules = SymbolExecutionRules {
            min_qty: Decimal::ZERO,
            step_size: Decimal::new(1, 1),
            min_notional: Decimal::ZERO,
        };

        assert_eq!(
            rules.normalize_qty(Decimal::new(123, 2)),
            Decimal::new(12, 1)
        );
        assert_eq!(
            rules.normalize_qty(Decimal::new(-123, 2)),
            Decimal::new(-12, 1)
        );
    }

    #[test]
    fn dry_run_executor_submits_market_order_from_target_diff() {
        let adapter = DryRunExecutionAdapter::default();
        let mut executor = TargetPositionExecutor::new(adapter, AccountId("dry".to_owned()))
            .with_default_rules(SymbolExecutionRules {
                min_qty: Decimal::new(1, 1),
                step_size: Decimal::new(1, 1),
                min_notional: Decimal::from(5),
            });

        let order = executor
            .execute_target(ExecutionRequest {
                target: target(Decimal::new(15, 1)),
                current_qty: Decimal::new(2, 1),
                reference_price: Decimal::from(100),
            })
            .unwrap()
            .unwrap();

        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.qty, Decimal::new(13, 1));
        assert_eq!(order.status, OrderStatus::Filled);
        assert_eq!(executor.adapter().submitted_orders().len(), 1);
    }

    #[test]
    fn executor_skips_too_small_order() {
        let adapter = DryRunExecutionAdapter::default();
        let mut executor = TargetPositionExecutor::new(adapter, AccountId("dry".to_owned()))
            .with_default_rules(SymbolExecutionRules {
                min_qty: Decimal::from(1),
                step_size: Decimal::new(1, 1),
                min_notional: Decimal::from(5),
            });

        let order = executor
            .execute_target(ExecutionRequest {
                target: target(Decimal::new(5, 1)),
                current_qty: Decimal::ZERO,
                reference_price: Decimal::from(100),
            })
            .unwrap();

        assert!(order.is_none());
        assert!(executor.adapter().submitted_orders().is_empty());
    }

    fn target(qty: Decimal) -> TargetPosition {
        TargetPosition {
            ts: 1,
            strategy_id: "unit".to_owned(),
            symbol: Symbol::from("BTCUSDT"),
            target_qty: qty,
            current_qty: None,
            signal_price: Some(Decimal::from(100)),
            reason: "unit".to_owned(),
        }
    }
}
