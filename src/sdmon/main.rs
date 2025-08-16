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
    Router,
    error_handling::HandleErrorLayer,
    extract::{Json, State},
    http::StatusCode,
    routing::{get, post},
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tupac_rs::common::{NodeInfo, RegisterNodeRequest, handle_error};
use uuid::Uuid;

#[derive(Default)]
struct AppState {
    db: HashMap<Uuid, NodeInfo>,
}

type SharedState = Arc<RwLock<AppState>>;

async fn register_node(
    State(state): State<SharedState>,
    Json(payload): Json<RegisterNodeRequest>,
) -> Result<String, StatusCode> {
    println!("Data: {:?}", (&payload.port, &payload.ip));

    // Create a new UUID for the node
    let node_id = Uuid::new_v4();
    let node_id_internal = node_id.clone();
    let node_info = NodeInfo {
        ip: payload.ip.clone(),
        port: payload.port,
        alive: true,
        id: node_id_internal,
        node_type: payload.node_type.clone(),
    };

    // mutate the internal state to store the node info
    state.write().unwrap().db.insert(node_id, node_info);

    // return a successful status code
    Ok(node_id.to_string())
}

async fn list_keys(State(state): State<SharedState>) -> Json<Vec<NodeInfo>> {
    let db = &state.read().unwrap().db;
    let db_vec = db.values().cloned().collect::<Vec<NodeInfo>>();
    axum::Json::from(db_vec)
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
        .route("/register", post(register_node))
        .route("/listnodes", get(list_keys))
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
