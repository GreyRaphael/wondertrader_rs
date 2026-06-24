use std::{fmt, str::FromStr};

use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{TsNs, error::WtCoreError, types::Symbol};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KlineInterval {
    M1,
    M3,
    M5,
    M15,
    M30,
    H1,
    H2,
    H4,
    H6,
    H8,
    H12,
    D1,
    D3,
    W1,
    Mo1,
}

impl KlineInterval {
    pub const ALL: [Self; 15] = [
        Self::M1,
        Self::M3,
        Self::M5,
        Self::M15,
        Self::M30,
        Self::H1,
        Self::H2,
        Self::H4,
        Self::H6,
        Self::H8,
        Self::H12,
        Self::D1,
        Self::D3,
        Self::W1,
        Self::Mo1,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::M1 => "1m",
            Self::M3 => "3m",
            Self::M5 => "5m",
            Self::M15 => "15m",
            Self::M30 => "30m",
            Self::H1 => "1h",
            Self::H2 => "2h",
            Self::H4 => "4h",
            Self::H6 => "6h",
            Self::H8 => "8h",
            Self::H12 => "12h",
            Self::D1 => "1d",
            Self::D3 => "3d",
            Self::W1 => "1w",
            Self::Mo1 => "1mo",
        }
    }

    pub fn duration_ns(self) -> Option<i64> {
        let minute = 60_i64 * 1_000_000_000;
        let hour = 60 * minute;
        let day = 24 * hour;
        Some(match self {
            Self::M1 => minute,
            Self::M3 => 3 * minute,
            Self::M5 => 5 * minute,
            Self::M15 => 15 * minute,
            Self::M30 => 30 * minute,
            Self::H1 => hour,
            Self::H2 => 2 * hour,
            Self::H4 => 4 * hour,
            Self::H6 => 6 * hour,
            Self::H8 => 8 * hour,
            Self::H12 => 12 * hour,
            Self::D1 => day,
            Self::D3 => 3 * day,
            Self::W1 => 7 * day,
            Self::Mo1 => return None,
        })
    }
}

impl fmt::Display for KlineInterval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for KlineInterval {
    type Err = WtCoreError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "1m" => Ok(Self::M1),
            "3m" => Ok(Self::M3),
            "5m" => Ok(Self::M5),
            "15m" => Ok(Self::M15),
            "30m" => Ok(Self::M30),
            "1h" => Ok(Self::H1),
            "2h" => Ok(Self::H2),
            "4h" => Ok(Self::H4),
            "6h" => Ok(Self::H6),
            "8h" => Ok(Self::H8),
            "12h" => Ok(Self::H12),
            "1d" => Ok(Self::D1),
            "3d" => Ok(Self::D3),
            "1w" => Ok(Self::W1),
            "1mo" => Ok(Self::Mo1),
            other => Err(WtCoreError::InvalidKlineInterval(other.to_owned())),
        }
    }
}

impl Serialize for KlineInterval {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for KlineInterval {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TickSource {
    Trade,
    AggTrade,
    BookTicker,
    RestRecovered,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tick {
    pub ts_event: TsNs,
    pub ts_recv: TsNs,
    pub symbol: Symbol,
    pub source: TickSource,
    pub trade_id: Option<i64>,
    pub price: Decimal,
    pub qty: Decimal,
    pub side: Option<String>,
    pub bid_price: Option<Decimal>,
    pub bid_qty: Option<Decimal>,
    pub ask_price: Option<Decimal>,
    pub ask_qty: Option<Decimal>,
    pub is_recovered: bool,
    pub raw_seq: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BookTicker {
    pub ts_event: TsNs,
    pub ts_recv: TsNs,
    pub symbol: Symbol,
    pub bid_price: Decimal,
    pub bid_qty: Decimal,
    pub ask_price: Decimal,
    pub ask_qty: Decimal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Kline {
    pub open_time: TsNs,
    pub close_time: TsNs,
    pub symbol: Symbol,
    pub interval: KlineInterval,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub quote_volume: Decimal,
    pub trade_count: i64,
    pub taker_buy_volume: Decimal,
    pub taker_buy_quote_volume: Decimal,
    pub is_final: bool,
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_binance_intervals() {
        let values = [
            "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "3d", "1w",
            "1mo",
        ];
        assert_eq!(values.len(), KlineInterval::ALL.len());

        for value in values {
            let interval: KlineInterval = value.parse().unwrap();
            assert_eq!(interval.to_string(), value);
        }
    }

    #[test]
    fn rejects_unknown_interval() {
        assert_eq!(
            "7m".parse::<KlineInterval>(),
            Err(WtCoreError::InvalidKlineInterval("7m".to_owned()))
        );
    }

    #[test]
    fn month_interval_has_no_fixed_duration() {
        assert_eq!(KlineInterval::Mo1.duration_ns(), None);
    }
}
