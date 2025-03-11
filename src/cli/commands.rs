use crate::cli::config::Config;
use crate::consensus::engine::{Engine, EngineConfig};
use hex::decode;
use std::{error::Error, str::FromStr};
use tokio::sync::oneshot;

/// Starts a new node in the consensus cluster.
/// This method sends a StartListening command and then prints any network events.
pub async fn start(config_file: String) -> Result<(), Box<dyn Error>> {
    println!("Starting node");
    let cfg = Config::new(&config_file)?;
    // For example, use a default listen address; you could also parse it from the config.
    let listen_addr = format!("/ip4/0.0.0.0/tcp/{}", cfg.listen_on_port);
    let engine_config = EngineConfig {
        secret_key: decode(&cfg.secret_key)?.try_into().unwrap(),
        listen_address: listen_addr.clone(),
        bootstrap_nodes: cfg.bootstrap_nodes.clone(),
    };
    let engine = Engine::new(engine_config);
    Ok(())
}

/// Joins an existing node to the consensus cluster.
/// It extracts the target peer's ID from the address, sends a DialPeer command,
/// and waits for the dial to complete.
pub async fn join(config_file: String, node_address: String) -> Result<(), Box<dyn Error>> {
    println!(
        "Joining node with config file: {} and target node address: {}",
        config_file, node_address
    );
    // Assume the address includes the peer ID as the last component (e.g., /ip4/1.2.3.4/tcp/12345/p2p/<peer_id>)
    Ok(())
}

/// Checks the status of the local node in the consensus cluster.
/// It sends a GetLocalPeerId command and prints the local peer id.
pub async fn status() -> Result<(), Box<dyn Error>> {
    Ok(())
}

// If your protocol supports additional commands like put/get, you could add them here.
// For example:
/*
pub async fn put(&mut self, key: String, value: Vec<u8>) -> Result<(), Box<dyn Error>> {
    println!("Putting value {}: {:?}", key, value);
    self.network_manager
        .command_sender()
        .try_send(NetworkCommand::PutValue { key, value })?;
    Ok(())
}

pub async fn get(&mut self, key: String) -> Result<(), Box<dyn Error>> {
    println!("Getting value for key: {}", key);
    let (tx, rx) = oneshot::channel();
    self.network_manager
        .command_sender()
        .try_send(NetworkCommand::GetValue {
            key,
            response_channel: tx,
        })?;
    if let Ok(Some(value)) = rx.await {
        println!("Got value: {:?}", value);
    }
    Ok(())
}
*/
