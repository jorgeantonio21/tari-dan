//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::Extension,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    routing::post,
    Router,
};
use axum_jrpc::{
    error::{JsonRpcError, JsonRpcErrorReason},
    JrpcResult,
    JsonRpcAnswer,
    JsonRpcExtractor,
    JsonRpcResponse,
};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use tari_dan_wallet_sdk::apis::jwt::JwtApiError;
use tari_shutdown::ShutdownSignal;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use super::handlers::HandlerContext;
use crate::handlers::{accounts, confidential, error::HandlerError, keys, rpc, transaction, webrtc, Handler};

const LOG_TARGET: &str = "tari::dan_wallet_daemon::json_rpc";

// We need to extract the token, because the first call is without any token. So we don't have to have two handlers.
async fn extract_token<B>(mut request: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let mut token_ext = None;
    if let Some(token) = request.headers().get("authorization") {
        if let Ok(token) = token.to_str() {
            if let Some(token) = token.strip_prefix("Bearer ") {
                token_ext = Some(token.to_string());
            }
        }
    }
    request.extensions_mut().insert::<Option<String>>(token_ext);
    let response = next.run(request).await;
    Ok(response)
}

pub async fn listen(
    preferred_address: SocketAddr,
    signaling_server_address: SocketAddr,
    context: HandlerContext,
    shutdown_signal: ShutdownSignal,
) -> Result<(), anyhow::Error> {
    let router = Router::new()
        .route("/", post(handler))
        .route("/json_rpc", post(handler))
        // TODO: Get these traces to work
        .layer(TraceLayer::new_for_http())
        .layer(Extension(Arc::new(context)))
        .layer(Extension((preferred_address,signaling_server_address)))
        .layer(Extension(Arc::new(shutdown_signal.clone())))
        .layer(CorsLayer::permissive())
        .layer(axum::middleware::from_fn(extract_token));

    let server = axum::Server::try_bind(&preferred_address)?;
    let server = server.serve(router.into_make_service());
    info!(target: LOG_TARGET, "🌐 JSON-RPC listening on {}", server.local_addr());
    let server = server.with_graceful_shutdown(shutdown_signal);
    server.await?;

    info!(target: LOG_TARGET, "💤 Stopping JSON-RPC");
    Ok(())
}

async fn handler(
    Extension(context): Extension<Arc<HandlerContext>>,
    Extension(addresses): Extension<(SocketAddr, SocketAddr)>,
    Extension(shutdown_signal): Extension<Arc<ShutdownSignal>>,
    Extension(token): Extension<Option<String>>,
    value: JsonRpcExtractor,
) -> JrpcResult {
    info!(target: LOG_TARGET, "🌐 JSON-RPC request: {}", value.method);

    dbg!(&value);

    match value.method.as_str().split_once('.') {
        Some(("auth", method)) => match method {
            "request" => call_handler(context, value, token, rpc::handle_login_request).await,
            "accept" => call_handler(context, value, token, rpc::handle_login_accept).await,
            "deny" => call_handler(context, value, token, rpc::handle_login_deny).await,
            _ => Ok(value.method_not_found(&value.method)),
        },
        Some(("webrtc", "start")) => webrtc::handle_start(context, value, token, shutdown_signal, addresses),
        Some(("rpc", "discover")) => call_handler(context, value, token, rpc::handle_discover).await,
        Some(("keys", method)) => match method {
            "create" => call_handler(context, value, token, keys::handle_create).await,
            "list" => call_handler(context, value, token, keys::handle_list).await,
            "set_active" => call_handler(context, value, token, keys::handle_set_active).await,
            _ => Ok(value.method_not_found(&value.method)),
        },
        Some(("transactions", method)) => match method {
            "submit" => call_handler(context, value, token, transaction::handle_submit).await,
            "get" => call_handler(context, value, token, transaction::handle_get).await,
            "get_result" => call_handler(context, value, token, transaction::handle_get_result).await,
            "wait_result" => call_handler(context, value, token, transaction::handle_wait_result).await,
            _ => Ok(value.method_not_found(&value.method)),
        },
        Some(("accounts", method)) => match method {
            "reveal_funds" => call_handler(context, value, token, accounts::handle_reveal_funds).await,
            "claim_burn" => call_handler(context, value, token, accounts::handle_claim_burn).await,
            "create" => call_handler(context, value, token, accounts::handle_create).await,
            "list" => call_handler(context, value, token, accounts::handle_list).await,
            "get_balances" => call_handler(context, value, token, accounts::handle_get_balances).await,
            "invoke" => call_handler(context, value, token, accounts::handle_invoke).await,
            "get" => call_handler(context, value, token, accounts::handle_get).await,
            "get_default" => call_handler(context, value, token, accounts::handle_get_default).await,
            "confidential_transfer" => {
                call_handler(context, value, token, accounts::handle_confidential_transfer).await
            },
            "set_default" => call_handler(context, value, token, accounts::handle_set_default).await,
            "create_free_test_coins" => {
                call_handler(context, value, token, accounts::handle_create_free_test_coins).await
            },
            _ => Ok(value.method_not_found(&value.method)),
        },
        Some(("confidential", method)) => match method {
            "create_transfer_proof" => {
                call_handler(context, value, token, confidential::handle_create_transfer_proof).await
            },
            "finalize" => call_handler(context, value, token, confidential::handle_finalize_transfer).await,
            "cancel" => call_handler(context, value, token, confidential::handle_cancel_transfer).await,
            "create_output_proof" => {
                call_handler(context, value, token, confidential::handle_create_output_proof).await
            },
            _ => Ok(value.method_not_found(&value.method)),
        },
        _ => Ok(value.method_not_found(&value.method)),
    }
}

async fn call_handler<H, TReq, TResp>(
    context: Arc<HandlerContext>,
    value: JsonRpcExtractor,
    token: Option<String>,
    mut handler: H,
) -> JrpcResult
where
    TReq: DeserializeOwned,
    TResp: Serialize,
    H: for<'a> Handler<'a, TReq, Response = TResp>,
{
    let answer_id = value.get_answer_id();
    let resp = handler
        .handle(
            &context,
            token,
            value.parse_params().map_err(|e| {
                match &e.result {
                    JsonRpcAnswer::Result(_) => {
                        unreachable!("parse_params should not return a result")
                    },
                    JsonRpcAnswer::Error(e) => {
                        warn!(target: LOG_TARGET, "🌐 JSON-RPC params error: {}", e);
                    },
                }
                e
            })?,
        )
        .await
        .map_err(|e| resolve_handler_error(answer_id, &e))?;
    Ok(JsonRpcResponse::success(answer_id, resp))
}

fn resolve_handler_error(answer_id: i64, e: &HandlerError) -> JsonRpcResponse {
    match e {
        HandlerError::Anyhow(e) => resolve_any_error(answer_id, e),
        HandlerError::NotFound => JsonRpcResponse::error(
            answer_id,
            JsonRpcError::new(JsonRpcErrorReason::ApplicationError(404), e.to_string(), json!({})),
        ),
    }
}

fn resolve_any_error(answer_id: i64, e: &anyhow::Error) -> JsonRpcResponse {
    warn!(target: LOG_TARGET, "🌐 JSON-RPC error: {}", e);
    if let Some(handler_err) = e.downcast_ref::<HandlerError>() {
        return resolve_handler_error(answer_id, handler_err);
    }

    if let Some(error) = e.downcast_ref::<JwtApiError>() {
        JsonRpcResponse::error(
            answer_id,
            JsonRpcError::new(
                JsonRpcErrorReason::ApplicationError(401),
                error.to_string(),
                serde_json::Value::Null,
            ),
        )
    } else {
        JsonRpcResponse::error(
            answer_id,
            JsonRpcError::new(JsonRpcErrorReason::ApplicationError(500), e.to_string(), json!({})),
        )
    }
}
