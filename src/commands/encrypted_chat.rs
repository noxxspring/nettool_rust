use colored::Colorize;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::Mutex,
};
use chrono::Local;
use std::{error::Error, sync::Arc};
use x25519_dalek::{EphemeralSecret, PublicKey};
use aes::cipher::{KeyIvInit, BlockEncryptMut, BlockDecryptMut};
use aes::Aes256;
use cbc::{Encryptor, Decryptor};
use block_padding::Pkcs7;
use aes::cipher::generic_array::GenericArray;
use rand::RngCore;
use tracing::{info, error, warn};

type Aes256CbcEnc = Encryptor<Aes256>;
type Aes256CbcDec = Decryptor<Aes256>;

const IV_LEN: usize = 16;

#[derive(Clone)]
struct Client {
    username: String,
    key: [u8; 32],
    writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
}

type SharedClients = Arc<Mutex<Vec<Client>>>;

async fn perform_key_exchange(stream: &mut TcpStream) -> Result<[u8; 32], Box<dyn Error + Send + Sync>> {
    let private = EphemeralSecret::new(rand::rngs::OsRng);
    let public = PublicKey::from(&private);

    stream.write_all(public.as_bytes()).await?;
    let mut peer_key = [0u8; 32];
    stream.read_exact(&mut peer_key).await?;
    let peer_public = PublicKey::from(peer_key);
    let shared_secret = private.diffie_hellman(&peer_public);
    Ok(*shared_secret.as_bytes())
}

fn encrypt_message(key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    let mut iv = [0u8; IV_LEN];
    rand::thread_rng().fill_bytes(&mut iv);

    let cipher = Aes256CbcEnc::new(GenericArray::from_slice(key), GenericArray::from_slice(&iv));
    let mut buffer = vec![0u8; plaintext.len() + IV_LEN];
    buffer[..plaintext.len()].copy_from_slice(plaintext);

    let encrypted = cipher.encrypt_padded_mut::<Pkcs7>(&mut buffer, plaintext.len())
    .map_err(|e| format!("unpadded errpr"))?;
    let mut result = iv.to_vec();
    result.extend_from_slice(encrypted);
    Ok(result)
}

fn decrypt_message(key: &[u8], data: &[u8]) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    if data.len() < IV_LEN {
        return Err("Data too short".into());
    }
    let (iv, encrypted_data) = data.split_at(IV_LEN);
    let cipher = Aes256CbcDec::new(GenericArray::from_slice(key), GenericArray::from_slice(iv));
    let mut buffer = encrypted_data.to_vec();
    let decrypted = cipher.decrypt_padded_mut::<Pkcs7>(&mut buffer)
    .map_err(|e| format!("unpaded error: {}", e))?;
    Ok(decrypted.to_vec())
}

pub async fn chat_server(port: u16) -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::fmt::init();
    let clients: SharedClients = Arc::new(Mutex::new(Vec::new()));
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    info!(" Encrypted Chat Server running on port {}", port);

    loop {
        let (mut stream, addr) = listener.accept().await?;
        let clients = Arc::clone(&clients);

        tokio::spawn(async move {
            let key = match perform_key_exchange(&mut stream).await {
                Ok(k) => k,
                Err(e) => {
                    error!("Key exchange failed: {}", e);
                    return;
                }
            };

            let (r, w) = stream.into_split();
            let writer = Arc::new(Mutex::new(w));
            let mut reader = BufReader::new(r);
            let mut line = String::new();

            let _ = writer.lock().await.write_all(b"Enter your username:\n").await;
            if reader.read_line(&mut line).await.is_err() {
                return;
            }

            let username = line.trim().to_string();
            info!(" {} Joined from {}", username, addr);

            let client = Client {
                username: username.clone(),
                key,
                writer: Arc::clone(&writer),
            };
            clients.lock().await.push(client.clone());

            let clients_reader = Arc::clone(&clients);
            let username_reader = username.clone();
            let key_reader = key;

            tokio::spawn(async move {
                let mut recv_reader = BufReader::new(reader);
                loop {
                    let mut len_buf = [0u8; 4];
                    if recv_reader.read_exact(&mut len_buf).await.is_err() {
                        break;
                    }

                    let msg_len = u32::from_be_bytes(len_buf) as usize;
                    let mut msg_buf = vec![0u8; msg_len];
                    if recv_reader.read_exact(&mut msg_buf).await.is_err() {
                        break;
                    }

                    let plaintext = match decrypt_message(&key_reader, &msg_buf) {
                        Ok(p) => p,
                        Err(_) => {
                            warn!(" Failed to decrypt message from {}", username_reader);
                            continue;
                        }
                    };

                    let text = match String::from_utf8(plaintext) {
                        Ok(t) => t.trim().to_string(),
                        Err(_) => {
                            warn!(" Invalid UTF-8 from {}", username_reader);
                            continue;
                        }
                    };

                    let timestamp = Local::now().format("%H:%M:%S");
                    let full_msg = format!("[{}] {}: {}", timestamp, username_reader, text);
                    info!(" Broadcasting: {}", full_msg);

                    let clients_guard = clients_reader.lock().await;
                    for other in clients_guard.iter() {
                        if other.username == username_reader {
                            if let Ok(ack) = encrypt_message(&other.key, b"") {
                                let mut writer = other.writer.lock().await;
                                let _ = writer.write_all(&(ack.len() as u32).to_be_bytes()).await;
                                let _ = writer.write_all(&ack).await;
                                
                            }
                        }

                        match encrypt_message(&other.key, full_msg.as_bytes()) {
                            Ok(encrypted) => {
                                let mut writer = other.writer.lock().await;
                                if let Err(e) = writer.write_all(&(encrypted.len() as u32).to_be_bytes()).await {
                                    warn!(" Failed to write length to {}: {}", other.username, e);
                                    continue;
                                }
                                if let Err(e) = writer.write_all(&encrypted).await {
                                    warn!(" Failed to write message to {}: {}", other.username, e);
                                    continue;
                                }
                                let _ = writer.flush().await;
                                info!(" Broadcasted message to {}", other.username);
                            }
                            Err(e) => warn!(" Failed to encrypt for {}: {}", other.username, e),
                        }
                    }
                }

                info!(" {} Disconnected.", username_reader);
                let mut clients_guard = clients_reader.lock().await;
                clients_guard.retain(|c| c.username != username_reader);
            });
        });
    }
}



pub async fn chat_client(host: &str, port: u16) -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::fmt::init();
    let mut stream = TcpStream::connect((host, port)).await?;
    let key = perform_key_exchange(&mut stream).await?;

    let (r, mut w) = stream.into_split();
    let mut reader = BufReader::new(r);
    let mut line = String::new();

    reader.read_line(&mut line).await?;
    print!("{}", line);
    let _ = tokio::io::stdout().flush().await;

    let mut stdin = BufReader::new(tokio::io::stdin()).lines();
    let username = if let Ok(Some(name)) = stdin.next_line().await {
        w.write_all(format!("{}\n", name).as_bytes()).await?;
        name.trim().to_string()
    } else {
        return Err("Failed to read username".into());
    };

    let username = Arc::new(username);
    let my_name = Arc::clone(&username);
    let key_recv = key.clone();
    let mut recv_reader = BufReader::new(reader);

    tokio::spawn(async move {
        loop {
            let mut len_buf = [0u8; 4];
            if recv_reader.read_exact(&mut len_buf).await.is_err() {
                warn!("Connection closed or read error.");
                break;
            }

            let msg_len = u32::from_be_bytes(len_buf) as usize;
            let mut msg_buf = vec![0u8; msg_len];
            if recv_reader.read_exact(&mut msg_buf).await.is_err() {
                warn!("Failed to read full message.");
                break;
            }

            match decrypt_message(&key_recv, &msg_buf) {
                Ok(decrypted) => match String::from_utf8(decrypted) {
                    Ok(text) => {
                        // only print messages not from self
                        if text.contains(&format!("{}:", my_name)) {
                            continue;
                        }
                        println!("{}", text.trim());
                    }
                    Err(_) => warn!("❌ Failed to decode broadcast message."),
                },
                Err(_) => warn!("❌ Failed to decrypt broadcast message."),
            }
        }
    });

    while let Ok(Some(msg)) = stdin.next_line().await {
        if msg.trim().is_empty() {
            continue;
        }

        let timestamp = chrono::Local::now().format("[%H:%M:%S]");
        let colored_name = username.blue().bold(); // 💙 bold blue name
        let formatted = format!("{} {}: {}", timestamp, colored_name, msg.trim());

        let encrypted = encrypt_message(&key, formatted.as_bytes())?;
        w.write_all(&(encrypted.len() as u32).to_be_bytes()).await?;
        w.write_all(&encrypted).await?;

        println!("{}", formatted);
        println!("{}", "✔ Delivered".green()); // ✅ green tick
    }

    Ok(())
}


