# OpenCryptoPay mock server

A mock OpenCryptoPay provider for testing the wallet's OpenCryptoPay flow
end-to-end with a **real transaction sent to your own address** — you only
lose the network fee.

On startup it prints an OpenCryptoPay QR code / link
(`https://<host>/pl/?lightning=LNURL1...`) whose LNURL decodes to this
server's API URL.

## Usage

```sh
cargo run --release -- \
  --host 127.0.0.1 --port 8080 \
  --method Monero --asset XMR \
  --address <ADDRESS-OF-YOUR-SECOND-WALLET> \
  --amount 0.005
```

Then scan the printed QR with the wallet (or paste the link) and send.

ERC-20 example (EIP-681 `/transfer` URI):

```sh
cargo run --release -- \
  --host 192.168.1.10 \
  --method Ethereum --asset USDC \
  --address 0xYourAddress \
  --token-contract 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48 \
  --token-decimals 6 \
  --amount 1.25
```

See `--help` for all options (`--no-pending` serves the spec's 404 error for
testing that path, `--quote-ttl` controls quote expiry, `--uri` overrides the
payment URI entirely).

## Notes

- The server binds `127.0.0.1`; `--host` is only used in the printed link and
  LNURL, so set it to an address the mobile can reach (LAN IP, or
  `127.0.0.1` + `adb reverse tcp:8080 tcp:8080` for a USB-connected Android
  device).
- For Monero/Zano/Solana/Tron/Cardano the wallet broadcasts the transaction
  itself and submits the hash: the real funds move to your address
  automatically.
- For EVM/Bitcoin/Firo/Namecoin the spec says the wallet must **not** broadcast; it
  submits the signed transaction HEX and the provider broadcasts. The mock
  only logs the HEX — broadcast it yourself (e.g.
  `bitcoin-cli sendrawtransaction <hex>`, Electrum, or an
  `eth_sendRawTransaction` push service) if you want the funds to actually
  move. Until you broadcast, nothing has left your wallet.
