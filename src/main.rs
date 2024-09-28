use axum::{
    extract::{Extension, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use bytes::Bytes;
use futures::{stream::FuturesUnordered, TryStreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{error::Error, sync::Arc, time::Duration, time::Instant};
use tokio::{sync::OnceCell, sync::Semaphore, time::timeout};
const MAX_CONCURRENT_REQUESTS: usize = 100;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const OVERALL_TIMEOUT: Duration = Duration::from_secs(10);
use tracing::log;
#[derive(Debug, Deserialize)]
struct ClusterMember {
    rpc: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    endpoints: Vec<String>,
    use_cluster_nodes: bool,
}

#[derive(Debug)]
pub struct SuccessResponse {
    pub endpoint: String,
    pub rtt: u128,
    //   pub status: StatusCode,
    pub headers: reqwest::header::HeaderMap,
    pub body: bytes::Bytes,
}

#[derive(Debug)]
struct UnsupportedMethodError {
    endpoint: String,
    method: String,
}

impl std::fmt::Display for UnsupportedMethodError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "method {} is not supported by endpoint {}",
            self.method, self.endpoint
        )
    }
}

impl Error for UnsupportedMethodError {}

static CONFIG: OnceCell<Arc<Config>> = OnceCell::const_new();

async fn load_config() -> Result<Arc<Config>, Box<dyn Error>> {
    let content = tokio::fs::read_to_string("config.yaml").await?;
    let config: Config = serde_yaml::from_str(&content)?;
    Ok(Arc::new(config))
}

async fn get_cluster_endpoints(client: &Client) -> Result<Vec<String>, Box<dyn Error>> {
    let endpoint = "https://api.mainnet-beta.solana.com";
    let json_body = r#"{ "jsonrpc": "2.0", "id": 1, "method": "getClusterNodes"}"#;

    let response: serde_json::Value = client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .body(json_body)
        .send()
        .await?
        .json()
        .await?;

    let cluster_members: Vec<ClusterMember> = serde_json::from_value(response["result"].clone())?;
    Ok(cluster_members
        .into_iter()
        .map(|member| format!("http://{}", member.rpc))
        .collect())
}

async fn load_endpoints(client: &Client) -> Result<Vec<String>, Box<dyn Error>> {
    let config = CONFIG
        .get_or_try_init(|| async { load_config().await })
        .await?;
    let mut endpoints = config.endpoints.clone();

    if config.use_cluster_nodes {
        match get_cluster_endpoints(client).await {
            Ok(cluster_endpoints) => {
                endpoints.extend(cluster_endpoints);
            }
            Err(err) => {
                log::error!("Error loading cluster endpoints: {}", err);
            }
        }
    }

    Ok(endpoints)
}
async fn make_request(
    client: &Client,
    ctx: &Arc<tokio::sync::Semaphore>,
    endpoint: String,
    req_body: Option<Value>, // Optional for flexibility
) -> Result<SuccessResponse, Box<dyn Error + Send + Sync>> {
    let _permit = ctx.acquire().await;

    let start_time = Instant::now();

    // Use timeout for the request
    let response = timeout(
        REQUEST_TIMEOUT,
        match req_body {
            Some(body) => client.post(&endpoint).json(&body).send(),
            None => client.post(&endpoint).body(Vec::new()).send(), // Handle empty body
        },
    )
    .await??;

    let rtt = start_time.elapsed().as_millis();
    let status = response.status();
    let body = response.bytes().await?;

    if status.is_success() {
        Ok(SuccessResponse {
            endpoint,
            rtt,
            body,
            // pass empty headers
            headers: reqwest::header::HeaderMap::new(),
        })
    } else {
        Err(format!("Request failed with status code: {}", status).into())
    }
}

async fn vulcana(
    ctx: Arc<tokio::sync::Semaphore>,
    client: Arc<Client>,
    endpoints: Vec<String>,
    body: Option<Value>,
) -> Result<SuccessResponse, Box<dyn Error + Send + Sync>> {
    let mut futures = FuturesUnordered::new();

    for endpoint in endpoints {
        let client = client.clone();
        let body = body.clone();
        let ctx = Arc::clone(&ctx);

        futures.push(tokio::spawn(async move {
            make_request(&client, &ctx, endpoint, body).await
        }));
    }

    // Wait for the first successful response
    while let Some(result) = futures.try_next().await? {
        match result {
            Ok(response) => {
                // Check if the response is of type SuccessResponse
                return Ok(response); // Return the successful response
            }
            Err(e) => return Err(format!("Task failed: {}", e).into()),
        }
    }

    Err("No successful responses".into())
}
async fn proxy(
    State(client): State<Arc<Client>>,
    Extension(ctx): Extension<Arc<Semaphore>>,
    body: Bytes,
) -> impl IntoResponse {
    // Load endpoints
    let endpoints = match load_endpoints(&client).await {
        Ok(e) => e,
        Err(err) => {
            eprintln!("Error loading endpoints: {:?}", err);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response();
        }
    };

    // vulcana request to all endpoints
    let body_value = Some(serde_json::from_slice(&body).unwrap_or(Value::Null)); // Parse the body into Value if possible

    match vulcana(ctx.clone(), client.clone(), endpoints, body_value).await {
        Ok(response) => {
            // Prepare response headers
            let mut headers = HeaderMap::new();
            for (name, value) in response.headers.iter() {
                headers.insert(name, value.clone());
            }

            // Set CORS headers
            headers.insert("Access-Control-Allow-Origin", "*".parse().unwrap());
            headers.insert(
                "Access-Control-Allow-Methods",
                "GET, POST, OPTIONS".parse().unwrap(),
            );
            headers.insert(
                "Access-Control-Allow-Headers",
                "Content-Type, solana-client".parse().unwrap(),
            );
            // Display Response endpoint,time & body;

            println!("Retunrned by Endpoint: {}", response.endpoint);
            print!("in RTT: {}ms", response.rtt);

            // Display response body
            if let Ok(body) = std::str::from_utf8(&response.body) {
                println!("Body: {}", body);
            }

            // Return response body
            (StatusCode::OK, headers, response.body).into_response()
        }
        Err(e) => {
            eprintln!("All requests failed or timed out: {}", e);
            (StatusCode::BAD_GATEWAY, "Bad Gateway").into_response()
        }
    }
}
#[tokio::main]
async fn main() {
    // Set up the HTTP client
    let client = Arc::new(
        Client::builder()
            .pool_max_idle_per_host(0)
            .build()
            .expect("Failed to create HTTP client"),
    );

    // Set up the semaphore for limiting concurrency
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));

    // Define the router
    let app = Router::new()
        .route("/vulcana", post(proxy))
        .with_state(client.clone())
        .layer(Extension(semaphore));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("Failed to bind TcpListener");

    println!("Starting the Axum web server on 0.0.0.0:8080");

    axum::serve(listener, app).await.unwrap();
}
/*
async fn make_request(
    client: &Client,
    ctx: &Arc<tokio::sync::Semaphore>,
    endpoint: String,
    req_body: Value,
    method: String,
) -> Result<SuccessResponse, Box<dyn Error + Send + Sync>> {
    let _permit = ctx.acquire().await;

    let start_time = Instant::now();
    let response = client.post(&endpoint).json(&req_body).send().await?;

    let rtt = start_time.elapsed().as_millis();
    let status = response.status();

    let body = response.bytes().await?;

    if status.is_success() {
        let json_response: Value = serde_json::from_slice(&body)?;
        if let Some(error_obj) = json_response.get("error") {
            if let Some(err_msg) = error_obj.get("message").and_then(Value::as_str) {
                if err_msg == "RPC Method not supported" {
                    return Err(Box::new(UnsupportedMethodError { endpoint, method }));
                }
            }
        }
    }

    Ok(SuccessResponse {
        endpoint,
        rtt,
        body,
    })
}
    */
/*
async fn vulcana(
    ctx: Arc<tokio::sync::Semaphore>,
    endpoints: Arc<Vec<String>>,
    method: String,
    req_body: Value,
) -> Result<SuccessResponse, Box<dyn Error + Send + Sync>> {
    let (response_tx, mut response_rx) = mpsc::channel::<SuccessResponse>(endpoints.len());
    let (error_tx, mut error_rx) = mpsc::channel::<Box<dyn Error + Send + Sync>>(endpoints.len());

    let client = Arc::new(Client::new());

    for endpoint in endpoints.iter() {
        let response_tx = response_tx.clone();
        let error_tx = error_tx.clone();
        let req_body = req_body.clone();
        let ctx = Arc::clone(&ctx);
        let client = Arc::clone(&client);
        let endpoint = endpoint.clone();
        let method = method.clone();

        tokio::spawn(async move {
            match make_request(&client, &ctx, endpoint, req_body, method).await {
                Ok(success) => {
                    let _ = response_tx.send(success).await;
                }
                Err(err) => {
                    let _ = error_tx.send(err).await;
                }
            }
        });
    }

    let mut last_err: Option<Box<dyn Error + Send + Sync>> = None;
    let mut unsupported_count = 0;

    loop {
        tokio::select! {
            Some(success) = response_rx.recv() => return Ok(success),
            Some(err) = error_rx.recv() => {
                if err.is::<UnsupportedMethodError>() {
                    unsupported_count += 1;
                }
                last_err = Some(err);
            }
            else => break,
        }
    }

    if unsupported_count == endpoints.len() {
        return Err("Method not supported by any endpoint".into());
    }

    if let Some(last_err) = last_err {
        return Err(last_err);
    }

    Err("No response from any endpoint".into())
}
    */
