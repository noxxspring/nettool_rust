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
    EncryptedChat{
         #[arg(short, long)]
        mode: String, // "server" or "client"

        #[arg(short = 'H', long, default_value = "127.0.0.1")]
        host: String,

        #[arg(short, long)]
        port: u16,
    },
    PortScan,

     ShellAccess {
        #[command(subcommand)]
        mode: ShellMode,
    },

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

#[derive(Subcommand)]
enum ShellMode {
    /// Listen for incoming shell access on a port
    Listen {
        #[arg(short, long)]
        port: u16,
    },
    /// Connect to a remote shell at given IP and port
    Connect {
        #[arg(short = 'H', long)]
        host: String,

        #[arg(short, long)]
        port: u16,
    },
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>>{
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
          Commands::EncryptedChat { mode, host, port } => {
            match mode.to_lowercase().as_str() {
                "server" => commands::encrypted_chat::chat_server(port).await?,
                "client" => commands::encrypted_chat::chat_client(&host, port).await?,
                _ => eprintln!("Invalid mode. Use 'server' or 'client'."),
            }
        }
        Commands::PortScan => commands::port_scan::run().await,
        Commands::ShellAccess { mode } => match mode {
            ShellMode::Listen { port } => {
                let _ = commands::shell_access::start_listener(port);
            }
            ShellMode::Connect { host, port } => {
                let _ = commands::shell_access::start_connector(&host, port);
            }
        },
    }

    Ok(())
}
        
