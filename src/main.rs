use crate::cli::{commands, Cli, Commands};
use clap::Parser;

mod cli;

fn main() {
    // Parse the command-line arguments using the clap parser.
    let cli = Cli::parse();

    // Dispatch the execution to the appropriate command handler based on the parsed subcommand.
    let result = match cli.command {
        Commands::Start { config_file } => commands::start(config_file),
        Commands::Join {
            config_file,
            node_address,
        } => commands::join(config_file, node_address),
        Commands::Status { node_address } => commands::status(node_address),
    };

    // Handle the result of the command execution.
    match result {
        Ok(_) => println!("Command executed successfully."),
        Err(e) => eprintln!("Error: {}", e),
    }
}
