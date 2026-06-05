use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chacha20poly1305::{
    aead::{
        rand_core::{OsRng, RngCore},
        Aead, KeyInit,
    },
    XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const ENVELOPE_VERSION: i64 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncEncryptedEnvelope {
    pub version: i64,
    pub algorithm: String,
    pub kdf: String,
    pub compression: String,
    pub salt_b64: String,
    pub nonce_b64: String,
    pub ciphertext_b64: String,
}

pub fn encrypt_sync_payload(
    plaintext: &[u8],
    password: &str,
) -> Result<SyncEncryptedEnvelope, String> {
    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    encrypt_sync_payload_with_material(plaintext, password, &salt, nonce)
}

#[allow(dead_code)]
pub fn decrypt_sync_payload(
    envelope: &SyncEncryptedEnvelope,
    password: &str,
) -> Result<Vec<u8>, String> {
    if envelope.version != ENVELOPE_VERSION {
        return Err(format!("不支持的同步文件版本：{}", envelope.version));
    }
    if envelope.algorithm != "XChaCha20-Poly1305" || envelope.compression != "zstd" {
        return Err("不支持的同步文件算法".to_string());
    }

    let salt = STANDARD
        .decode(&envelope.salt_b64)
        .map_err(|err| format!("同步文件 salt 无效：{err}"))?;
    let nonce = STANDARD
        .decode(&envelope.nonce_b64)
        .map_err(|err| format!("同步文件 nonce 无效：{err}"))?;
    let ciphertext = STANDARD
        .decode(&envelope.ciphertext_b64)
        .map_err(|err| format!("同步文件密文无效：{err}"))?;
    if nonce.len() != NONCE_LEN {
        return Err("同步文件 nonce 长度无效".to_string());
    }

    let key = derive_key(password, &salt)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|err| format!("初始化同步解密器失败：{err}"))?;
    let compressed = cipher
        .decrypt(XNonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| "解密失败：同步密码不正确或文件已损坏".to_string())?;

    zstd::stream::decode_all(compressed.as_slice())
        .map_err(|err| format!("解压同步文件失败：{err}"))
}

pub fn content_hash_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
fn encrypt_sync_payload_for_test(
    plaintext: &[u8],
    password: &str,
    salt: &[u8],
    nonce: [u8; NONCE_LEN],
) -> Result<SyncEncryptedEnvelope, String> {
    encrypt_sync_payload_with_material(plaintext, password, salt, nonce)
}

fn encrypt_sync_payload_with_material(
    plaintext: &[u8],
    password: &str,
    salt: &[u8],
    nonce: [u8; NONCE_LEN],
) -> Result<SyncEncryptedEnvelope, String> {
    let key = derive_key(password, salt)?;
    let compressed =
        zstd::stream::encode_all(plaintext, 3).map_err(|err| format!("压缩同步数据失败：{err}"))?;
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|err| format!("初始化同步加密器失败：{err}"))?;
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), compressed.as_ref())
        .map_err(|err| format!("加密同步数据失败：{err}"))?;

    Ok(SyncEncryptedEnvelope {
        version: ENVELOPE_VERSION,
        algorithm: "XChaCha20-Poly1305".to_string(),
        kdf: "Argon2id".to_string(),
        compression: "zstd".to_string(),
        salt_b64: STANDARD.encode(salt),
        nonce_b64: STANDARD.encode(nonce),
        ciphertext_b64: STANDARD.encode(ciphertext),
    })
}

fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; KEY_LEN], String> {
    if password.is_empty() {
        return Err("同步密码不能为空".to_string());
    }
    if salt.is_empty() {
        return Err("同步文件 salt 不能为空".to_string());
    }

    let mut key = [0u8; KEY_LEN];
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default());
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|err| format!("派生同步密钥失败：{err}"))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_sync_payload_round_trips_and_hides_plaintext() {
        let plaintext = br#"{"secret":"token usage metadata","calls":[{"total_tokens":123}]}"#;
        let envelope = encrypt_sync_payload_for_test(
            plaintext,
            "correct horse battery staple",
            b"0123456789abcdef",
            [7u8; 24],
        )
        .expect("payload encrypts");

        assert_eq!(envelope.algorithm, "XChaCha20-Poly1305");
        assert_eq!(envelope.compression, "zstd");
        assert!(!envelope.ciphertext_b64.contains("token usage metadata"));

        let decrypted = decrypt_sync_payload(&envelope, "correct horse battery staple")
            .expect("payload decrypts");
        assert_eq!(decrypted, plaintext);

        let random_envelope = encrypt_sync_payload(plaintext, "correct horse battery staple")
            .expect("payload encrypts with random material");
        let random_decrypted =
            decrypt_sync_payload(&random_envelope, "correct horse battery staple")
                .expect("random payload decrypts");
        assert_eq!(random_decrypted, plaintext);
    }

    #[test]
    fn encrypted_sync_payload_rejects_wrong_password() {
        let plaintext = br#"{"calls":[]}"#;
        let envelope = encrypt_sync_payload_for_test(
            plaintext,
            "correct password",
            b"0123456789abcdef",
            [3u8; 24],
        )
        .expect("payload encrypts");

        let err =
            decrypt_sync_payload(&envelope, "wrong password").expect_err("wrong password fails");
        assert!(err.contains("解密失败"));
    }

    #[test]
    fn content_hash_is_stable_hex_sha256() {
        assert_eq!(
            content_hash_hex(b"tokenscope"),
            "29eb5c250c9ee1b9fa38b42ee53920c6a4ad8641e609b7cef4b944f569add5ab"
        );
    }
}
