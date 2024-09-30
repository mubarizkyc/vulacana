pub mod vulacana;
use crate::vulacana::*;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use reqwest::Client;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tracing::log;

async fn proxy(
    State((config, client)): State<(Arc<Config>, Arc<Client>)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let ctx = Arc::new(Semaphore::new(config.endpoints.len()));

    // Validate headers
    //  log::info!("Received request with headers: {:?}", headers);
    //log::info!("Received request body: {:?}", body);

    match vulacana(ctx, client, config.endpoints.clone(), Some(body)).await {
        Ok(success_response) => {
            let mut response_headers = success_response.headers;
            response_headers.insert("Access-Control-Allow-Origin", " *".parse().unwrap());
            response_headers.insert("Access-Control-Allow-Methods", "POST, GET".parse().unwrap());

            //           log::info!("Returning successful response: {}", success_response.body);

            (
                success_response.status,
                response_headers,
                success_response.body,
            )
                .into_response()
        }
        Err(e) => {
            log::error!("Proxy error: {}", e);
            // Return error message in response
            (StatusCode::BAD_GATEWAY, format!("Bad Gateway: {}", e)).into_response()
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let client = Client::builder()
        .pool_max_idle_per_host(0)
        .pool_idle_timeout(None)
        .build()
        .expect("Failed to build HTTP client");

    let client = Arc::new(client);
    let config = Arc::new(Config {
        endpoints: vec![
            "https://api.mainnet-beta.solana.com".to_string(),
            "https://mainnet.helius-rpc.com/?api-key=a61bb4c9-2102-40b6-a4ac-533628cb6617"
                .to_string(),
            "https://rpc.hellomoon.io/fa2b8d79-0432-4059-9f7f-750b0df82a36".to_string(),
        ],
    });

    let app = Router::new()
        .route("/", post(proxy))
        .layer(
            ServiceBuilder::new().layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            ),
        )
        .with_state((config, client));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Failed to bind TcpListener");

    println!("Starting the Axum web server on 0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}
