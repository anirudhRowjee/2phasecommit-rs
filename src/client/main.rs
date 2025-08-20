use axum::{
    Json, Router,
    error_handling::HandleErrorLayer,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tupac_rs::common::{CommitRequest, WriteUncommittedRequest, handle_error, register_with_sdmon};
use uuid::Uuid;

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
struct KVPair {
    key: String,
    value: String,
    committed: bool,
    txn_id: Uuid,
}

#[derive(Default)]
struct AppState {
    db: HashMap<Uuid, KVPair>,
}

type SharedState = Arc<RwLock<AppState>>;

async fn ping(State(_): State<SharedState>) -> String {
    println!("Received ping request");
    String::from("pong")
}

async fn write_records_uncommitted(
    State(state): State<SharedState>,
    Json(payload): Json<WriteUncommittedRequest>,
) -> Result<String, StatusCode> {
    println!("Recieved uncommitted write request: {:?}", payload);

    let write_object = KVPair {
        key: payload.key,
        value: payload.value,
        committed: false,
        txn_id: payload.txn_id,
    };
    // lock the local state and insert the object
    state
        .write()
        .unwrap()
        .db
        .insert(payload.txn_id, write_object);

    println!("Sleeping for 5s..");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    Ok("Write uncommitted successful".to_string())
}

// Function to read all the key-value pairs that have been committed
async fn read_committed(State(state): State<SharedState>) -> Result<Json<Vec<KVPair>>, StatusCode> {
    let kvpairs: Vec<KVPair> = state
        .read()
        .unwrap()
        .db
        .values()
        .cloned()
        .into_iter()
        .filter(|kvpair| kvpair.committed == true)
        .collect();
    Ok(axum::Json::from(kvpairs))
}

async fn commit_txn(
    State(state): State<SharedState>,
    Json(payload): Json<CommitRequest>,
) -> Result<String, StatusCode> {
    println!("Recieved commit request: {:?}", payload);

    {
        let mut wlock = state.write().unwrap();
        let present = wlock.db.get_mut(&payload.txn_id);
        match present {
            Some(key) => key.committed = true,
            None => return Err(StatusCode::BAD_REQUEST),
        }
    }

    Ok("Commit successful".to_string())
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    port: u16,

    #[arg(long)]
    sdmon_ip: String,

    #[arg(long)]
    sdmon_port: u16,
}

#[tokio::main]
async fn main() {
    println!("Hello, client");

    // Accept CLI Arguments for port and IP address
    let args = Args::parse();
    let port: u16 = args.port;
    let ip: String = Ipv4Addr::LOCALHOST.to_string();
    println!("Starting Client at {}:{}", ip, port);

    let client = reqwest::Client::new();
    let sdmon_ipv4 = args.sdmon_ip.parse::<Ipv4Addr>().unwrap();
    let sdmon_ip = std::net::SocketAddrV4::new(sdmon_ipv4, args.sdmon_port);
    let node_type = tupac_rs::common::NodeType::DBNode;

    let uuid = register_with_sdmon(sdmon_ip, &client, ip, port, node_type)
        .await
        .unwrap();
    println!("Registered with SDMon, UUID: {}", uuid);
    let all_nodes = tupac_rs::common::get_all_nodes_from_sdmon(sdmon_ip, &client)
        .await
        .unwrap();
    println!("All Nodes: {:?}", all_nodes);

    /*
    Start the server
    */

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
        .route("/write", post(write_records_uncommitted))
        .route("/commit", post(commit_txn))
        .route("/ping", get(ping))
        .route("/readcommitted", get(read_committed))
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
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
    // start server to accept client requests
}
