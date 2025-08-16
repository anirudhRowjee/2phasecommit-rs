use std::net::Ipv4Addr;

use tupac_rs::common::{NodeInfo, NodeType, register_with_sdmon};

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

#[tokio::main]
async fn main() {
    println!("Hello, coordinator");

    // Accept CLI Arguments for port and IP address
    let port: u16 = 3001;
    let ip: String = "127.0.0.1".to_string();

    let client = reqwest::Client::new();
    let sdmon_ip = std::net::SocketAddrV4::new(Ipv4Addr::LOCALHOST, 3000);
    let node_type = tupac_rs::common::NodeType::Coordinator;

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
    // dial into SDMON to register self
    // start server to accept client requests
}
