//! HTTP route handlers implementing the OpenCryptoPay provider API.

use std::collections::HashMap;

use actix_web::http::StatusCode;
use actix_web::{get, web, HttpRequest, HttpResponse};
use serde_json::{json, Value};

use crate::payloads;
use crate::state::{AppState, Quote};

pub fn routes(service: &mut web::ServiceConfig) {
    service
        .service(payment_details)
        .service(transaction_details_cb)
        .service(transaction_proof)
        .service(payment_link_page)
        .default_service(web::route().to(not_found));
}

fn log_request(req: &HttpRequest) {
    println!("← {} {}", req.method(), req.uri());
}

fn respond(code: u16, body: Value) -> HttpResponse {
    println!("→ HTTP {code}");
    HttpResponse::build(StatusCode::from_u16(code).expect("static status codes")).json(body)
}

/// Payment details (step 2), or transaction details when `method` & `asset`
/// are passed (the spec's simplified flow).
#[get("/v1/lnurlp/{id}")]
async fn payment_details(
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<HashMap<String, String>>,
    state: web::Data<AppState>,
) -> HttpResponse {
    log_request(&req);
    if *path != state.cfg.id {
        return respond(
            404,
            payloads::error(404, "Unknown payment link", "Not Found"),
        );
    }
    if state.cfg.no_pending {
        return respond(404, payloads::no_pending(&state.cfg));
    }
    if query.contains_key("method") && query.contains_key("asset") {
        return transaction_details_response(&state, &query, /*quote_required=*/ false);
    }
    state.refresh_quote();
    let quote = state.quote.lock().unwrap();
    respond(
        200,
        payloads::payment_details(&state.cfg, &quote, &state.coins),
    )
}

/// Transaction details via the callback URL (step 3).
#[get("/v1/lnurlp/cb/{id}")]
async fn transaction_details_cb(
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<HashMap<String, String>>,
    state: web::Data<AppState>,
) -> HttpResponse {
    log_request(&req);
    if *path != state.cfg.id {
        return respond(
            404,
            payloads::error(404, "Unknown payment link", "Not Found"),
        );
    }
    if state.cfg.no_pending {
        return respond(404, payloads::no_pending(&state.cfg));
    }
    if ["hex", "tx", "sender"].iter().any(|p| query.contains_key(*p)) {
        return respond(
            400,
            payloads::error(
                400,
                "Transaction proofs go to the /tx/ endpoint, not the /cb/ callback \
                 (construct it by replacing /cb with /tx, see the hint)",
                "Bad Request",
            ),
        );
    }
    transaction_details_response(&state, &query, /*quote_required=*/ true)
}

fn transaction_details_response(
    state: &AppState,
    query: &HashMap<String, String>,
    quote_required: bool,
) -> HttpResponse {
    let quote = state.quote.lock().unwrap();
    if let Err(response) = check_quote(&quote, query, quote_required) {
        return response;
    }

    let method = query.get("method").map(String::as_str).unwrap_or("");
    let asset = query.get("asset").map(String::as_str).unwrap_or("");
    let Some(coin) = state.find_coin(method, asset) else {
        return respond(
            400,
            payloads::error(
                400,
                &format!(
                    "Method/asset {method}/{asset} not available (supported: {})",
                    state.supported_pairs()
                ),
                "Bad Request",
            ),
        );
    };

    respond(
        200,
        payloads::transaction_details(&state.cfg, &quote, coin),
    )
}

/// Transaction proof submission (step 4, `/tx/` endpoint).
#[get("/v1/lnurlp/tx/{payment}")]
async fn transaction_proof(
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<HashMap<String, String>>,
    state: web::Data<AppState>,
) -> HttpResponse {
    log_request(&req);
    let quote = state.quote.lock().unwrap();

    // The spec advertises the payment id (plp_...) in the hint, but its
    // construction rule — replace /cb with /tx in the callback URL — yields
    // the payment link id (pl_...). Accept both.
    if *path != quote.payment_id && *path != state.cfg.id {
        return respond(
            404,
            payloads::error(
                404,
                &format!(
                    "Unknown payment '{}' (expected '{}' or '{}')",
                    path, quote.payment_id, state.cfg.id
                ),
                "Not Found",
            ),
        );
    }

    if let Err(response) = check_quote(&quote, &query, /*required=*/ true) {
        return response;
    }

    let method_param = query.get("method").map(String::as_str).unwrap_or("");
    let Some(method) = state.find_method(method_param) else {
        return respond(
            400,
            payloads::error(
                400,
                &format!(
                    "Wrong method '{method_param}' (supported: {})",
                    state.supported_pairs()
                ),
                "Bad Request",
            ),
        );
    };

    let expected = method.proof_param();
    let other = if expected == "hex" { "tx" } else { "hex" };
    if query.contains_key(other) {
        return respond(
            400,
            payloads::error(
                400,
                &format!("Method {method} expects the '{expected}' parameter, got '{other}'"),
                "Bad Request",
            ),
        );
    }
    let Some(proof) = query.get(expected).filter(|p| !p.is_empty()) else {
        return respond(
            400,
            payloads::error(
                400,
                &format!("Missing '{expected}' parameter for method {method}"),
                "Bad Request",
            ),
        );
    };

    println!();
    println!("=== PAYMENT PROOF RECEIVED ({method}) ===");
    if expected == "hex" {
        println!("Signed transaction HEX (NOT broadcast by this mock):");
        println!("{proof}");
        println!("Broadcast it yourself to actually move the funds.");
    } else {
        println!("Transaction hash (wallet broadcast it): {proof}");
        println!("Watch for it on a block explorer / your receiving wallet.");
    }
    println!("====================================");
    println!();

    respond(200, json!({ "status": "Complete" }))
}

fn check_quote(
    quote: &Quote,
    query: &HashMap<String, String>,
    required: bool,
) -> Result<(), HttpResponse> {
    match query.get("quote") {
        Some(q) if *q != quote.quote_id => Err(respond(
            400,
            payloads::error(
                400,
                &format!("Unknown quote '{q}' (expected '{}')", quote.quote_id),
                "Bad Request",
            ),
        )),
        None if required => Err(respond(
            400,
            payloads::error(400, "Missing 'quote' parameter", "Bad Request"),
        )),
        _ => {
            if quote.is_expired() {
                Err(respond(
                    400,
                    payloads::error(400, "Quote is expired", "Bad Request"),
                ))
            } else {
                Ok(())
            }
        }
    }
}

/// The informative page the QR link points at when opened in a browser.
#[get("/pl")]
async fn payment_link_page(req: HttpRequest) -> HttpResponse {
    log_request(&req);
    respond(
        200,
        json!({
            "message": "Mock OpenCryptoPay payment link. \
                        Scan with an OpenCryptoPay-enabled wallet."
        }),
    )
}

async fn not_found(req: HttpRequest) -> HttpResponse {
    log_request(&req);
    respond(404, payloads::error(404, "Unknown path", "Not Found"))
}

#[cfg(test)]
mod tests {
    use actix_http::Request;
    use actix_web::body::MessageBody;
    use actix_web::dev::{Service, ServiceResponse};
    use actix_web::{middleware, test, App};

    use super::*;
    use crate::cli::{CoinSpec, Config};
    use crate::method::Method;

    fn multi_coin_state() -> web::Data<AppState> {
        let cfg = Config::for_tests(vec![
            CoinSpec::for_tests(Method::Monero, "XMR", "4AdUmoney", "0.005"),
            CoinSpec::for_tests(
                Method::Namecoin,
                "NMC",
                "N2pGWAh65TWpWmEFrFssRQkQubbczJSKi9",
                "0.5",
            ),
            CoinSpec::for_tests(Method::Ethereum, "ETH", "0x11", "0.0004"),
            {
                let mut usdc = CoinSpec::for_tests(Method::Ethereum, "USDC", "0x11", "1.25");
                usdc.token_contract = Some("0xA0b8".into());
                usdc.token_decimals = Some(6);
                usdc
            },
        ]);
        web::Data::new(AppState::new(cfg).unwrap())
    }

    fn quote_id(state: &AppState) -> String {
        state.quote.lock().unwrap().quote_id.clone()
    }

    macro_rules! app {
        ($state:expr) => {
            test::init_service(
                App::new()
                    .wrap(middleware::NormalizePath::trim())
                    .app_data($state)
                    .configure(routes),
            )
            .await
        };
    }

    async fn get<S, B>(app: &S, uri: &str) -> (StatusCode, Value)
    where
        S: Service<Request, Response = ServiceResponse<B>, Error = actix_web::Error>,
        B: MessageBody,
    {
        let response =
            test::call_service(app, test::TestRequest::get().uri(uri).to_request()).await;
        let status = response.status();
        (status, test::read_body_json(response).await)
    }

    #[actix_web::test]
    async fn payment_details_lists_all_methods_grouped_by_method() {
        let state = multi_coin_state();
        let app = app!(state.clone());

        let (status, body) = get(&app, "/v1/lnurlp/pl_mock01").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["quote"]["id"], quote_id(&state));

        let transfers = body["transferAmounts"].as_array().unwrap();
        // Ethereum ETH + USDC collapse into one method entry.
        assert_eq!(transfers.len(), 3);
        assert_eq!(transfers[0]["method"], "Monero");
        assert_eq!(transfers[1]["method"], "Namecoin");
        assert_eq!(transfers[2]["method"], "Ethereum");
        let eth_assets = transfers[2]["assets"].as_array().unwrap();
        assert_eq!(eth_assets.len(), 2);
        assert_eq!(eth_assets[0]["asset"], "ETH");
        assert_eq!(eth_assets[1]["asset"], "USDC");
        assert_eq!(eth_assets[1]["amount"], "1.25");
    }

    #[actix_web::test]
    async fn each_coin_gets_its_own_transaction_details() {
        let state = multi_coin_state();
        let quote = quote_id(&state);
        let app = app!(state);

        let (status, body) = get(
            &app,
            &format!("/v1/lnurlp/cb/pl_mock01?quote={quote}&method=Monero&asset=XMR"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["uri"], "monero:4AdUmoney?tx_amount=0.005");
        assert!(!body["hint"].as_str().unwrap().contains("as HEX"));

        let (status, body) = get(
            &app,
            &format!("/v1/lnurlp/cb/pl_mock01?quote={quote}&method=Ethereum&asset=USDC"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["uri"],
            "ethereum:0xA0b8@1/transfer?address=0x11&uint256=1250000"
        );
        assert!(body["hint"].as_str().unwrap().contains("as HEX"));
    }

    #[actix_web::test]
    async fn unknown_pair_lists_supported_coins() {
        let state = multi_coin_state();
        let quote = quote_id(&state);
        let app = app!(state);

        let (status, body) = get(
            &app,
            &format!("/v1/lnurlp/cb/pl_mock01?quote={quote}&method=Bitcoin&asset=BTC"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let message = body["message"].as_str().unwrap();
        assert!(message.contains("Bitcoin/BTC not available"));
        assert!(message.contains("Monero/XMR, Namecoin/NMC, Ethereum/ETH, Ethereum/USDC"));
    }

    #[actix_web::test]
    async fn simplified_flow_returns_transaction_details() {
        let state = multi_coin_state();
        let quote = quote_id(&state);
        let app = app!(state);

        let (status, body) = get(
            &app,
            &format!("/v1/lnurlp/pl_mock01?quote={quote}&method=Monero&asset=XMR"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["blockchain"], "Monero");
        assert_eq!(body["uri"], "monero:4AdUmoney?tx_amount=0.005");
    }

    #[actix_web::test]
    async fn callback_rejects_proof_parameters() {
        let state = multi_coin_state();
        let quote = quote_id(&state);
        let app = app!(state);

        let (status, body) = get(
            &app,
            &format!("/v1/lnurlp/cb/pl_mock01?quote={quote}&method=Monero&tx=abc"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["message"].as_str().unwrap().contains("/tx/ endpoint"));
    }

    #[actix_web::test]
    async fn proof_param_follows_the_submitted_method() {
        let state = multi_coin_state();
        let quote = quote_id(&state);
        let app = app!(state);

        // Monero (hash family): `tx` accepted.
        let (status, _) = get(
            &app,
            &format!("/v1/lnurlp/tx/pl_mock01?quote={quote}&method=Monero&tx=hash1"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // Namecoin (hex family): `tx` rejected, `hex` accepted.
        let (status, body) = get(
            &app,
            &format!("/v1/lnurlp/tx/pl_mock01?quote={quote}&method=Namecoin&tx=abc"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["message"].as_str().unwrap().contains("'hex'"));

        let (status, body) = get(
            &app,
            &format!("/v1/lnurlp/tx/pl_mock01?quote={quote}&method=Namecoin&hex=beef"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "Complete");

        // A method that is not configured at all.
        let (status, body) = get(
            &app,
            &format!("/v1/lnurlp/tx/pl_mock01?quote={quote}&method=Bitcoin&hex=beef"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["message"].as_str().unwrap().contains("Wrong method"));
    }

    #[actix_web::test]
    async fn proof_accepted_on_both_tx_path_ids() {
        let state = multi_coin_state();
        let quote = quote_id(&state);
        let payment_id = state.quote.lock().unwrap().payment_id.clone();
        let app = app!(state);

        // Spec construction rule: callback URL with /cb replaced by /tx (pl_ id).
        let (status, _) = get(
            &app,
            &format!("/v1/lnurlp/tx/pl_mock01?quote={quote}&method=Monero&tx=hash1"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // Hint form: the payment id (plp_...).
        let (status, _) = get(
            &app,
            &format!("/v1/lnurlp/tx/{payment_id}?quote={quote}&method=Monero&tx=hash2"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[actix_web::test]
    async fn proof_rejects_unknown_quote() {
        let state = multi_coin_state();
        let app = app!(state);

        let (status, _) = get(
            &app,
            "/v1/lnurlp/tx/pl_mock01?quote=plq_bogus&method=Monero&tx=abc",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn no_pending_serves_spec_404_shape() {
        let mut cfg = Config::for_tests(vec![CoinSpec::for_tests(
            Method::Monero,
            "XMR",
            "4AdUmoney",
            "0.005",
        )]);
        cfg.no_pending = true;
        let app = app!(web::Data::new(AppState::new(cfg).unwrap()));

        let (status, body) = get(&app, "/v1/lnurlp/pl_mock01").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["message"], "No pending payment found");
        assert_eq!(body["statusCode"], 404);
    }
}
