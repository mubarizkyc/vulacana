use axum::http::{HeaderMap, StatusCode};

use reqwest::{Client, Response};
use serde_json::Value;
use std::{error::Error, future::Future, pin::Pin, sync::Arc, time::Duration};
use tokio::sync::Semaphore;
use tokio::time::Instant;

use tracing::log;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct Config {
    pub endpoints: Vec<String>,
}

#[derive(Debug)]
pub struct SuccessResponse {
    pub status: StatusCode,
    pub body: String,
    pub headers: HeaderMap,
}
pub async fn make_request(
    ctx: Arc<Semaphore>,
    client: Arc<Client>,
    endpoint: &str,
    body: &Option<Value>,
) -> Result<Response, Box<dyn Error + Send + Sync>> {
    let _permit = ctx.acquire().await.unwrap();

    //   log::info!("Sending request to endpoint: {}", endpoint);

    let request_builder = client.post(endpoint);
    let request = if let Some(body) = body {
        //  log::info!("Request body: {:?}", body);
        request_builder.json(&body).build()?
    } else {
        request_builder.build()?
    };

    let response = tokio::time::timeout(REQUEST_TIMEOUT, client.execute(request)).await??;
    /*
    log::info!(
        "Response from {}: Status: {}, Headers: {:?}",
        endpoint,
        response.status(),
        response.headers()
    );
    */

    Ok(response)
}

pub async fn vulacana(
    ctx: Arc<Semaphore>,
    client: Arc<Client>,
    endpoints: Vec<String>,
    body: Option<Value>,
) -> Result<SuccessResponse, Box<dyn Error + Send + Sync>> {
    let futures: Vec<_> = endpoints
        .into_iter()
        .map(|endpoint| {
            let ctx = ctx.clone();
            let client = client.clone();
            let body = body.clone();

            Box::pin(async move {
                let start_time = Instant::now();
                let response = make_request(ctx, client, &endpoint, &body).await?;

                if response.status().is_success() {
                    let status = response.status();
                    let headers = response.headers().clone();
                    let body = response.text().await?;
                    let duration = start_time.elapsed();
                    log::info!(
                        "Successfully received response from {}: Status: {}, Duration: {:?}",
                        endpoint,
                        status,
                        duration
                    );

                    Ok(SuccessResponse {
                        status,
                        headers,
                        body,
                    })
                } else {
                    // Explicit return for error case
                    Err(format!(
                        "Request to {} failed with status: {}",
                        endpoint,
                        response.status()
                    )
                    .into())
                }
            })
                as Pin<
                    Box<
                        dyn Future<Output = Result<SuccessResponse, Box<dyn Error + Send + Sync>>>
                            + Send,
                    >,
                >
        })
        .collect();

    // Use select_ok to get the first successful response
    match futures::future::select_ok(futures).await {
        Ok((success_response, _)) => {
            /*
            log::info!(
                "Successfully received response: Status: {}, Body: {}",
                success_response.status,
                success_response.body
            );
            */
            Ok(success_response) // Return the successful response
        }
        Err(e) => {
            log::error!("All requests failed: {}", e);
            Err("All requests failed".into()) // Return error if all requests fail
        }
    }
}
