use bytes::Buf;
use libflate::{deflate::Encoder as DfEncoder, gzip::Encoder as GzEncoder};
use serde_derive::Deserialize;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::io::Read;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

pub mod api;
mod core;

#[derive(Debug, Deserialize, Clone)]
pub struct Headers {
    pub content_security_policy: Option<String>,
}
#[derive(Debug, Deserialize, Clone)]
pub struct CreeOptions {
    pub port: Option<u16>,
    pub enable_php: Option<bool>,
    pub root_directory: Option<PathBuf>,
    pub php_path: Option<PathBuf>,
    pub use_compression: Option<bool>,
    pub pc_chunk_size: Option<usize>,
    pub headers: Option<Headers>,
}
impl CreeOptions {
    pub fn get_default() -> CreeOptions {
        CreeOptions {
            port: Some(80),
            enable_php: Some(false),
            root_directory: None,
            php_path: None,
            use_compression: Some(true),
            pc_chunk_size: Some(M_BYTE),
            headers: None,
        }
    }
}

#[derive(Debug)]
pub struct Error {
    pub msg: String,
    pub code: u32,
}

impl Error {
    pub fn new(msg: &str, code: u32) -> Error {
        Error {
            msg: msg.to_owned(),
            code,
        }
    }
}

pub const M_BYTE: usize = 1048576;

pub fn join_bytes(bytes: &[u8]) -> Result<u64, Error> {
    if bytes.len() > 8 {
        return Err(Error::new("Invalid input. (max length is 8)", 1007));
    }
    let mut full_bytes = vec![0u8; 8 - bytes.len()];
    full_bytes.extend(bytes);

    let mut bytes = [0u8; 8];
    full_bytes
        .reader()
        .read(&mut bytes)
        .or(Err(Error::new("Failed to read the bytes.", 1002)))?;

    Ok(u64::from_be_bytes(bytes))
}
