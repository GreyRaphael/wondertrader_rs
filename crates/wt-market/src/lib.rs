//! Market data gateway crate.
//!
//! Phase 2 implements the Binance USDⓈ-M public market-data boundary: REST
//! backfill clients, WebSocket stream naming, and payload normalization into
//! `wt-core` market events.

use std::{fmt, str::FromStr};

use futures_util::{StreamExt, stream::SplitStream};
use reqwest::Url;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use wt_core::{
    BookTicker, Kline, KlineInterval, MarketEvent, Symbol, Tick, TickSource, WtCoreError,
    unix_ts_ns_now,
};

pub const BINANCE_USDM_REST_BASE_URL: &str = "https://fapi.binance.com";
pub const BINANCE_USDM_WS_BASE_URL: &str = "wss://fstream.binance.com";
pub const CRATE_PHASE: &str = "phase-2-binance-market-data";

pub type MarketResult<T> = Result<T, MarketError>;

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Error)]
pub enum MarketError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("url parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("decimal parse error: {0}")]
    Decimal(#[from] rust_decimal::Error),

    #[error("core type error: {0}")]
    Core(#[from] WtCoreError),

    #[error("unsupported or malformed message: {0}")]
    UnsupportedMessage(String),
}

#[derive(Clone, Debug)]
pub struct BinanceRestClient {
    base_url: Url,
    client: reqwest::Client,
}

impl Default for BinanceRestClient {
    fn default() -> Self {
        Self::new(BINANCE_USDM_REST_BASE_URL).expect("default Binance base URL must be valid")
    }
}

impl BinanceRestClient {
    pub fn new(base_url: &str) -> MarketResult<Self> {
        Ok(Self {
            base_url: Url::parse(base_url)?,
            client: reqwest::Client::new(),
        })
    }

    pub async fn exchange_info(&self) -> MarketResult<ExchangeInfoResponse> {
        let url = self.endpoint("/fapi/v1/exchangeInfo")?;
        Ok(self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn klines(
        &self,
        symbol: &str,
        interval: KlineInterval,
        start_time_ms: Option<i64>,
        end_time_ms: Option<i64>,
        limit: Option<u16>,
    ) -> MarketResult<Vec<Kline>> {
        let mut url = self.endpoint("/fapi/v1/klines")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("symbol", symbol);
            query.append_pair("interval", interval.as_str());
            if let Some(start) = start_time_ms {
                query.append_pair("startTime", &start.to_string());
            }
            if let Some(end) = end_time_ms {
                query.append_pair("endTime", &end.to_string());
            }
            if let Some(limit) = limit {
                query.append_pair("limit", &limit.to_string());
            }
        }

        let rows: Vec<RestKlineRow> = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        rows.into_iter()
            .map(|row| row.into_kline(symbol, interval, "rest_kline"))
            .collect()
    }

    pub async fn agg_trades(
        &self,
        symbol: &str,
        from_id: Option<i64>,
        start_time_ms: Option<i64>,
        end_time_ms: Option<i64>,
        limit: Option<u16>,
    ) -> MarketResult<Vec<Tick>> {
        let mut url = self.endpoint("/fapi/v1/aggTrades")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("symbol", symbol);
            if let Some(from_id) = from_id {
                query.append_pair("fromId", &from_id.to_string());
            }
            if let Some(start) = start_time_ms {
                query.append_pair("startTime", &start.to_string());
            }
            if let Some(end) = end_time_ms {
                query.append_pair("endTime", &end.to_string());
            }
            if let Some(limit) = limit {
                query.append_pair("limit", &limit.to_string());
            }
        }

        let rows: Vec<BinanceAggTrade> = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        rows.into_iter().map(|row| row.into_tick(true)).collect()
    }

    fn endpoint(&self, path: &str) -> MarketResult<Url> {
        Ok(self.base_url.join(path)?)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BinanceStream {
    AggTrade {
        symbol: String,
    },
    Trade {
        symbol: String,
    },
    BookTicker {
        symbol: String,
    },
    Kline {
        symbol: String,
        interval: KlineInterval,
    },
}

impl BinanceStream {
    pub fn agg_trade(symbol: impl Into<String>) -> Self {
        Self::AggTrade {
            symbol: symbol.into(),
        }
    }

    pub fn kline(symbol: impl Into<String>, interval: KlineInterval) -> Self {
        Self::Kline {
            symbol: symbol.into(),
            interval,
        }
    }

    pub fn stream_name(&self) -> String {
        match self {
            Self::AggTrade { symbol } => format!("{}@aggTrade", symbol.to_ascii_lowercase()),
            Self::Trade { symbol } => format!("{}@trade", symbol.to_ascii_lowercase()),
            Self::BookTicker { symbol } => format!("{}@bookTicker", symbol.to_ascii_lowercase()),
            Self::Kline { symbol, interval } => {
                format!(
                    "{}@kline_{}",
                    symbol.to_ascii_lowercase(),
                    interval.as_str()
                )
            }
        }
    }
}

impl fmt::Display for BinanceStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.stream_name())
    }
}

pub fn combined_stream_url(streams: &[BinanceStream]) -> MarketResult<Url> {
    let joined = streams
        .iter()
        .map(BinanceStream::stream_name)
        .collect::<Vec<_>>()
        .join("/");
    Ok(Url::parse(&format!(
        "{BINANCE_USDM_WS_BASE_URL}/stream?streams={joined}"
    ))?)
}

pub async fn connect_combined_stream(streams: &[BinanceStream]) -> MarketResult<BinanceWsReader> {
    let url = combined_stream_url(streams)?;
    let (ws, _) = connect_async(url.as_str()).await?;
    let (_, read) = ws.split();
    Ok(BinanceWsReader { read })
}

pub struct BinanceWsReader {
    read: SplitStream<WsStream>,
}

impl BinanceWsReader {
    pub async fn next_event(&mut self) -> MarketResult<Option<MarketEvent>> {
        while let Some(message) = self.read.next().await {
            match message? {
                Message::Text(text) => return parse_ws_market_event(&text).map(Some),
                Message::Binary(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    return parse_ws_market_event(&text).map(Some);
                }
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(_) => return Ok(None),
                other => return Err(MarketError::UnsupportedMessage(other.to_string())),
            }
        }
        Ok(None)
    }
}

pub fn parse_ws_market_event(text: &str) -> MarketResult<MarketEvent> {
    let value: Value = serde_json::from_str(text)?;
    let data = value.get("data").unwrap_or(&value);
    let event_type = data.get("e").and_then(Value::as_str).ok_or_else(|| {
        MarketError::UnsupportedMessage("websocket payload missing event type".to_owned())
    })?;

    match event_type {
        "aggTrade" | "trade" => {
            let trade: BinanceAggTrade = serde_json::from_value(data.clone())?;
            Ok(MarketEvent::Tick {
                tick: trade.into_tick(false)?,
            })
        }
        "kline" => {
            let event: BinanceKlineEvent = serde_json::from_value(data.clone())?;
            let kline = event.into_kline()?;
            if kline.is_final {
                Ok(MarketEvent::BarClosed { kline })
            } else {
                Ok(MarketEvent::Kline { kline })
            }
        }
        "bookTicker" => {
            let book_ticker: BinanceBookTicker = serde_json::from_value(data.clone())?;
            Ok(MarketEvent::BookTicker {
                book_ticker: book_ticker.into_book_ticker()?,
            })
        }
        other => Err(MarketError::UnsupportedMessage(format!(
            "unsupported Binance event type: {other}"
        ))),
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeInfoResponse {
    pub timezone: String,
    pub server_time: i64,
    pub symbols: Vec<ExchangeSymbolInfo>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeSymbolInfo {
    pub symbol: String,
    pub status: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub contract_type: Option<String>,
    pub filters: Vec<ExchangeFilter>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ExchangeFilter {
    #[serde(rename = "filterType")]
    pub filter_type: String,
    #[serde(flatten)]
    pub fields: Value,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BinanceAggTrade {
    #[serde(rename = "e")]
    pub event_type: Option<String>,
    #[serde(rename = "E")]
    pub event_time_ms: Option<i64>,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "a")]
    pub agg_trade_id: i64,
    #[serde(rename = "p")]
    pub price: String,
    #[serde(rename = "q")]
    pub qty: String,
    #[serde(rename = "f")]
    pub first_trade_id: Option<i64>,
    #[serde(rename = "l")]
    pub last_trade_id: Option<i64>,
    #[serde(rename = "T")]
    pub trade_time_ms: i64,
    #[serde(rename = "m")]
    pub buyer_is_maker: bool,
    #[serde(rename = "M")]
    pub best_match: Option<bool>,
}

impl BinanceAggTrade {
    fn into_tick(self, is_recovered: bool) -> MarketResult<Tick> {
        Ok(Tick {
            ts_event: ms_to_ns(self.trade_time_ms),
            ts_recv: unix_ts_ns_now(),
            symbol: Symbol::from(self.symbol),
            source: if is_recovered {
                TickSource::RestRecovered
            } else {
                TickSource::AggTrade
            },
            trade_id: Some(self.agg_trade_id),
            price: Decimal::from_str(&self.price)?,
            qty: Decimal::from_str(&self.qty)?,
            side: Some(if self.buyer_is_maker { "sell" } else { "buy" }.to_owned()),
            bid_price: None,
            bid_qty: None,
            ask_price: None,
            ask_qty: None,
            is_recovered,
            raw_seq: Some(self.agg_trade_id),
        })
    }
}

#[derive(Clone, Debug)]
pub struct RestKlineRow(pub Vec<Value>);

impl<'de> Deserialize<'de> for RestKlineRow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(Vec::<Value>::deserialize(deserializer)?))
    }
}

impl RestKlineRow {
    fn into_kline(
        self,
        symbol: &str,
        interval: KlineInterval,
        source: &str,
    ) -> MarketResult<Kline> {
        if self.0.len() < 11 {
            return Err(MarketError::UnsupportedMessage(format!(
                "kline row expected at least 11 fields, got {}",
                self.0.len()
            )));
        }

        Ok(Kline {
            open_time: ms_to_ns(as_i64(&self.0[0], "open_time")?),
            close_time: ms_to_ns(as_i64(&self.0[6], "close_time")?),
            symbol: Symbol::from(symbol),
            interval,
            open: decimal_value(&self.0[1], "open")?,
            high: decimal_value(&self.0[2], "high")?,
            low: decimal_value(&self.0[3], "low")?,
            close: decimal_value(&self.0[4], "close")?,
            volume: decimal_value(&self.0[5], "volume")?,
            quote_volume: decimal_value(&self.0[7], "quote_volume")?,
            trade_count: as_i64(&self.0[8], "trade_count")?,
            taker_buy_volume: decimal_value(&self.0[9], "taker_buy_volume")?,
            taker_buy_quote_volume: decimal_value(&self.0[10], "taker_buy_quote_volume")?,
            is_final: true,
            source: source.to_owned(),
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct BinanceKlineEvent {
    #[serde(rename = "E")]
    pub event_time_ms: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "k")]
    pub kline: BinanceKlinePayload,
}

impl BinanceKlineEvent {
    fn into_kline(self) -> MarketResult<Kline> {
        let interval = KlineInterval::from_str(&self.kline.interval)?;
        Ok(Kline {
            open_time: ms_to_ns(self.kline.open_time_ms),
            close_time: ms_to_ns(self.kline.close_time_ms),
            symbol: Symbol::from(self.symbol),
            interval,
            open: Decimal::from_str(&self.kline.open)?,
            high: Decimal::from_str(&self.kline.high)?,
            low: Decimal::from_str(&self.kline.low)?,
            close: Decimal::from_str(&self.kline.close)?,
            volume: Decimal::from_str(&self.kline.volume)?,
            quote_volume: Decimal::from_str(&self.kline.quote_volume)?,
            trade_count: self.kline.trade_count,
            taker_buy_volume: Decimal::from_str(&self.kline.taker_buy_volume)?,
            taker_buy_quote_volume: Decimal::from_str(&self.kline.taker_buy_quote_volume)?,
            is_final: self.kline.is_final,
            source: "ws_kline".to_owned(),
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct BinanceKlinePayload {
    #[serde(rename = "t")]
    pub open_time_ms: i64,
    #[serde(rename = "T")]
    pub close_time_ms: i64,
    #[serde(rename = "i")]
    pub interval: String,
    #[serde(rename = "o")]
    pub open: String,
    #[serde(rename = "c")]
    pub close: String,
    #[serde(rename = "h")]
    pub high: String,
    #[serde(rename = "l")]
    pub low: String,
    #[serde(rename = "v")]
    pub volume: String,
    #[serde(rename = "n")]
    pub trade_count: i64,
    #[serde(rename = "x")]
    pub is_final: bool,
    #[serde(rename = "q")]
    pub quote_volume: String,
    #[serde(rename = "V")]
    pub taker_buy_volume: String,
    #[serde(rename = "Q")]
    pub taker_buy_quote_volume: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BinanceBookTicker {
    #[serde(rename = "E")]
    pub event_time_ms: Option<i64>,
    #[serde(rename = "T")]
    pub transaction_time_ms: Option<i64>,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "b")]
    pub bid_price: String,
    #[serde(rename = "B")]
    pub bid_qty: String,
    #[serde(rename = "a")]
    pub ask_price: String,
    #[serde(rename = "A")]
    pub ask_qty: String,
}

impl BinanceBookTicker {
    fn into_book_ticker(self) -> MarketResult<BookTicker> {
        let ts_ms = self
            .event_time_ms
            .or(self.transaction_time_ms)
            .unwrap_or_default();
        Ok(BookTicker {
            ts_event: ms_to_ns(ts_ms),
            ts_recv: unix_ts_ns_now(),
            symbol: Symbol::from(self.symbol),
            bid_price: Decimal::from_str(&self.bid_price)?,
            bid_qty: Decimal::from_str(&self.bid_qty)?,
            ask_price: Decimal::from_str(&self.ask_price)?,
            ask_qty: Decimal::from_str(&self.ask_qty)?,
        })
    }
}

fn ms_to_ns(ms: i64) -> i64 {
    ms * 1_000_000
}

fn as_i64(value: &Value, field: &str) -> MarketResult<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str()?.parse().ok())
        .ok_or_else(|| {
            MarketError::UnsupportedMessage(format!("field {field} is not an integer: {value}"))
        })
}

fn decimal_value(value: &Value, field: &str) -> MarketResult<Decimal> {
    let text = value.as_str().ok_or_else(|| {
        MarketError::UnsupportedMessage(format!("field {field} is not a decimal string: {value}"))
    })?;
    Ok(Decimal::from_str(text)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_stream_names_and_combined_url() {
        let streams = vec![
            BinanceStream::agg_trade("BTCUSDT"),
            BinanceStream::kline("ETHUSDT", KlineInterval::M15),
        ];

        assert_eq!(streams[0].stream_name(), "btcusdt@aggTrade");
        assert_eq!(streams[1].stream_name(), "ethusdt@kline_15m");
        assert_eq!(
            combined_stream_url(&streams).unwrap().as_str(),
            "wss://fstream.binance.com/stream?streams=btcusdt@aggTrade/ethusdt@kline_15m"
        );
    }

    #[test]
    fn parses_rest_kline_row() {
        let row: RestKlineRow = serde_json::from_str(
            r#"[
                1499040000000,
                "0.01634790",
                "0.80000000",
                "0.01575800",
                "0.01577100",
                "148976.11427815",
                1499644799999,
                "2434.19055334",
                308,
                "1756.87402397",
                "28.46694368",
                "0"
            ]"#,
        )
        .unwrap();

        let kline = row
            .into_kline("BTCUSDT", KlineInterval::M1, "rest_kline")
            .unwrap();
        assert_eq!(kline.symbol.as_str(), "BTCUSDT");
        assert_eq!(kline.open_time, 1_499_040_000_000_000_000);
        assert_eq!(kline.interval, KlineInterval::M1);
        assert!(kline.is_final);
    }

    #[test]
    fn parses_ws_agg_trade() {
        let event = parse_ws_market_event(
            r#"{
                "stream":"btcusdt@aggTrade",
                "data":{
                    "e":"aggTrade",
                    "E":123456789,
                    "s":"BTCUSDT",
                    "a":5933014,
                    "p":"65000.10",
                    "q":"0.002",
                    "f":100,
                    "l":105,
                    "T":123456785,
                    "m":false,
                    "M":true
                }
            }"#,
        )
        .unwrap();

        match event {
            MarketEvent::Tick { tick } => {
                assert_eq!(tick.symbol.as_str(), "BTCUSDT");
                assert_eq!(tick.trade_id, Some(5_933_014));
                assert_eq!(tick.side.as_deref(), Some("buy"));
                assert!(!tick.is_recovered);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn parses_final_ws_kline_as_bar_closed() {
        let event = parse_ws_market_event(
            r#"{
                "e":"kline",
                "E":123456789,
                "s":"BNBUSDT",
                "k":{
                    "t":123400000,
                    "T":123459999,
                    "s":"BNBUSDT",
                    "i":"1m",
                    "f":100,
                    "L":200,
                    "o":"100.0",
                    "c":"101.0",
                    "h":"102.0",
                    "l":"99.0",
                    "v":"12.0",
                    "n":50,
                    "x":true,
                    "q":"1200.0",
                    "V":"7.0",
                    "Q":"700.0",
                    "B":"0"
                }
            }"#,
        )
        .unwrap();

        match event {
            MarketEvent::BarClosed { kline } => {
                assert_eq!(kline.symbol.as_str(), "BNBUSDT");
                assert_eq!(kline.interval, KlineInterval::M1);
                assert!(kline.is_final);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn parses_ws_book_ticker() {
        let event = parse_ws_market_event(
            r#"{
                "e":"bookTicker",
                "E":1568014460893,
                "T":1568014460891,
                "s":"BTCUSDT",
                "b":"10000.00",
                "B":"1.25",
                "a":"10000.10",
                "A":"2.50"
            }"#,
        )
        .unwrap();

        match event {
            MarketEvent::BookTicker { book_ticker } => {
                assert_eq!(book_ticker.symbol.as_str(), "BTCUSDT");
                assert_eq!(book_ticker.ts_event, 1_568_014_460_893_000_000);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
