use crate::consensus::message::NetworkCommand;
use crate::cli::config::Config;
use libp2p::{Multiaddr, PeerId};
use std::{error::Error, str::FromStr};
use tokio::sync::oneshot;


/// Starts a new node in the consensus cluster.
/// This method sends a StartListening command and then prints any network events.
pub async fn start(cfg: &Config) -> Result<(), Box<dyn Error>> {
    println!("Starting node with config file: {}", config_file);
    // For example, use a default listen address; you could also parse it from the config.
    let listen_addr: Multiaddr = "/ip4/0.0.0.0/tcp/0".parse()?;
    let (resp_tx, resp_rx) = oneshot::channel();
    network_manager
        .send_command(NetworkCommand::StartListening {
            addr: listen_addr,
            response: resp_tx,
        })
        .await?;
    // Wait for confirmation that listening has started.
    resp_rx.await?.unwrap();

    // Optionally, print out events continuously.
    while let Some(event) = network_manager.next_event().await {
        println!("Received network event: {:?}", event);
    }
    Ok(())
}

/// Joins an existing node to the consensus cluster.
/// It extracts the target peer's ID from the address, sends a DialPeer command,
/// and waits for the dial to complete.
pub async fn join(
    &mut self,
    config_file: String,
    node_address: String,
) -> Result<(), Box<dyn Error>> {
    println!(
        "Joining node with config file: {} and target node address: {}",
        config_file, node_address
    );
    // Assume the address includes the peer ID as the last component (e.g., /ip4/1.2.3.4/tcp/12345/p2p/<peer_id>)
    let parts: Vec<&str> = node_address.split('/').collect();
    let peer_id_str = parts.last().ok_or("Invalid node address")?;
    let peer_id = PeerId::from_str(peer_id_str)?;
    let addr = Multiaddr::from_str(&node_address)?;
    let (resp_tx, resp_rx) = oneshot::channel();
    self.network_manager
        .send_command(NetworkCommand::DialPeer {
            peer_id,
            addr,
            response: resp_tx,
        })
        .await?;
    resp_rx.await?.unwrap();
    Ok(())
}

/// Checks the status of the local node in the consensus cluster.
/// It sends a GetLocalPeerId command and prints the local peer id.
pub async fn status(&mut self) -> Result<(), Box<dyn Error>> {
    let (tx, rx) = oneshot::channel();
    self.network_manager
        .send_command(NetworkCommand::GetLocalPeerId { response: tx })
        .await?;
    let peer_id = rx.await?;
    println!("Current local peer id: {}", peer_id.unwrap());
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

