use libflate::{deflate::Encoder as DfEncoder, gzip::Encoder as GzEncoder};
use serde_derive::Deserialize;
use std::ffi::OsStr;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

#[derive(Debug, Deserialize, Clone)]
pub struct Headers {
    pub content_security_policy: Option<String>,
}
#[derive(Debug, Deserialize, Clone)]
pub struct CreeOptions {
    pub enable_php: Option<bool>,
    pub root_directory: Option<PathBuf>,
    pub php_path: Option<PathBuf>,
    pub headers: Option<Headers>,
}
impl CreeOptions {
    pub fn get_default() -> CreeOptions {
        CreeOptions {
            enable_php: Some(false),
            root_directory: None,
            php_path: None,
            headers: None,
        }
    }
}

pub struct FileMeta<'b> {
    pub name: &'b str,
    pub extension: Option<String>,
}

pub fn get_file_meta<'a>(path: &'a PathBuf) -> Result<FileMeta<'a>, Error> {
    let name = path
        .file_stem()
        .and_then(OsStr::to_str)
        .ok_or(Error::new("Invalid file name", 1001))?;
    let extension = path.extension().and_then(OsStr::to_str);

    let extension = if let Some(ext) = extension {
        Some(ext.to_lowercase())
    } else {
        None
    };
    let meta = FileMeta { name, extension };
    Ok(meta)
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

pub async fn close_socket(mut socket: TcpStream) -> Result<(), Error> {
    if let Err(_) = socket.shutdown().await {
        return Err(Error::new("Failed to close the connection.", 1004));
    }
    Ok(())
}
