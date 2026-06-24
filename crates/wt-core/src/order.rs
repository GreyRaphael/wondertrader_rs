use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{TsNs, types::Symbol};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrderId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClientOrderId(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionSide {
    Long,
    Short,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderType {
    Market,
    Limit,
    StopMarket,
    StopLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeInForce {
    Gtc,
    Ioc,
    Fok,
    Gtx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Canceled,
    Rejected,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TargetPosition {
    pub ts: TsNs,
    pub strategy_id: String,
    pub symbol: Symbol,
    pub target_qty: Decimal,
    pub current_qty: Option<Decimal>,
    pub signal_price: Option<Decimal>,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Order {
    pub ts_create: TsNs,
    pub ts_update: TsNs,
    pub strategy_id: Option<String>,
    pub account_id: AccountId,
    pub symbol: Symbol,
    pub side: Side,
    pub position_side: PositionSide,
    pub order_type: OrderType,
    pub time_in_force: Option<TimeInForce>,
    pub price: Option<Decimal>,
    pub qty: Decimal,
    pub status: OrderStatus,
    pub client_order_id: Option<ClientOrderId>,
    pub exchange_order_id: Option<OrderId>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    pub ts_trade: TsNs,
    pub strategy_id: Option<String>,
    pub account_id: AccountId,
    pub symbol: Symbol,
    pub side: Side,
    pub position_side: PositionSide,
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
    pub fee_asset: String,
    pub realized_pnl: Decimal,
    pub order_id: Option<OrderId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub ts: TsNs,
    pub strategy_id: Option<String>,
    pub account_id: AccountId,
    pub symbol: Symbol,
    pub qty_long: Decimal,
    pub qty_short: Decimal,
    pub net_qty: Decimal,
    pub avg_entry_price: Option<Decimal>,
    pub mark_price: Option<Decimal>,
    pub unrealized_pnl: Decimal,
}
