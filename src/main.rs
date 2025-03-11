use crate::cli::{commands, Cli};
use clap::Parser;

mod cli;
mod consensus;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command-line arguments using Clap.
    let cli = Cli::parse();

    // Dispatch to the appropriate command method based on the subcommand.
    match cli.command {
        cli::Commands::Start { config_file } => {
            commands::start(config_file).await?;
        }
        cli::Commands::Join {
            config_file,
            node_address,
        } => {
            commands::join(config_file, node_address).await?;
        }
        cli::Commands::Status { node_address: _ } => {
            // In our embedded version, status may not require a node address parameter.
            commands::status().await?;
        }
    }

    Ok(())
}
