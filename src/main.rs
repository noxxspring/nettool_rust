mod commands;
mod utils;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "nettool")]
#[command(about = "A Netcat-like networking tool in Rust", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    FileTransfer,
    EncryptedChat,
    PortScan,
    ShellAccess,
    
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::FileTransfer => commands::file_transfer::run().await,
        Commands::EncryptedChat => commands::encrypted_chat::run().await,
        Commands::PortScan => commands::port_scan::run().await,
        Commands::ShellAccess => commands::shell_access::run().await,
        
    }
}
