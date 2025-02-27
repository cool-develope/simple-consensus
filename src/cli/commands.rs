use crate::consensus::network::{NetworkCommand, NetworkManager};
use libp2p::{Multiaddr, PeerId};
use std::{error::Error, str::FromStr};
use tokio::sync::oneshot;

/// Starts a new node in the consensus cluster.
///
/// # Arguments
///
/// * `config_file` - The path to the configuration file.
/// * `network_manager` - the NetworkManager Instance
///
/// # Returns
///
/// Returns `Ok(())` if the node started successfully, otherwise returns an `Err` with an error message.
pub async fn start(
    config_file: String,
    mut network_manager: NetworkManager,
) -> Result<(), Box<dyn Error>> {
    // TODO: Implement the actual node start logic.
    println!("Starting node with config file: {}", config_file);
    network_manager.send_command(NetworkCommand::StartListening)?;

    loop {
        if let Some(event) = network_manager.next_event().await {
            println!("Received network event: {:?}", event);
        }
    }
}

/// Joins an existing node to the consensus cluster.
///
/// # Arguments
///
/// * `config_file` - The path to the configuration file.
/// * `node_address` - The address of an existing node in the cluster.
/// * `network_manager` - the NetworkManager Instance
///
/// # Returns
///
/// Returns `Ok(())` if the node joined successfully, otherwise returns an `Err` with an error message.
pub async fn join(
    config_file: String,
    node_address: String,
    network_manager: NetworkManager,
) -> Result<(), Box<dyn Error>> {
    // TODO: Implement the actual node join logic.
    println!(
        "Joining node with config file: {} and target node address: {}",
        config_file, node_address
    );
    let parts: Vec<&str> = node_address.split('/').collect();
    let peer_id_str = parts.last().ok_or("Invalid node address")?;
    let peer_id = PeerId::from_str(peer_id_str)?;
    let node_address = Multiaddr::from_str(node_address.as_str())?;
    network_manager.send_command(NetworkCommand::DialPeer {
        peer_id,
        address: node_address,
    })?;
    Ok(())
}

/// Checks the status of a node in the consensus cluster.
///
/// # Arguments
///
/// * `node_address` - The address of the node to check.
/// * `network_manager` - the NetworkManager Instance
///
/// # Returns
///
/// Returns `Ok(())` if the status check was successful, otherwise returns an `Err` with an error message.
pub async fn status(
    node_address: String,
    network_manager: NetworkManager,
) -> Result<(), Box<dyn Error>> {
    // TODO: Implement the actual node status check logic.
    println!("Checking status of node: {}", node_address);

    let (tx, rx) = oneshot::channel();
    network_manager.send_command(NetworkCommand::GetLocalPeerId {
        response_channel: tx,
    })?;
    if let Ok(peer_id) = rx.await {
        println!("Current peer id:{}", peer_id);
    }
    Ok(())
}

pub async fn put(
    key: String,
    value: Vec<u8>,
    network_manager: NetworkManager,
) -> Result<(), Box<dyn Error>> {
    println!("put value {key}:{value:?}");
    network_manager.send_command(NetworkCommand::PutValue { key, value })?;
    Ok(())
}

pub async fn get(key: String, network_manager: NetworkManager) -> Result<(), Box<dyn Error>> {
    println!("get value {key}");
    let (tx, rx) = oneshot::channel();
    network_manager.send_command(NetworkCommand::GetValue {
        key,
        response_channel: tx,
    })?;
    if let Ok(Some(value)) = rx.await {
        println!("get value {value:?}");
    }
    Ok(())
}
