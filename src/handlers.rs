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
    respond(200, payloads::payment_details(&state.cfg, &quote))
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
    if method != state.cfg.method.spec_name() || asset != state.cfg.asset {
        return respond(
            400,
            payloads::error(
                400,
                &format!(
                    "Method/asset not available (expected {}/{}, got {method}/{asset})",
                    state.cfg.method, state.cfg.asset
                ),
                "Bad Request",
            ),
        );
    }

    respond(
        200,
        payloads::transaction_details(&state.cfg, &quote, &state.uri),
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

    let method = query.get("method").map(String::as_str).unwrap_or("");
    if method != state.cfg.method.spec_name() {
        return respond(
            400,
            payloads::error(
                400,
                &format!("Wrong method (expected '{}', got '{method}')", state.cfg.method),
                "Bad Request",
            ),
        );
    }

    let expected = state.cfg.method.proof_param();
    let other = if expected == "hex" { "tx" } else { "hex" };
    if query.contains_key(other) {
        return respond(
            400,
            payloads::error(
                400,
                &format!(
                    "Method {} expects the '{expected}' parameter, got '{other}'",
                    state.cfg.method
                ),
                "Bad Request",
            ),
        );
    }
    let Some(proof) = query.get(expected).filter(|p| !p.is_empty()) else {
        return respond(
            400,
            payloads::error(
                400,
                &format!("Missing '{expected}' parameter for method {}", state.cfg.method),
                "Bad Request",
            ),
        );
    };

    println!();
    println!("=== PAYMENT PROOF RECEIVED ({}) ===", state.cfg.method);
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
    use crate::cli::Config;
    use crate::method::Method;
    use crate::payment_uri::build_uri;

    fn xmr_state() -> web::Data<AppState> {
        let cfg = Config::for_tests(Method::Monero, "XMR", "4AdUmoney", "0.005");
        let uri = build_uri(&cfg).unwrap();
        web::Data::new(AppState::new(cfg, uri))
    }

    fn nmc_state() -> web::Data<AppState> {
        let cfg = Config::for_tests(
            Method::Namecoin,
            "NMC",
            "N2pGWAh65TWpWmEFrFssRQkQubbczJSKi9",
            "0.5",
        );
        let uri = build_uri(&cfg).unwrap();
        web::Data::new(AppState::new(cfg, uri))
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
    async fn payment_details_carries_quote_and_transfer_amounts() {
        let state = xmr_state();
        let app = app!(state.clone());

        let (status, body) = get(&app, "/v1/lnurlp/pl_mock01").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["displayName"], "OCP Mock Shop");
        assert_eq!(body["callback"], "http://127.0.0.1:8080/v1/lnurlp/cb/pl_mock01");
        assert_eq!(body["quote"]["id"], state.quote.lock().unwrap().quote_id);
        assert_eq!(body["transferAmounts"][0]["method"], "Monero");
        assert_eq!(body["transferAmounts"][0]["assets"][0]["asset"], "XMR");
        assert_eq!(body["transferAmounts"][0]["assets"][0]["amount"], "0.005");
    }

    #[actix_web::test]
    async fn simplified_flow_returns_transaction_details() {
        let state = xmr_state();
        let quote_id = state.quote.lock().unwrap().quote_id.clone();
        let app = app!(state);

        let (status, body) = get(
            &app,
            &format!("/v1/lnurlp/pl_mock01?quote={quote_id}&method=Monero&asset=XMR"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["blockchain"], "Monero");
        assert_eq!(body["uri"], "monero:4AdUmoney?tx_amount=0.005");
        // Hash-family hint: wallet broadcasts, no "as HEX" wording.
        assert!(!body["hint"].as_str().unwrap().contains("as HEX"));
    }

    #[actix_web::test]
    async fn callback_rejects_proof_parameters() {
        let state = xmr_state();
        let quote_id = state.quote.lock().unwrap().quote_id.clone();
        let app = app!(state);

        let (status, body) = get(
            &app,
            &format!("/v1/lnurlp/cb/pl_mock01?quote={quote_id}&method=Monero&tx=abc"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["message"].as_str().unwrap().contains("/tx/ endpoint"));
    }

    #[actix_web::test]
    async fn proof_accepted_on_both_tx_path_ids() {
        let state = xmr_state();
        let quote_id = state.quote.lock().unwrap().quote_id.clone();
        let payment_id = state.quote.lock().unwrap().payment_id.clone();
        let app = app!(state);

        let (status, _) = get(
            &app,
            &format!("/v1/lnurlp/tx/pl_mock01?quote={quote_id}&method=Monero&tx=hash1"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = get(
            &app,
            &format!("/v1/lnurlp/tx/{payment_id}?quote={quote_id}&method=Monero&tx=hash2"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "Complete");
    }

    #[actix_web::test]
    async fn proof_validates_quote_and_param_name() {
        let state = nmc_state();
        let quote_id = state.quote.lock().unwrap().quote_id.clone();
        let app = app!(state);

        let (status, body) = get(
            &app,
            &format!("/v1/lnurlp/tx/pl_mock01?quote={quote_id}&method=Namecoin&tx=abc"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["message"].as_str().unwrap().contains("'hex'"));

        let (status, _) = get(
            &app,
            "/v1/lnurlp/tx/pl_mock01?quote=plq_bogus&method=Namecoin&hex=beef",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = get(
            &app,
            &format!("/v1/lnurlp/tx/pl_mock01?quote={quote_id}&method=Namecoin&hex=beef"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[actix_web::test]
    async fn no_pending_serves_spec_404_shape() {
        let mut cfg = Config::for_tests(Method::Monero, "XMR", "4AdUmoney", "0.005");
        cfg.no_pending = true;
        let uri = build_uri(&cfg).unwrap();
        let app = app!(web::Data::new(AppState::new(cfg, uri)));

        let (status, body) = get(&app, "/v1/lnurlp/pl_mock01").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["message"], "No pending payment found");
        assert_eq!(body["statusCode"], 404);
    }
}
