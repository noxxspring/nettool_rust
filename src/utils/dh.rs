use rand::rngs::OsRng;
use x25519_dalek::{EphemeralSecret, PublicKey};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use std::error::Error;

/// Perform Diffie-Hellman key exchange using x25519
pub async fn perform_key_exchange(
    mut reader: BufReader<ReadHalf<tokio::net::TcpStream>>,
    mut writer: WriteHalf<tokio::net::TcpStream>,
) -> Result<([u8; 32], BufReader<ReadHalf<tokio::net::TcpStream>>, WriteHalf<tokio::net::TcpStream>), Box<dyn Error>> {
    // Generate local key pair
    let private = EphemeralSecret::new(OsRng);
    let public = PublicKey::from(&private);

    // Send public key to peer
    writer.write_all(public.as_bytes()).await?;

    // Receive peer's public key
    let mut peer_pub_bytes = [0u8; 32];
    reader.read_exact(&mut peer_pub_bytes).await?;

    let peer_public = PublicKey::from(peer_pub_bytes);
    let shared_secret = private.diffie_hellman(&peer_public);

    // Return shared key (AES key) and split handles
    Ok((*shared_secret.as_bytes(), reader, writer))
}
