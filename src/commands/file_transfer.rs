use std::{fs::create_dir_all, path::Path};

use indicatif::{ProgressBar, ProgressStyle};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use crate::utils::encryption::{encrypt_chunk, decrypt_chunk};

const CHUNK_SIZE: usize = 8192;
const AES_KEY: &[u8; 32] = b"This_is_32_byte_long_aes_key_!!!";

/// Send a file to the receiver over TCP with AES-256 encryption.
pub async fn send(file_path: &str, host: &str, port: u16) {
    let address = format!("{}:{}", host, port);
    let mut stream = match TcpStream::connect(&address).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to connect to receiver at {}: {}", address, e);
            return;
        }
    };

    let mut file = match File::open(file_path).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Could not open file '{}': {}", file_path, e);
            return;
        }
    };

    let metadata = match file.metadata().await {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to read file metadata: {}", e);
            return;
        }
    };

    let filename = Path::new(file_path).file_name().unwrap().to_string_lossy();
    let header = format!("{}:{}\n", filename, metadata.len());

    if stream.write_all(header.as_bytes()).await.is_err() {
        eprintln!("Failed to send file header");
        return;
    }

    let progress = ProgressBar::new(metadata.len());
    progress.set_style(
        ProgressStyle::with_template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("=> "),
    );

    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut total_sent = 0;

    loop {
        let n = file.read(&mut buffer).await.unwrap_or(0);
        if n == 0 {
            break;
        }

        let encrypted = match encrypt_chunk(&buffer[..n], AES_KEY) {
            Ok(enc) => enc,
            Err(e) => {
                eprintln!("Encryption failed: {}", e);
                return;
            }
        };

        let size = (encrypted.len() as u32).to_be_bytes(); // prefix with chunk size
        if stream.write_all(&size).await.is_err()
            || stream.write_all(&encrypted).await.is_err()
        {
            eprintln!("Failed to send encrypted chunk");
            return;
        }

        total_sent += n as u64;
        progress.set_position(total_sent);
    }

    progress.finish_with_message("File Sent");
    println!("Sent file '{}' to {}", filename, address);
}

/// Receive an encrypted file and save it.
pub async fn receive(port: u16, output_dir: &str) {
    if let Err(e) = create_dir_all(output_dir) {
        eprintln!("Could not create output dir: {}", e);
        return;
    }

    let listener = TcpListener::bind(("0.0.0.0", port)).await.unwrap();
    println!("Receiver listening on port {}", port);

    let (mut socket, addr) = listener.accept().await.unwrap();
    println!("Connection from {}", addr);

    // Read header: `filename:filesize\n`
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

    let header_str = String::from_utf8_lossy(&header);
    let mut parts = header_str.split(':');
    let filename = parts.next().unwrap_or("file");
    let filesize: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);

    let save_path = Path::new(output_dir).join(filename);
    let mut file = match File::create(&save_path).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to create output file: {}", e);
            return;
        }
    };

    let progress = ProgressBar::new(filesize as u64);
    progress.set_style(
        ProgressStyle::with_template("[{elapsed_precise}] [{bar:40.green/white}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("=> "),
    );

    let mut total_written = 0u64;
    loop {
        let mut size_buf = [0u8; 4];
        if socket.read_exact(&mut size_buf).await.is_err() {
            break; // No more chunks
        }

        let chunk_size = u32::from_be_bytes(size_buf) as usize;
        let mut encrypted_chunk = vec![0u8; chunk_size];
        if socket.read_exact(&mut encrypted_chunk).await.is_err() {
            eprintln!("Failed to read encrypted chunk");
            return;
        }

        let decrypted = match decrypt_chunk(&encrypted_chunk, AES_KEY) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Decryption failed: {}", e);
                return;
            }
        };

        if file.write_all(&decrypted).await.is_err() {
            eprintln!("Failed to write to output file");
            return;
        }

        total_written += decrypted.len() as u64;
        progress.set_position(total_written);
    }

    progress.finish_with_message("File received");
    println!("Received file '{}' ({} bytes)", filename, total_written);
}
