//! Shared application state and small formatting helpers.

use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde_json::Number;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::cli::{CoinSpec, Config};
use crate::method::Method;
use crate::payment_uri::build_uri;

/// The single active quote. A fresh expiration is set on every
/// payment-details request.
pub struct Quote {
    pub quote_id: String,
    pub payment_id: String,
    pub expiration: SystemTime,
}

impl Quote {
    pub fn is_expired(&self) -> bool {
        self.expiration <= SystemTime::now()
    }
}

/// A configured coin with its precomputed payment URI (which points at the
/// user's own address).
pub struct CoinEntry {
    pub spec: CoinSpec,
    pub uri: String,
}

pub struct AppState {
    pub cfg: Config,
    pub coins: Vec<CoinEntry>,
    pub quote: Mutex<Quote>,
}

impl AppState {
    /// Validate the configured coins and precompute their payment URIs.
    pub fn new(cfg: Config) -> Result<Self, String> {
        for (i, a) in cfg.coins.iter().enumerate() {
            if cfg.coins[i + 1..]
                .iter()
                .any(|b| a.method == b.method && a.asset == b.asset)
            {
                return Err(format!("duplicate coin {}/{}", a.method, a.asset));
            }
        }
        let coins = cfg
            .coins
            .iter()
            .map(|spec| {
                let uri = build_uri(spec)
                    .map_err(|e| format!("coin {}/{}: {e}", spec.method, spec.asset))?;
                Ok(CoinEntry {
                    spec: spec.clone(),
                    uri,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let tag = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let quote = Quote {
            quote_id: format!("plq_mock{tag:08x}"),
            payment_id: format!("plp_mock{tag:08x}"),
            expiration: SystemTime::now() + Duration::from_secs(cfg.quote_ttl),
        };
        Ok(AppState {
            cfg,
            coins,
            quote: Mutex::new(quote),
        })
    }

    /// Extend the quote's validity by the configured TTL from now.
    pub fn refresh_quote(&self) {
        let mut quote = self.quote.lock().unwrap();
        quote.expiration = SystemTime::now() + Duration::from_secs(self.cfg.quote_ttl);
    }

    /// The coin matching a requested `method` & `asset` query pair.
    pub fn find_coin(&self, method: &str, asset: &str) -> Option<&CoinEntry> {
        self.coins
            .iter()
            .find(|e| e.spec.method.spec_name() == method && e.spec.asset == asset)
    }

    /// The configured method matching a requested `method` query parameter.
    pub fn find_method(&self, method: &str) -> Option<Method> {
        self.coins
            .iter()
            .map(|e| e.spec.method)
            .find(|m| m.spec_name() == method)
    }

    /// Human-readable "Method/ASSET" list for error messages.
    pub fn supported_pairs(&self) -> String {
        self.coins
            .iter()
            .map(|e| format!("{}/{}", e.spec.method, e.spec.asset))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub fn rfc3339(t: SystemTime) -> String {
    OffsetDateTime::from(t)
        .replace_nanosecond(0)
        .unwrap()
        .format(&Rfc3339)
        .unwrap()
}

/// Render an f64 as a JSON integer when it is whole, mirroring the spec's
/// example payloads (`"minFee": 0` but `"minFee": 4.5`).
pub fn number(v: f64) -> Number {
    if v.fract() == 0.0 && v >= 0.0 && v <= u64::MAX as f64 {
        Number::from(v as u64)
    } else {
        Number::from_f64(v).unwrap_or_else(|| Number::from(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_method_asset_pairs() {
        let cfg = Config::for_tests(vec![
            CoinSpec::for_tests(Method::Monero, "XMR", "addr1", "0.005"),
            CoinSpec::for_tests(Method::Monero, "XMR", "addr2", "0.006"),
        ]);
        let err = AppState::new(cfg).map(|_| ()).unwrap_err();
        assert!(err.contains("duplicate coin Monero/XMR"));
    }

    #[test]
    fn same_method_different_assets_is_allowed() {
        let cfg = Config::for_tests(vec![
            CoinSpec::for_tests(Method::Ethereum, "ETH", "0x11", "0.0004"),
            CoinSpec::for_tests(Method::Solana, "SOL", "sol1", "0.02"),
        ]);
        let state = AppState::new(cfg).unwrap();
        assert_eq!(state.coins.len(), 2);
        assert_eq!(state.supported_pairs(), "Ethereum/ETH, Solana/SOL");
        assert!(state.find_coin("Solana", "SOL").is_some());
        assert!(state.find_coin("Solana", "USDC").is_none());
        assert_eq!(state.find_method("Ethereum"), Some(Method::Ethereum));
        assert_eq!(state.find_method("Bitcoin"), None);
    }
}
