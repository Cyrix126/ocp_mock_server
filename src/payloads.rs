//! JSON payloads, structured after the example responses in the
//! OpenCryptoPay documentation (https://github.com/openCryptoPay/landingPage).

use serde_json::{json, Value};

use crate::cli::Config;
use crate::state::{number, rfc3339, Quote};

/// Payment details (step 2 of the spec flow).
pub fn payment_details(cfg: &Config, quote: &Quote) -> Value {
    let description = format!("{} - {} {}", cfg.name, cfg.fiat, cfg.fiat_amount);
    let metadata = serde_json::to_string(&json!([["text/plain", description]])).unwrap();
    json!({
        "id": cfg.id,
        "externalId": "ocp-mock",
        "mode": "Multiple",
        "tag": "payRequest",
        "callback": cfg.callback_url(),
        "minSendable": 1000,
        "maxSendable": 1000,
        "metadata": metadata,
        "displayName": cfg.name,
        "standard": "OpenCryptoPay",
        "possibleStandards": ["OpenCryptoPay"],
        "displayQr": true,
        "recipient": {
            "name": cfg.name,
            "address": {
                "street": "Example Street",
                "houseNumber": "1",
                "zip": "0000",
                "city": "Testville",
                "country": "CH"
            },
            "website": "http://localhost/",
            "storeType": "Physical",
            "merchantCategory": "RetailTradeOthers",
            "goodsType": "Tangible",
            "goodsCategory": "FoodGroceryHealthProducts"
        },
        "route": "MOCK 01",
        "quote": {
            "id": quote.quote_id,
            "expiration": rfc3339(quote.expiration),
            "payment": quote.payment_id,
        },
        "requestedAmount": {
            "asset": cfg.fiat,
            "amount": number(cfg.fiat_amount),
        },
        "transferAmounts": [
            {
                "method": cfg.method.spec_name(),
                "minFee": number(cfg.min_fee),
                "assets": [ { "asset": cfg.asset, "amount": cfg.amount } ],
                "available": true
            }
        ]
    })
}

/// Transaction details (step 3 / simplified flow), with the spec's hint
/// wording per proof family.
pub fn transaction_details(cfg: &Config, quote: &Quote, uri: &str) -> Value {
    let tx_url = cfg.proof_url(&quote.payment_id);
    let hint = if cfg.method.is_hex() {
        format!(
            "Use this data to create a transaction and sign it. Send the signed \
             transaction back as HEX via the endpoint {tx_url}. We check the \
             transferred HEX and broadcast the transaction to the blockchain."
        )
    } else {
        format!(
            "Use this data to create a transaction and sign it. Broadcast the signed \
             transaction to the blockchain and send the transaction hash back via \
             the endpoint {tx_url}"
        )
    };
    json!({
        "expiryDate": rfc3339(quote.expiration),
        "blockchain": cfg.method.spec_name(),
        "uri": uri,
        "hint": hint,
    })
}

/// The spec's 404 shape ("No pending payment found").
pub fn no_pending(cfg: &Config) -> Value {
    json!({
        "id": cfg.id,
        "externalId": "ocp-mock",
        "displayName": cfg.name,
        "standard": "OpenCryptoPay",
        "possibleStandards": ["OpenCryptoPay"],
        "displayQr": true,
        "recipient": { "name": cfg.name },
        "statusCode": 404,
        "message": "No pending payment found",
        "error": "Not Found"
    })
}

pub fn error(code: u16, message: &str, error: &str) -> Value {
    json!({ "statusCode": code, "message": message, "error": error })
}
