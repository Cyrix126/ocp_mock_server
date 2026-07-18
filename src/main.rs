//! Mock OpenCryptoPay provider.
//!
//! Implements the provider side of the OpenCryptoPay standard as documented
//! at https://github.com/openCryptoPay/landingPage, so a wallet can be tested
//! end-to-end against addresses you own. See README.md and `--help`.

mod cli;
mod handlers;
mod lnurl;
mod method;
mod payloads;
mod payment_uri;
mod state;

use actix_web::{middleware, web, App, HttpServer};
use clap::Parser;

use crate::cli::Config;
use crate::state::AppState;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cfg = Config::parse();
    let state = match AppState::new(cfg) {
        Ok(state) => web::Data::new(state),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };
    print_banner(&state);

    let port = state.cfg.port;
    let app_state = state.clone();
    let server = HttpServer::new(move || {
        App::new()
            .wrap(middleware::NormalizePath::trim())
            .app_data(app_state.clone())
            .configure(handlers::routes)
    })
    .bind(("0.0.0.0", port))?;

    println!("Listening on 0.0.0.0:{port} ...");
    println!();
    server.run().await
}

fn print_banner(state: &AppState) {
    let cfg = &state.cfg;
    let api_url = cfg.api_url();
    let lnurl = lnurl::lnurl_encode(&api_url);
    // The wallet only decodes the outer link (it must be https per the QR
    // format); every actual request goes to the http API URL inside the LNURL.
    let qr_link = format!("https://{}:{}/pl/?lightning={}", cfg.host, cfg.port, lnurl);
    let payment_id = state.quote.lock().unwrap().payment_id.clone();

    println!("Mock OpenCryptoPay provider (spec: github.com/openCryptoPay/landingPage)");
    println!();
    println!("Accepted coins:");
    for coin in &state.coins {
        let spec = &coin.spec;
        let proof = if spec.method.is_hex() {
            "signed HEX, wallet must NOT broadcast"
        } else {
            "tx hash, wallet broadcasts"
        };
        println!(
            "  - {} / {} : {} -> {} ({proof})",
            spec.method, spec.asset, spec.amount, spec.address
        );
        println!("    URI: {}", coin.uri);
    }
    println!();
    println!("  API URL        : {api_url}");
    println!("  Proof endpoint : {}", cfg.proof_url(&payment_id));
    println!();
    println!("Scan this QR with the wallet (or paste the link below):");
    println!();
    match qrcode::QrCode::new(qr_link.as_bytes()) {
        Ok(code) => {
            let rendered = code
                .render::<qrcode::render::unicode::Dense1x2>()
                .quiet_zone(true)
                .build();
            println!("{rendered}");
        }
        Err(e) => eprintln!("(could not render QR: {e})"),
    }
    println!();
    println!("{qr_link}");
    println!();
    if state.coins.iter().any(|c| c.spec.method.is_hex()) {
        let hex_methods: Vec<&str> = state
            .coins
            .iter()
            .filter(|c| c.spec.method.is_hex())
            .map(|c| c.spec.method.spec_name())
            .collect();
        println!(
            "NOTE: for {} the wallet submits the SIGNED TX HEX and does not\n\
             broadcast — the mock only logs it. Broadcast it yourself (e.g.\n\
             `bitcoin-cli sendrawtransaction <hex>` or an eth_sendRawTransaction\n\
             push service) if you want the funds to actually move.",
            hex_methods.join(", ")
        );
        println!();
    }
}
