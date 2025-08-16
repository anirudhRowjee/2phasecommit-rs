use std::net::Ipv4Addr;

use tupac_rs::common::register_with_sdmon;

#[tokio::main]
async fn main() {
    println!("Hello, client");

    // Accept CLI Arguments for port and IP address
    let port: u16 = 3002;
    let ip: String = "127.0.0.1".to_string();

    let client = reqwest::Client::new();
    let sdmon_ip = std::net::SocketAddrV4::new(Ipv4Addr::LOCALHOST, 3000);
    let node_type = tupac_rs::common::NodeType::DBNode;

    let uuid = register_with_sdmon(sdmon_ip, &client, ip, port, node_type)
        .await
        .unwrap();
    println!("Registered with SDMon, UUID: {}", uuid);
    let all_nodes = tupac_rs::common::get_all_nodes_from_sdmon(sdmon_ip, &client)
        .await
        .unwrap();
    println!("All Nodes: {:?}", all_nodes);
    // dial into SDMON to register self
    // start server to accept client requests
}
