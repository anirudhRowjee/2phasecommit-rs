pub mod common {

    use axum::http::Error;
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;
    #[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Copy)]
    pub enum NodeType {
        #[default]
        Coordinator,
        DBNode,
    }

    #[derive(Serialize, Deserialize)]
    pub struct RegisterNodeRequest {
        pub ip: String,
        pub port: u16,
        pub node_type: NodeType,
    }

    #[derive(Default, Clone, Debug, Serialize, Deserialize)]
    pub struct NodeInfo {
        pub ip: String,
        pub port: u16,
        pub alive: bool,
        pub id: Uuid,
        pub node_type: NodeType,
    }

    // method to register self with SDMon and get back node UUID
    pub async fn register_with_sdmon(
        sdmon_ip: std::net::SocketAddrV4,
        client: &reqwest::Client,
        ip: String,
        port: u16,
        node_type: NodeType,
    ) -> Result<String, Error> {
        let sdmon_url = format!("http://{}/register", sdmon_ip);
        println!("Sending request to SDMon at {}", sdmon_url);
        let req = RegisterNodeRequest {
            ip,
            port,
            node_type,
        };
        let res: String = client
            .post(sdmon_url)
            .json(&req)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        println!("registered node, UUID -> {}", res);
        Ok(res)
    }

    pub async fn get_all_nodes_from_sdmon(
        sdmon_ip: std::net::SocketAddrV4,
        client: &reqwest::Client,
    ) -> Result<Vec<NodeInfo>, Error> {
        let sdmon_url = format!("http://{}/listnodes", sdmon_ip);
        let res: Vec<NodeInfo> = client
            .get(sdmon_url)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        println!("All Nodes -> {:?}", res);
        Ok(res)
    }
}
