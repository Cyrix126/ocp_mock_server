//! Blockchain payment URI construction for the transaction details response.

use crate::cli::Config;

/// Build the payment URI handed out in the transaction details:
/// EIP-681 for EVM (native `?value=` or token `/transfer?...&uint256=`),
/// `tx_amount` for Monero, BIP-21-style `?amount=` for everything else.
pub fn build_uri(cfg: &Config) -> Result<String, String> {
    if let Some(uri) = &cfg.uri_override {
        return Ok(uri.clone());
    }
    if cfg.method.is_evm() {
        let chain_id = cfg
            .chain_id
            .or_else(|| cfg.method.default_chain_id())
            .ok_or("no default chain id for this method, pass --chain-id")?;
        return if let Some(contract) = &cfg.token_contract {
            let decimals = cfg
                .token_decimals
                .ok_or("--token-decimals is required with --token-contract")?;
            let raw = scale_decimal(&cfg.amount, decimals)?;
            Ok(format!(
                "ethereum:{contract}@{chain_id}/transfer?address={}&uint256={raw}",
                cfg.address
            ))
        } else {
            let raw = scale_decimal(&cfg.amount, 18)?;
            Ok(format!("ethereum:{}@{chain_id}?value={raw}", cfg.address))
        };
    }
    if cfg.token_contract.is_some() {
        return Err("--token-contract is only supported for EVM methods".into());
    }
    Ok(match cfg.method.spec_name() {
        "Monero" => format!("monero:{}?tx_amount={}", cfg.address, cfg.amount),
        m => format!("{}:{}?amount={}", m.to_lowercase(), cfg.address, cfg.amount),
    })
}

/// Scale a decimal string by 10^decimals into an exact integer string.
pub fn scale_decimal(amount: &str, decimals: u32) -> Result<String, String> {
    let (int_part, frac_part) = amount.split_once('.').unwrap_or((amount, ""));
    if int_part.is_empty() && frac_part.is_empty() {
        return Err(format!("invalid amount '{amount}'"));
    }
    if !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(format!("invalid amount '{amount}': expected a plain decimal"));
    }
    if frac_part.len() > decimals as usize {
        return Err(format!(
            "amount '{amount}' has more than {decimals} decimal places"
        ));
    }
    let mut digits = String::with_capacity(int_part.len() + decimals as usize);
    digits.push_str(int_part);
    digits.push_str(frac_part);
    digits.extend(std::iter::repeat_n('0', decimals as usize - frac_part.len()));
    let trimmed = digits.trim_start_matches('0');
    Ok(if trimmed.is_empty() { "0".into() } else { trimmed.into() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::method::Method;

    #[test]
    fn scale_decimal_scales_exactly() {
        assert_eq!(scale_decimal("0.0001", 8).unwrap(), "10000");
        assert_eq!(scale_decimal("1", 18).unwrap(), "1000000000000000000");
        assert_eq!(scale_decimal("1.5", 6).unwrap(), "1500000");
        assert_eq!(scale_decimal("0", 8).unwrap(), "0");
        assert!(scale_decimal("0.123456789", 8).is_err());
        assert!(scale_decimal("1,5", 8).is_err());
        assert!(scale_decimal(".", 8).is_err());
    }

    #[test]
    fn evm_native_uses_wei_value() {
        let cfg = Config::for_tests(
            Method::Ethereum,
            "ETH",
            "0x1111111111111111111111111111111111111111",
            "0.0004",
        );
        assert_eq!(
            build_uri(&cfg).unwrap(),
            "ethereum:0x1111111111111111111111111111111111111111@1?value=400000000000000"
        );
    }

    #[test]
    fn erc20_uses_eip681_transfer_form() {
        let mut cfg = Config::for_tests(
            Method::Ethereum,
            "USDC",
            "0x1111111111111111111111111111111111111111",
            "1.25",
        );
        cfg.token_contract = Some("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".into());
        cfg.token_decimals = Some(6);
        assert_eq!(
            build_uri(&cfg).unwrap(),
            "ethereum:0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48@1/transfer\
             ?address=0x1111111111111111111111111111111111111111&uint256=1250000"
        );
    }

    #[test]
    fn monero_uses_tx_amount() {
        let cfg = Config::for_tests(Method::Monero, "XMR", "4AdUmoney", "0.005");
        assert_eq!(build_uri(&cfg).unwrap(), "monero:4AdUmoney?tx_amount=0.005");
    }

    #[test]
    fn namecoin_uses_bip21_style_uri() {
        let cfg = Config::for_tests(
            Method::Namecoin,
            "NMC",
            "N2pGWAh65TWpWmEFrFssRQkQubbczJSKi9",
            "0.5",
        );
        assert_eq!(
            build_uri(&cfg).unwrap(),
            "namecoin:N2pGWAh65TWpWmEFrFssRQkQubbczJSKi9?amount=0.5"
        );
    }

    #[test]
    fn token_contract_rejected_outside_evm() {
        let mut cfg = Config::for_tests(Method::Bitcoin, "BTC", "bc1qtest", "0.0001");
        cfg.token_contract = Some("0xdead".into());
        cfg.token_decimals = Some(6);
        assert!(build_uri(&cfg).is_err());
    }
}
