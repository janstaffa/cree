use self::crypto::ECCurve;
use crate::utils::Error;

pub mod crypto;
pub mod digest;
pub mod protocol;
pub mod signature;

#[derive(Debug, Clone)]
pub struct Certificate {
    pub raw: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct TLSExtension {
    id: u16,
    content: Vec<u8>,
}

impl TLSExtension {
    pub fn new(id: u16, content: Vec<u8>) -> TLSExtension {
        TLSExtension { id, content }
    }
}

#[derive(Debug, Clone)]
pub enum KeyExchange {
    ECDHE { curve: ECCurve, public_key: Vec<u8> },
}

#[derive(Debug, Clone)]
pub enum CipherSuite {
    TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
}

impl CipherSuite {
    fn bytes(&self) -> [u8; 2] {
        match self {
            CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 => [0xc0, 0x2f],
        }
    }
}

#[derive(Debug, Clone)]
pub enum TLSVersion {
    TLS1_0,
    TLS1_1,
    TLS1_2,
}
impl TLSVersion {
    pub fn from(e: &[u8]) -> Result<TLSVersion, Error> {
        Ok(match e {
            &[0x03, 0x03] => TLSVersion::TLS1_2,
            _ => return Err(Error::new("Invalid TLS version.", 5001)),
        })
    }
    pub fn get_value(&self) -> [u8; 2] {
        match self {
            TLSVersion::TLS1_0 => [0x03, 0x01],
            TLSVersion::TLS1_1 => [0x03, 0x02],
            TLSVersion::TLS1_2 => [0x03, 0x03],
        }
    }
}

#[derive(Debug)]
pub enum TLSRecord {
    Handshake,
    ChangeCipherSpec,
    Alert,
    Application,
    Heartbeat,
}

impl TLSRecord {
    /// Returns the assigned numerical value equivalent. (ex: 22 - handshake)
    pub fn get_value(&self) -> u8 {
        match self {
            &Self::ChangeCipherSpec => 0x14,
            &Self::Alert => 0x15,
            &Self::Handshake => 0x16,
            &Self::Application => 0x17,
            &Self::Heartbeat => 0x18,
        }
    }
}
