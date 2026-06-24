use serde::{Deserialize, Serialize};

use crate::{BookTicker, Kline, Tick, TsNs};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MarketEvent {
    Tick { tick: Tick },
    Kline { kline: Kline },
    BarClosed { kline: Kline },
    BookTicker { book_ticker: BookTicker },
    Schedule { event: ScheduleEvent },
    Session { event: SessionEvent },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleEvent {
    pub ts: TsNs,
    pub name: String,
    pub fire_time: TsNs,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEvent {
    pub ts: TsNs,
    pub kind: SessionEventKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEventKind {
    Begin,
    End,
    DateRoll,
}
