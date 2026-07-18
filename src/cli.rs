//! Command-line configuration.

use clap::{Parser, ValueEnum};

use crate::method::Method;
use crate::payment_uri::scale_decimal;

/// Mock OpenCryptoPay provider — test a wallet's OpenCryptoPay flow with a
/// real transaction paid to your own address.
///
/// Implements the provider API from the OpenCryptoPay documentation
/// (https://github.com/openCryptoPay/landingPage) and prints a scannable
/// QR / payment link on startup. Several --coin specs may be given; they are
/// all offered in the payment details and the wallet picks one, exactly like
/// a real provider.
#[derive(Parser, Debug, Clone)]
#[command(version, about)]
pub struct Config {
    /// Coin accepted for the payment; repeat the flag to offer several.
    /// Comma-separated key=value spec with required keys
    /// method=, asset=, address=, amount= and optional keys
    /// chain-id= (EVM), contract= + decimals= (ERC-20), min-fee=, uri=
    /// (full URI override; must not contain commas).
    /// Hex-proof methods (Ethereum, Polygon, Arbitrum, Optimism, Base,
    /// BinanceSmartChain, Bitcoin, Firo, Namecoin) receive the signed
    /// transaction HEX and the wallet must NOT broadcast; hash-proof methods
    /// (Monero, Zano, Solana, Tron, Cardano) receive the tx hash after the
    /// wallet broadcast itself.
    /// Example: --coin method=Monero,asset=XMR,address=4AdU...,amount=0.005
    #[arg(long = "coin", required = true, value_parser = parse_coin_spec)]
    pub coins: Vec<CoinSpec>,

    /// Host used in the printed link and LNURL — set it to an address the
    /// paying device can reach; the server itself binds 0.0.0.0
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Port to listen on
    #[arg(long, default_value_t = 8080)]
    pub port: u16,

    /// Requested fiat asset shown in the payment details
    #[arg(long, default_value = "CHF")]
    pub fiat: String,

    /// Requested fiat amount shown in the payment details
    #[arg(long, default_value_t = 1.0)]
    pub fiat_amount: f64,

    /// Quote validity in seconds, refreshed on each payment-details call
    #[arg(long, default_value_t = 600, value_parser = clap::value_parser!(u64).range(1..))]
    pub quote_ttl: u64,

    /// Payment link id
    #[arg(long, default_value = "pl_mock01")]
    pub id: String,

    /// Merchant display name
    #[arg(long, default_value = "OCP Mock Shop")]
    pub name: String,

    /// Always answer 404 "No pending payment found" (error-path testing)
    #[arg(long)]
    pub no_pending: bool,
}

/// One coin the mock accepts: a (method, asset) pair paying out to the given
/// address. Parsed from the `--coin` key=value spec.
#[derive(Debug, Clone)]
pub struct CoinSpec {
    pub method: Method,
    pub asset: String,
    pub address: String,
    pub amount: String,
    pub chain_id: Option<u64>,
    pub token_contract: Option<String>,
    pub token_decimals: Option<u32>,
    /// minFee advertised for the method (gas price in WEI for EVM, sat/vB
    /// for Bitcoin). When several assets share a method, the first spec's
    /// value is used.
    pub min_fee: f64,
    pub uri_override: Option<String>,
}

impl Config {
    pub fn api_url(&self) -> String {
        format!("http://{}:{}/v1/lnurlp/{}", self.host, self.port, self.id)
    }

    pub fn callback_url(&self) -> String {
        format!("http://{}:{}/v1/lnurlp/cb/{}", self.host, self.port, self.id)
    }

    pub fn proof_url(&self, payment_id: &str) -> String {
        format!("http://{}:{}/v1/lnurlp/tx/{}", self.host, self.port, payment_id)
    }
}

fn parse_coin_spec(s: &str) -> Result<CoinSpec, String> {
    let mut method = None;
    let mut asset = None;
    let mut address = None;
    let mut amount = None;
    let mut chain_id = None;
    let mut token_contract = None;
    let mut token_decimals = None;
    let mut min_fee = 0.0;
    let mut uri_override = None;

    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| format!("invalid coin spec part '{part}': expected key=value"))?;
        match key {
            "method" => {
                method = Some(Method::from_str(value, false).map_err(|_| {
                    format!(
                        "unknown method '{value}' (expected one of {})",
                        method_names().join(", ")
                    )
                })?)
            }
            "asset" => asset = Some(value.to_string()),
            "address" => address = Some(value.to_string()),
            "amount" => {
                // Only plain decimals; scaling by up to 30 digits must
                // succeed so any coin's base-unit conversion works.
                scale_decimal(value, 30)?;
                amount = Some(value.to_string());
            }
            "chain-id" => {
                chain_id = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| format!("invalid chain-id '{value}'"))?,
                )
            }
            "contract" => token_contract = Some(value.to_string()),
            "decimals" => {
                token_decimals = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| format!("invalid decimals '{value}'"))?,
                )
            }
            "min-fee" => {
                min_fee = value
                    .parse::<f64>()
                    .map_err(|_| format!("invalid min-fee '{value}'"))?
            }
            "uri" => uri_override = Some(value.to_string()),
            other => return Err(format!("unknown key '{other}' in coin spec")),
        }
    }

    let (Some(method), Some(asset), Some(address), Some(amount)) =
        (method, asset, address, amount)
    else {
        return Err("coin spec needs method=, asset=, address= and amount=".into());
    };
    if token_contract.is_some() && token_decimals.is_none() {
        return Err("contract= requires decimals=".into());
    }

    Ok(CoinSpec {
        method,
        asset,
        address,
        amount,
        chain_id,
        token_contract,
        token_decimals,
        min_fee,
        uri_override,
    })
}

fn method_names() -> Vec<&'static str> {
    Method::value_variants()
        .iter()
        .map(|m| m.spec_name())
        .collect()
}

#[cfg(test)]
impl CoinSpec {
    /// Minimal coin spec for tests; override fields as needed.
    pub fn for_tests(method: Method, asset: &str, address: &str, amount: &str) -> Self {
        CoinSpec {
            method,
            asset: asset.into(),
            address: address.into(),
            amount: amount.into(),
            chain_id: None,
            token_contract: None,
            token_decimals: None,
            min_fee: 0.0,
            uri_override: None,
        }
    }
}

#[cfg(test)]
impl Config {
    /// Minimal config for tests; override fields as needed.
    pub fn for_tests(coins: Vec<CoinSpec>) -> Self {
        Config {
            coins,
            host: "127.0.0.1".into(),
            port: 8080,
            fiat: "CHF".into(),
            fiat_amount: 1.0,
            quote_ttl: 600,
            id: "pl_mock01".into(),
            name: "OCP Mock Shop".into(),
            no_pending: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        Config::command().debug_assert();
    }

    #[test]
    fn parses_a_minimal_coin_spec() {
        let spec =
            parse_coin_spec("method=Monero,asset=XMR,address=4AdUmoney,amount=0.005").unwrap();
        assert_eq!(spec.method, Method::Monero);
        assert_eq!(spec.asset, "XMR");
        assert_eq!(spec.address, "4AdUmoney");
        assert_eq!(spec.amount, "0.005");
        assert_eq!(spec.min_fee, 0.0);
    }

    #[test]
    fn parses_erc20_options() {
        let spec = parse_coin_spec(
            "method=Ethereum,asset=USDC,address=0x11,amount=1.25,\
             contract=0xA0b8,decimals=6,chain-id=1,min-fee=1000000000",
        )
        .unwrap();
        assert_eq!(spec.token_contract.as_deref(), Some("0xA0b8"));
        assert_eq!(spec.token_decimals, Some(6));
        assert_eq!(spec.chain_id, Some(1));
        assert_eq!(spec.min_fee, 1_000_000_000.0);
    }

    #[test]
    fn rejects_bad_coin_specs() {
        // Missing required keys.
        assert!(parse_coin_spec("method=Monero,asset=XMR").is_err());
        // Malformed amount.
        assert!(parse_coin_spec("method=Monero,asset=XMR,address=a,amount=1,5").is_err());
        // Unknown method with a helpful message.
        let err = parse_coin_spec("method=Dogecoin,asset=DOGE,address=a,amount=1").unwrap_err();
        assert!(err.contains("unknown method 'Dogecoin'"));
        assert!(err.contains("Namecoin"));
        // Unknown key.
        assert!(parse_coin_spec("method=Monero,asset=XMR,address=a,amount=1,foo=bar").is_err());
        // contract= without decimals=.
        assert!(
            parse_coin_spec("method=Ethereum,asset=USDC,address=a,amount=1,contract=0x1").is_err()
        );
    }
}
