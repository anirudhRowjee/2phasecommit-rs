use axum::{
    Router,
    error_handling::HandleErrorLayer,
    extract::State,
    routing::{get, post},
};
use clap::Parser;
use std::net::Ipv4Addr;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tupac_rs::common::{NodeInfo, handle_error, register_with_sdmon};
use uuid::Uuid;

#[derive(Default, Clone, Debug)]
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

// WARN: GPT Slop up ahead
fn write_record_uncommitted(
    state: &mut AppState,
    key: String,
    value: String,
    txn_id: Uuid,
) -> Result<(), String> {
    if state.db.contains_key(&txn_id) {
        return Err("Transaction ID already exists".to_string());
    }
    let kv_pair = KVPair {
        key,
        value,
        committed: false,
        txn_id,
    };
    state.db.insert(txn_id, kv_pair);
    Ok(())
}

fn commit_record(state: &mut AppState, txn_id: Uuid) -> Result<(), String> {
    if let Some(kv_pair) = state.db.get_mut(&txn_id) {
        kv_pair.committed = true;
        Ok(())
    } else {
        Err("Transaction ID not found".to_string())
    }
}

/*
TODO
1. Create request handlers for writing and committing records based on txn ID
2. Wire up request handlers for writing and committing records with local state
*/

type SharedState = Arc<RwLock<AppState>>;

async fn ping(State(state): State<SharedState>) -> String {
    println!("Received ping request");
    String::from("pong")
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
        .route("/ping", get(ping))
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
