//! OpenCryptoPay payment methods and their properties.

use std::fmt;

use clap::ValueEnum;

/// A blockchain/payment method as named by the OpenCryptoPay standard.
///
/// Namecoin is not part of the published spec's method list; it is included
/// as a Bitcoin-family `hex` method for wallet testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "PascalCase")]
pub enum Method {
    Ethereum,
    Polygon,
    Arbitrum,
    Optimism,
    Base,
    BinanceSmartChain,
    Bitcoin,
    Firo,
    Namecoin,
    Monero,
    Zano,
    Solana,
    Tron,
    Cardano,
}

impl Method {
    /// Exact method name used in API requests and responses.
    pub fn spec_name(self) -> &'static str {
        match self {
            Method::Ethereum => "Ethereum",
            Method::Polygon => "Polygon",
            Method::Arbitrum => "Arbitrum",
            Method::Optimism => "Optimism",
            Method::Base => "Base",
            Method::BinanceSmartChain => "BinanceSmartChain",
            Method::Bitcoin => "Bitcoin",
            Method::Firo => "Firo",
            Method::Namecoin => "Namecoin",
            Method::Monero => "Monero",
            Method::Zano => "Zano",
            Method::Solana => "Solana",
            Method::Tron => "Tron",
            Method::Cardano => "Cardano",
        }
    }

    /// The proof is the raw signed transaction, broadcast by the provider;
    /// otherwise the wallet broadcasts and submits the transaction hash.
    pub fn is_hex(self) -> bool {
        !matches!(
            self,
            Method::Monero | Method::Zano | Method::Solana | Method::Tron | Method::Cardano
        )
    }

    pub fn is_evm(self) -> bool {
        matches!(
            self,
            Method::Ethereum
                | Method::Polygon
                | Method::Arbitrum
                | Method::Optimism
                | Method::Base
                | Method::BinanceSmartChain
        )
    }

    pub fn default_chain_id(self) -> Option<u64> {
        match self {
            Method::Ethereum => Some(1),
            Method::Optimism => Some(10),
            Method::BinanceSmartChain => Some(56),
            Method::Polygon => Some(137),
            Method::Base => Some(8453),
            Method::Arbitrum => Some(42161),
            _ => None,
        }
    }

    /// Query parameter carrying the transaction proof on the `/tx/` endpoint.
    pub fn proof_param(self) -> &'static str {
        if self.is_hex() {
            "hex"
        } else {
            "tx"
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.spec_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namecoin_is_hex_family() {
        assert!(Method::Namecoin.is_hex());
        assert!(!Method::Namecoin.is_evm());
        assert_eq!(Method::Namecoin.proof_param(), "hex");
    }

    #[test]
    fn hash_family_uses_tx_param() {
        for m in [
            Method::Monero,
            Method::Zano,
            Method::Solana,
            Method::Tron,
            Method::Cardano,
        ] {
            assert!(!m.is_hex());
            assert_eq!(m.proof_param(), "tx");
        }
    }

    #[test]
    fn spec_names_are_pascal_case_cli_values() {
        assert_eq!(
            Method::from_str("BinanceSmartChain", false).unwrap(),
            Method::BinanceSmartChain
        );
        assert_eq!(Method::from_str("Namecoin", false).unwrap(), Method::Namecoin);
    }
}
