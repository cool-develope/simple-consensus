use clap::{Parser, Subcommand};

pub mod commands;

/// The top-level CLI structure for the simple consensus application.
#[derive(Parser)]
#[command(name = "simple-consensus")]
#[command(about = "A simple consensus CLI", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// The available subcommands for the CLI.
#[derive(Subcommand)]
pub enum Commands {
    /// Starts a new node.
    Start {
        #[arg(short, long)]
        config_file: String,
    },
    /// Joins an existing node to the cluster.
    Join {
        #[arg(short, long)]
        config_file: String,
        #[arg(short, long)]
        node_address: String,
    },
    /// Checks the status of a node.
    Status {
        #[arg(short, long)]
        node_address: String,
    },
}
