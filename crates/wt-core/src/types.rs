use std::fmt;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::{Result, WtCoreError};

pub type Price = Decimal;
pub type Qty = Decimal;

macro_rules! string_id {
    ($name:ident, $empty_err:expr) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err($empty_err);
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }
    };
}

string_id!(Symbol, WtCoreError::EmptySymbol);
string_id!(ExchangeSymbol, WtCoreError::EmptySymbol);
string_id!(StrategyId, WtCoreError::EmptySymbol);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_symbols() {
        assert_eq!(Symbol::new("   "), Err(WtCoreError::EmptySymbol));
    }

    #[test]
    fn keeps_symbol_text_unchanged() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        assert_eq!(symbol.as_str(), "BTCUSDT");
    }
}
