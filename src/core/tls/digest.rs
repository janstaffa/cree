use crypto::{hmac, sha2};

type _HmacSha256 = hmac::Hmac<sha2::Sha256>;
pub struct HmacSha256;

impl HmacSha256 {
    pub fn new(key: &[u8]) -> _HmacSha256 {
        let sha256_hasher = sha2::Sha256::new();
        _HmacSha256::new(sha256_hasher, key)
    }
}

pub enum DigestAlgorithm {
    SHA256,
}
