use crate::cli::{commands, Cli, Commands};
use crate::consensus::network::{NetworkConfig, NetworkManager};
use clap::Parser;

mod cli;
mod consensus;

fn main() {
    // Parse the command-line arguments using the clap parser.
    let cli = Cli::parse();

    let mut network_manager = NetworkManager::new(NetworkConfig::default());

    // Dispatch the execution to the appropriate command handler based on the parsed subcommand.
    let result = match cli.command {
        Commands::Start { config_file } => commands::start(config_file, network_manager),
        Commands::Join {
            config_file,
            node_address,
        } => commands::join(config_file, node_address, network_manager),
        Commands::Status { node_address } => commands::status(node_address, network_manager),
    };

    // Handle the result of the command execution.
    match result {
        Ok(_) => println!("Command executed successfully."),
        Err(e) => eprintln!("Error: {}", e),
    }
}
