use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use aes::Aes256;
use cbc::{Decryptor, Encryptor};
use rand::RngCore;
use block_padding::Pkcs7;
use aes::cipher::generic_array::GenericArray;

type Aes256CbcEnc = Encryptor<Aes256>;
type Aes256CbcDec = Decryptor<Aes256>;

pub fn encrypt_chunk(data: &[u8], key: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    if key.len() != 32 {
        return Err("Key must be 32 bytes".into());
    }

    let mut iv = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut iv);

    let cipher = Aes256CbcEnc::new(GenericArray::from_slice(key), GenericArray::from_slice(&iv));
    let mut buffer = vec![0u8; data.len() + 16];
    buffer[..data.len()].copy_from_slice(data);
    let encrypted = cipher.encrypt_padded_mut::<Pkcs7>(&mut buffer, data.len())
    .map_err(|e| format!("Encryption failed"))?;

    let mut result = iv.to_vec();
    result.extend_from_slice(encrypted);
    Ok(result)
}

pub fn decrypt_chunk(data: &[u8], key: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    if key.len() != 32 {
        return Err("Key must be 32 bytes".into());
    }
    if data.len() < 16 {
        return Err("Chunk too small to contain IV".into());
    }

    let (iv, encrypted_data) = data.split_at(16);
    let cipher = Aes256CbcDec::new(GenericArray::from_slice(key), GenericArray::from_slice(iv));
    let mut buffer = encrypted_data.to_vec();
    let decrypted = cipher.decrypt_padded_mut::<Pkcs7>(&mut buffer)
    .map_err(|e| format!("Decrypton failed"))?;
    Ok(decrypted.to_vec())
}

