//! Command-line configuration.

use clap::Parser;

use crate::method::Method;
use crate::payment_uri::scale_decimal;

/// Mock OpenCryptoPay provider — test a wallet's OpenCryptoPay flow with a
/// real transaction paid to your own address.
///
/// Implements the provider API from the OpenCryptoPay documentation
/// (https://github.com/openCryptoPay/landingPage) and prints a scannable
/// QR / payment link on startup.
#[derive(Parser, Debug, Clone)]
#[command(version, about)]
pub struct Config {
    /// Blockchain method. Hex-proof methods (Ethereum, Polygon, Arbitrum,
    /// Optimism, Base, BinanceSmartChain, Bitcoin, Firo, Namecoin) receive
    /// the signed transaction HEX and the wallet must NOT broadcast;
    /// hash-proof methods (Monero, Zano, Solana, Tron, Cardano) receive the
    /// tx hash after the wallet broadcast itself
    #[arg(long)]
    pub method: Method,

    /// Asset ticker (e.g. BTC, ETH, USDT, XMR, NMC)
    #[arg(long)]
    pub asset: String,

    /// YOUR receiving address (second wallet)
    #[arg(long)]
    pub address: String,

    /// Crypto amount as a plain decimal (e.g. 0.0001)
    #[arg(long, value_parser = parse_amount)]
    pub amount: String,

    /// Host used in the printed link and LNURL — set it to an address the
    /// paying device can reach; the server itself binds 0.0.0.0
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Port to listen on
    #[arg(long, default_value_t = 8080)]
    pub port: u16,

    /// EVM chain id (defaults per method, e.g. Ethereum = 1)
    #[arg(long)]
    pub chain_id: Option<u64>,

    /// ERC-20 contract address; emits an EIP-681 `/transfer` URI (EVM only)
    #[arg(long, requires = "token_decimals")]
    pub token_contract: Option<String>,

    /// ERC-20 token decimals (required with --token-contract)
    #[arg(long)]
    pub token_decimals: Option<u32>,

    /// Requested fiat asset shown in the payment details
    #[arg(long, default_value = "CHF")]
    pub fiat: String,

    /// Requested fiat amount shown in the payment details
    #[arg(long, default_value_t = 1.0)]
    pub fiat_amount: f64,

    /// minFee for the method (gas price in WEI for EVM, sat/vB for Bitcoin)
    #[arg(long, default_value_t = 0.0)]
    pub min_fee: f64,

    /// Quote validity in seconds, refreshed on each payment-details call
    #[arg(long, default_value_t = 600, value_parser = clap::value_parser!(u64).range(1..))]
    pub quote_ttl: u64,

    /// Full payment URI override (skips URI construction)
    #[arg(long = "uri")]
    pub uri_override: Option<String>,

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

/// Accept only plain decimals; scaling by up to 30 digits must succeed so the
/// amount is usable for any coin's base-unit conversion.
fn parse_amount(s: &str) -> Result<String, String> {
    scale_decimal(s, 30).map(|_| s.to_string())
}

#[cfg(test)]
impl Config {
    pub fn for_tests(method: Method, asset: &str, address: &str, amount: &str) -> Self {
        Config {
            method,
            asset: asset.into(),
            address: address.into(),
            amount: amount.into(),
            host: "127.0.0.1".into(),
            port: 8080,
            chain_id: None,
            token_contract: None,
            token_decimals: None,
            fiat: "CHF".into(),
            fiat_amount: 1.0,
            min_fee: 0.0,
            quote_ttl: 600,
            uri_override: None,
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
    fn rejects_malformed_amounts() {
        assert!(parse_amount("0.0001").is_ok());
        assert!(parse_amount("1,5").is_err());
        assert!(parse_amount(".").is_err());
    }
}
