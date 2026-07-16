//! Shared application state and small formatting helpers.

use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde_json::Number;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::cli::Config;

/// The single active quote.
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

pub struct AppState {
    pub cfg: Config,
    /// Blockchain payment URI pointing at the user's own address.
    pub uri: String,
    pub quote: Mutex<Quote>,
}

impl AppState {
    pub fn new(cfg: Config, uri: String) -> Self {
        let tag = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let quote = Quote {
            quote_id: format!("plq_mock{tag:08x}"),
            payment_id: format!("plp_mock{tag:08x}"),
            expiration: SystemTime::now() + Duration::from_secs(cfg.quote_ttl),
        };
        AppState {
            cfg,
            uri,
            quote: Mutex::new(quote),
        }
    }

    pub fn refresh_quote(&self) {
        let mut quote = self.quote.lock().unwrap();
        quote.expiration = SystemTime::now() + Duration::from_secs(self.cfg.quote_ttl);
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
