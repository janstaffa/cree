use cree::Error;
use crypto::aead::{AeadDecryptor, AeadEncryptor};
use crypto::aes_gcm::AesGcm;
use crypto::{aes::KeySize, hmac, sha2};
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use curve25519_dalek::montgomery::MontgomeryPoint;
use curve25519_dalek::scalar::Scalar;
use rand_core::{OsRng, RngCore};

pub struct EphemeralPair {
    private_key: Scalar,
    public_key: MontgomeryPoint,
}

impl EphemeralPair {
    pub fn new() -> EphemeralPair {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);

        let private = clamp_scalar(bytes);
        let public = (&ED25519_BASEPOINT_TABLE * &private).to_montgomery();

        EphemeralPair {
            private_key: private,
            public_key: public,
        }
    }
    pub fn diffie_hellman(&self, other_public: &[u8; 32]) -> MontgomeryPoint {
        self.private_key * MontgomeryPoint(*other_public)
    }

    /// Get a reference to the ephemeral pair's public key.
    pub fn public_key(&self) -> MontgomeryPoint {
        self.public_key
    }
}
fn clamp_scalar(mut scalar: [u8; 32]) -> Scalar {
    scalar[0] &= 248;
    scalar[31] &= 127;
    scalar[31] |= 64;

    Scalar::from_bits(scalar)
}

#[derive(Debug, Clone)]
pub enum ECCurve {
    x25519,
}

// this struct carries all the encryption and decryption logic
pub struct EncryptedMessage;

impl EncryptedMessage {
    /// This function encrypts the data passed to it.
    /// Returns: encrypted message in bytes
    pub fn encrypt(data: &[u8], encryption_iv: &[u8], encrypt_key: &[u8], aad: &[u8]) -> Vec<u8> {
        let mut cipher = AesGcm::new(KeySize::KeySize128, encrypt_key, encryption_iv, aad);

        let mut enc = vec![0; data.len()];
        let mut tag = vec![0; 16];
        cipher.encrypt(&data, &mut enc, &mut tag);
        enc.extend(tag);
        enc
    }
    pub fn decrypt(
        data: &[u8],
        encryption_iv: &[u8],
        decrypt_key: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, Error> {
        let mut cipher = AesGcm::new(KeySize::KeySize128, decrypt_key, encryption_iv, aad);

        let tag = &data[(data.len() - 16)..];
        let mut dec = vec![0; data.len() - 16];

        let decrypted = cipher.decrypt(&data[..(data.len() - 16)], &mut dec, tag);
        if decrypted {
            Ok(dec)
        } else {
            Err(Error::new("Failed to decrypt the message.", 5006))
        }
    }
}
