# OpenCryptoPay mock server

A mock OpenCryptoPay provider for testing the wallet's OpenCryptoPay flow
end-to-end with a **real transaction sent to your own address** — you only
lose the network fee.

On startup it prints an OpenCryptoPay QR code / link
(`https://<host>/pl/?lightning=LNURL1...`) whose LNURL decodes to this
server's API URL.

## Usage

Coins are given as repeatable `--coin` specs (comma-separated key=value).
Like a real provider, every configured coin is offered in the payment details
and the wallet picks which one to pay with:

```sh
cargo run --release -- \
  --host 127.0.0.1 --port 8080 \
  --coin method=Monero,asset=XMR,address=<YOUR-XMR-ADDRESS>,amount=0.005 \
  --coin method=Bitcoin,asset=BTC,address=<YOUR-BTC-ADDRESS>,amount=0.0001
```

Then scan the printed QR with the wallet (or paste the link) and send.

Spec keys: `method=`, `asset=`, `address=`, `amount=` (required) plus
`chain-id=` (EVM, defaults per method), `contract=` + `decimals=` (ERC-20,
emits an EIP-681 `/transfer` URI), `min-fee=` (advertised minFee for the
method), `uri=` (full URI override; must not contain commas).

ERC-20 example — multiple assets on the same method become one
`transferAmounts` entry with several assets, like a real provider:

```sh
cargo run --release -- \
  --host 192.168.1.10 \
  --coin method=Ethereum,asset=ETH,address=0xYourAddress,amount=0.0004 \
  --coin method=Ethereum,asset=USDC,address=0xYourAddress,amount=1.25,contract=0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48,decimals=6
```

See `--help` for all options (`--no-pending` serves the spec's 404 error for
testing that path, `--quote-ttl` controls quote expiry).

## Notes

- The server binds `0.0.0.0`; `--host` is only used in the printed link and
  LNURL, so set it to an address the mobile can reach (LAN IP, or
  `127.0.0.1` + `adb reverse tcp:8080 tcp:8080` for a USB-connected Android
  device).
- When several assets share a method, the method's advertised `minFee` comes
  from the first `--coin` spec for that method.
- For Monero/Zano/Solana/Tron/Cardano the wallet broadcasts the transaction
  itself and submits the hash: the real funds move to your address
  automatically.
- For EVM/Bitcoin/Firo/Namecoin the spec says the wallet must **not** broadcast; it
  submits the signed transaction HEX and the provider broadcasts. The mock
  only logs the HEX — broadcast it yourself (e.g.
  `bitcoin-cli sendrawtransaction <hex>`, Electrum, or an
  `eth_sendRawTransaction` push service) if you want the funds to actually
  move. Until you broadcast, nothing has left your wallet.
