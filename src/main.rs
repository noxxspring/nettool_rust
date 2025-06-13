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
    FileTransfer{
        #[command(subcommand)]
        mode: FileTransferMode,
    },
    EncryptedChat,
    PortScan,
    ShellAccess,
    
}

#[derive(Subcommand)]
enum FileTransferMode {
    Send {
        #[arg(short = 'f', long)]
        file: String,

        #[arg(short = 'H', long)]
        host: String, 

        #[arg(short, long)]
        port: u16,
    },
    Receive {
        #[arg(short, long)]
        port: u16,

        #[arg(short, long)]
        output: String,
    },
}


#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::FileTransfer { mode } => match mode {
            FileTransferMode::Send { file, host, port } => {
                commands::file_transfer::send(&file, &host, port).await
            }
            FileTransferMode::Receive { port, output } => {
                commands::file_transfer::receive(port, &output).await
            }
            
        },
        Commands::EncryptedChat => commands::encrypted_chat::run().await,
        Commands::PortScan => commands::port_scan::run().await,
        Commands::ShellAccess => commands::shell_access::run().await,
        
    }
}
