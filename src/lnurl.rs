//! LNURL (LUD-01) encoding.
//!
//! Hand-rolled BIP-173 bech32: LNURLs routinely exceed the 90-character limit
//! that general-purpose bech32 crates enforce.

const CHARSET: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

fn polymod(values: &[u8]) -> u32 {
    const GEN: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
    let mut chk: u32 = 1;
    for &v in values {
        let b = chk >> 25;
        chk = ((chk & 0x1ff_ffff) << 5) ^ u32::from(v);
        for (i, g) in GEN.iter().enumerate() {
            if (b >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    chk
}

fn hrp_expand(hrp: &str) -> Vec<u8> {
    let mut out: Vec<u8> = hrp.bytes().map(|b| b >> 5).collect();
    out.push(0);
    out.extend(hrp.bytes().map(|b| b & 31));
    out
}

fn convert_bits_8_to_5(data: &[u8]) -> Vec<u8> {
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut out = Vec::with_capacity(data.len() * 8 / 5 + 1);
    for &b in data {
        acc = (acc << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(((acc >> bits) & 31) as u8);
        }
    }
    if bits > 0 {
        out.push(((acc << (5 - bits)) & 31) as u8);
    }
    out
}

pub fn lnurl_encode(url: &str) -> String {
    let hrp = "lnurl";
    let data = convert_bits_8_to_5(url.as_bytes());

    let mut values = hrp_expand(hrp);
    values.extend(&data);
    values.extend([0u8; 6]);
    let pm = polymod(&values) ^ 1;

    let mut encoded = String::with_capacity(hrp.len() + 1 + data.len() + 6);
    encoded.push_str(hrp);
    encoded.push('1');
    for d in &data {
        encoded.push(CHARSET[*d as usize] as char);
    }
    for i in 0..6 {
        let d = ((pm >> (5 * (5 - i))) & 31) as usize;
        encoded.push(CHARSET[d] as char);
    }
    encoded.to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Example from the spec README.
    #[test]
    fn matches_spec_example() {
        assert_eq!(
            lnurl_encode("https://api.dfx.swiss/v1/lnurlp/pl_beeddb41cd4b6d9e"),
            "LNURL1DP68GURN8GHJ7CTSDYHXGENC9EEHW6TNWVHHVVF0D3H82UNVWQHHQMZLVFJK2ERYVG6RZCMYX33RVEPEV5YEJ9WT"
        );
    }
}
