/*
SDMOn:
in-memory service discovery daemon
1. Accept incoming requests to register system with IP address and port
2. Maintain a map of systems to IP Addresses and ports
3. Give simple API to get a list of all systems
LATER:
Allow for health checks
*/

use axum::{
    body::Bytes, 
    error_handling::HandleErrorLayer, 
    extract::{Path, State, Json}, 
    http::StatusCode, 
    response::IntoResponse, 
    routing::{post, get}, 
    Router
};
use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use tower::{BoxError, ServiceBuilder};
use tower_http::{
    trace::TraceLayer
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use serde::Deserialize;
#[derive(Deserialize)]
struct RegisterNodeRequest {
    ip: String,
    port: u16, 
}


#[derive(Default)]
struct AppState {
    db: HashMap<String, Bytes>,
}

type SharedState = Arc<RwLock<AppState>>;

async fn register_node(
    State(state): State<SharedState>,
    Json(payload): Json<RegisterNodeRequest>,
) -> Result<Bytes, StatusCode> {

    // Code inside here can access SharedState

    println!("Data: {:?}", (&payload.port, &payload.ip));
    let new_ip = payload.ip.clone();

    state.write().unwrap().db.insert(new_ip, Bytes::new());

    Ok(Bytes::new())
}

async fn list_keys(State(state): State<SharedState>) -> String {
    let db = &state.read().unwrap().db;

    db.keys()
        .map(|key| key.to_string())
        .collect::<Vec<String>>()
        .join("\n")
}

async fn handle_error(error: BoxError) -> impl IntoResponse {
    if error.is::<tower::timeout::error::Elapsed>() {
        return (StatusCode::REQUEST_TIMEOUT, Cow::from("request timed out"));
    }

    if error.is::<tower::load_shed::error::Overloaded>() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Cow::from("service is overloaded, try again later"),
        );
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Cow::from(format!("Unhandled internal error: {error}")),
    )
}

#[tokio::main]
async fn main() {
    // add tracing support
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!("{}=debug,tower_http=debug", env!("CARGO_CRATE_NAME")).into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let shared_state = SharedState::default();

    // Build our application by composing routes
    let app = Router::new()
        .route( "/register", post(register_node))
        .route("/listkeys", get(list_keys))
        // Add middleware to all routes
        .layer(
            ServiceBuilder::new()
                // Handle errors from middleware
                .layer(HandleErrorLayer::new(handle_error))
                .load_shed()
                .concurrency_limit(1024)
                .timeout(Duration::from_secs(10))
                .layer(TraceLayer::new_for_http()),
        )
        .with_state(Arc::clone(&shared_state));

    // Run our app with hyper
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
