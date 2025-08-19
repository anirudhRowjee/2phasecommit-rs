use axum::{
    Router,
    body::Bytes,
    error_handling::HandleErrorLayer,
    extract::{Json, State},
    http::StatusCode,
    routing::{get, post},
};
use std::{
    collections::HashMap,
    fmt::Write,
    sync::{Arc, RwLock},
    time::Duration,
};
use std::{net::Ipv4Addr, os::macos::raw::stat};
use tokio::task::JoinSet;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tupac_rs::common::{
    CommitRequest, CoordinatorWriteRequest, NodeInfo, WriteUncommittedRequest, handle_error,
};
use uuid::Uuid;

use clap::Parser;
use tupac_rs::common::{NodeType, register_with_sdmon};

// Create an in-memory state for the coordinator
// Requests come in from the external clients, and we process them

// refresh global node state once at the beginning of every 2PC initialization

// FSM to track status of the algorithm
enum CommitState {
    TxnBegin,
    PrepareSending,
    PrepareRecieving,
    CommitDecision,
    CommitSending,
    CommitReceieving,
    TxnEnd,
}

// tracker object for in-flight transaction
struct Txn {
    state: CommitState,
}

#[derive(Default)]
struct AppState {
    db: HashMap<Uuid, NodeInfo>,
    client: Arc<reqwest::Client>,
    latest_client_nodes: Vec<NodeInfo>,
}

type SharedState = Arc<RwLock<AppState>>;

async fn ping(State(state): State<SharedState>) -> String {
    String::from("pong")
}

async fn ping_node(client: Arc<reqwest::Client>, node: NodeInfo) -> Result<(), reqwest::Error> {
    let url = format!("http://{}:{}/ping", node.ip, node.port);
    let res = client.get(url).send().await?;
    if res.status() == StatusCode::OK {
        println!("Node {} is alive", node.id);
    } else {
        println!("Node {} is not responding", node.id);
    }
    Ok(())
}

async fn ping_all_nodes(client: Arc<reqwest::Client>, client_nodes: Vec<NodeInfo>) {
    // asynchronously ping all nodes
    let mut tasks = JoinSet::new();
    for node in client_nodes {
        tasks.spawn(ping_node(Arc::clone(&client), node));
    }
    while let Some(res) = tasks.join_next().await {
        match res {
            Ok(Ok(())) => println!("Ping successful"),
            Ok(Err(e)) => eprintln!("Ping failed: {}", e),
            Err(e) => eprintln!("Task failed: {}", e),
        }
    }
}

// Function to write to a client node without committing
async fn write_no_commit(
    client: Arc<reqwest::Client>,
    node: NodeInfo,
    req: WriteUncommittedRequest,
) -> Result<StatusCode, reqwest::Error> {
    let url = format!("http://{}:{}/write", node.ip, node.port);

    // Send the write request to the node
    let res = client.post(url).json(&req).send().await?;

    if res.status() == StatusCode::OK {
        println!("Node {} has accepted the write", node.id);
    } else {
        println!("Node {} is not responding", node.id);
    }
    Ok(res.status())
}

// Function to commit a txn on a client
async fn write_commit(
    client: Arc<reqwest::Client>,
    node: NodeInfo,
    req: CommitRequest,
) -> Result<StatusCode, reqwest::Error> {
    let url = format!("http://{}:{}/commit", node.ip, node.port);

    // Send the write request to the node
    let res = client.post(url).json(&req).send().await?;

    if res.status() == StatusCode::OK {
        println!("Node {} has committed the write", node.id);
    } else {
        println!("Node {} is not responding", node.id);
    }
    Ok(res.status())
}

async fn run_2pc_txn(
    client: Arc<reqwest::Client>,
    client_nodes: Vec<NodeInfo>,
    req: WriteUncommittedRequest,
) -> Result<(), axum::Error> {
    println!("Running 2PC Transaction with request: {:?}", req);
    let client_nodes_commit = client_nodes.clone();

    /*
    Request Phase
    */

    println!("Request Phase: Sending uncommitted writes to all nodes");
    // in parallel, write without commit to all the nodes in the cluster
    let mut precommit_tasks = JoinSet::new();
    let mut client_count = 0;

    for node in client_nodes {
        precommit_tasks.spawn(write_no_commit(Arc::clone(&client), node, req.clone()));
        client_count += 1;
    }

    let quorum = (client_count / 2) + 1;
    println!("Total clients: {}, Quorum needed: {}", client_count, quorum);
    let mut vote_yes = 0;

    while let Some(res) = precommit_tasks.join_next().await {
        match res {
            Ok(statusRes) => match statusRes {
                Ok(status) if status == StatusCode::OK => {
                    vote_yes += 1;
                    println!("Vote YES from node");
                }
                _ => println!("Vote NO from node"),
            },
            Err(e) => {
                eprintln!("Task failed: {}", e);
            }
        }
    }

    // If we have a quorum, we can proceed to commit
    println!("Votes YES: {}, Quorum: {}", vote_yes, quorum);
    if (vote_yes < quorum) {
        println!("Not enough votes to commit, aborting transaction");
        return Err(axum::Error::new("Not enough votes to commit"));
    }

    /*
    Commit Phase
    */
    println!("Commit Phase: Sending commit requests to all nodes");
    let mut commit_tasks = JoinSet::new();
    let commit_req = CommitRequest { txn_id: req.txn_id };
    for node in client_nodes_commit {
        commit_tasks.spawn(write_commit(Arc::clone(&client), node, commit_req.clone()));
    }

    let mut commit_vote_yes = 0;
    while let Some(res) = commit_tasks.join_next().await {
        match res {
            Ok(statusRes) => match statusRes {
                Ok(status) if status == StatusCode::OK => {
                    println!("Commit Vote YES from node");
                    commit_vote_yes += 1;
                }
                _ => println!("Commit Vote NO from node"),
            },
            Err(e) => {
                eprintln!("Task failed: {}", e);
            }
        }
    }

    // If we have a quorum, we can proceed to commit
    println!("Commit Votes YES: {}, Quorum: {}", commit_vote_yes, quorum);
    if (commit_vote_yes < quorum) {
        println!("Not enough votes to commit, aborting transaction");
        return Err(axum::Error::new("Not enough votes to commit"));
    }

    Ok(())
}

async fn execute_2pc_transaction(
    State(state): State<SharedState>,
    Json(payload): Json<CoordinatorWriteRequest>,
) -> Result<(), StatusCode> {
    // Refresh the list of client nodes from SDMon

    // Create txn UUID
    let txn_id = Uuid::new_v4();
    let mut own_request = WriteUncommittedRequest {
        key: payload.key.clone(),
        value: payload.value.clone(),
        txn_id,
    };

    let client: Arc<reqwest::Client>;
    let client_nodes: Vec<NodeInfo>;

    // lock under minimal contention so we don't hold the global state lock
    {
        let local_state = state.read().unwrap();
        client = Arc::clone(&local_state.client);
        client_nodes = local_state.latest_client_nodes.clone();
    }

    // Run the 2pc transaction and wait for it to complete
    let res = run_2pc_txn(client, client_nodes, own_request).await;
    match res {
        Ok(_) => {
            println!("2PC Transaction executed successfully");
            Ok(())
        }
        Err(e) => {
            eprintln!("2PC Transaction failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
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
    println!("Hello, coordinator");

    // Accept CLI Arguments for port and IP address
    let args = Args::parse();
    let port: u16 = args.port;
    let ip: String = local_ip_address::local_ip().unwrap().to_string();
    println!("Starting Coordinator at {}:{}", ip, port);

    /*
    BOOTSTRAP Phase
    */
    let client = Arc::new(reqwest::Client::new());
    let sdmon_ipv4 = args.sdmon_ip.parse::<Ipv4Addr>().unwrap();
    let sdmon_ip = std::net::SocketAddrV4::new(sdmon_ipv4, args.sdmon_port);
    let node_type = tupac_rs::common::NodeType::Coordinator;

    // dial into SDMON to register self
    let uuid = register_with_sdmon(sdmon_ip, &client, ip, port, node_type)
        .await
        .unwrap();
    println!("Registered with SDMon, UUID: {}", uuid);

    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    let all_nodes = tupac_rs::common::get_all_nodes_from_sdmon(sdmon_ip, &client)
        .await
        .unwrap();
    println!("All Nodes: {:?}", all_nodes);
    let clients: Vec<NodeInfo> = all_nodes
        .iter()
        .filter(|node| node.node_type == NodeType::DBNode)
        .cloned()
        .collect();
    println!("Client Nodes: {:?}", clients);

    /*
    Start the server
    */

    // Coordinator test: ping all DB Nodes
    let clients_temp = clients.clone();
    ping_all_nodes(Arc::clone(&client), clients_temp).await;

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

    // update the state with the latest known client and nodes
    shared_state.write().unwrap().client = Arc::clone(&client);
    shared_state.write().unwrap().latest_client_nodes = clients.clone();

    // Build our application by composing routes
    let app = Router::new()
        .route("/ping", get(ping))
        .route("/txn_write", post(execute_2pc_transaction))
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
