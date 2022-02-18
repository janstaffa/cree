use crate::Error;
use ring::{rand, signature};

use super::digest::DigestAlgorithm;

#[derive(Debug, Clone)]
pub struct SignedData {
    pub signature_scheme: Signature,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum Signature {
    RSA_SHA256,
}

pub struct RSASignature {
    encryption_key: Vec<u8>,
}
impl RSASignature {
    pub fn new(encryption_key: Vec<u8>) -> RSASignature {
        RSASignature { encryption_key }
    }
    pub fn sign(&self, digest: DigestAlgorithm, data: &[u8]) -> Result<SignedData, Error> {
        let (padding, signature_scheme) = match digest {
            DigestAlgorithm::SHA256 => (&signature::RSA_PKCS1_SHA256, Signature::RSA_SHA256),
        };
        let key_pair = signature::RsaKeyPair::from_der(&self.encryption_key).unwrap();

        // Sign the message, using PKCS#1 v1.5 padding and the
        // SHA256 digest algorithm.
        let rng = rand::SystemRandom::new();
        let mut signature = vec![0; key_pair.public_modulus_len()];
        key_pair.sign(padding, &rng, &data, &mut signature).unwrap();
        Ok(SignedData {
            signature_scheme,
            data: signature,
        })
    }
}
