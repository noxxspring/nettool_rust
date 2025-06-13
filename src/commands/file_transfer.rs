use std::{fs::create_dir_all, path::Path};

use indicatif::{ProgressBar, ProgressStyle};
use tokio::{fs::{ File}, io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}};


/// Send a file to the receiver over TCP.
pub async fn send(file_path: &str, host: &str, port: u16) {
    let address = format!("{}:{}", host, port);
    let mut stream = match TcpStream::connect(&address).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to connect to receiver at {}: {}", address, e);
            return;
        }
    };
    
    let mut file = match tokio::fs::File::open(file_path).await {
    Ok(f) => f,
    Err(e) => {
        eprintln!("Error: Could not open file '{}': {}", file_path, e);
        return;
    }
};


let metadata = match file.metadata().await {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to read metadata for '{}': {}", file_path, e);
            return;
        }
    }; 

    let filename = match Path::new(file_path).file_name().and_then(|f| f.to_str()) {
        Some(name) => name,
        None => {
            eprintln!("Invalid file name '{}'", file_path);
            return;
        }
    };

    let header = format!("{}:{}", filename, metadata.len());
    if stream.write_all(header.as_bytes()).await.is_err() || stream.write_all(b"\n").await.is_err() {
        eprintln!("Failed to send header to {}", address);
        return;
    }

    let progress = ProgressBar::new(metadata.len());
    progress.set_style(
        ProgressStyle::with_template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .unwrap()
        .progress_chars("=> ")
    );


    let mut buffer = [0u8; 4096];
    let mut total_sent = 0;
    loop {
        let bytes_read = match file.read(&mut buffer).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                eprintln!("Error reading file '{}': {}", file_path, e);
                return;
            }
        };

        if let Err(e) = stream.write_all(&buffer[..bytes_read]).await {
            eprintln!("Error sending data to {}: {}", address, e);
            return;
        }

        total_sent += bytes_read as u64;
        progress.set_position(total_sent);

    }
    progress.finish_with_message("File Sent");

    println!("File '{}' successfully sent to {}", file_path, address);
}


/// Receive a file over TCP and save it to output_dir.
pub async fn receive(port: u16, output_dir: &str) {
    if let Err(e) = create_dir_all(output_dir) {
        eprintln!("Could not create output directory '{}': {}", output_dir, e);
        return;
    }
   let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind to port {}: {}", port, e);
            return;
        }
    };
    println!("Receiver listening on port {}",port);

  let (mut socket, addr) = match listener.accept().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to accept connection: {}", e);
            return;
        }
    };        
    
    println!("Connection from {}", addr);


       //Read header

        let mut header = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        if socket.read_exact(&mut byte).await.is_err() {
            eprintln!("Failed to read file header");
            return;
        }
        if byte[0] == b'\n' {
            break;
        }
        header.push(byte[0]);

    }

let header_str = match String::from_utf8(header) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Header is not valid UTF-8: {}", e);
            return;
        }
    };  

        let mut parts = header_str.split(':');
        let filename = match parts.next() {
        Some(name) => name,
        None => {
            eprintln!("Header missing filename");
            return;
        }
    };

     let filesize: usize = match parts.next().and_then(|s| s.parse().ok()) {
        Some(size) => size,
        None => {
            eprintln!("Header missing or invalid filesize");
            return;
        }
    };

     let save_path = Path::new(output_dir).join(filename);
    let mut file = match File::create(&save_path).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to create output file '{}': {}", save_path.display(), e);
            return;
        }
    };

    let progress = ProgressBar::new(filesize as u64);
    progress.set_style(
       ProgressStyle::with_template("[{elapsed_precise}] [{bar:40.green/white}] {bytes}/{total_bytes} ({eta})")
        .unwrap()
        .progress_chars("=> "),
);


    let mut total_read = 0;
    let mut buffer = [0u8; 4096];
    while total_read < filesize {
        let read = match socket.read(&mut buffer).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                eprintln!("Failed to receive file data: {}", e);
                return;
            }
        };

        if file.write_all(&buffer[..read]).await.is_err() {
            eprintln!("Failed to write to output file");
            return;
        }

        total_read += read;
        progress.set_position(total_read as u64);

    }

    progress.finish_with_message("File received");

    println!("Received file '{}' ({} bytes)", filename, total_read);
    }
    

