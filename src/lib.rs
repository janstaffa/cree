use serde_derive::Deserialize;
use std::ffi::OsStr;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::TcpStream;

#[derive(Debug, Deserialize, Clone)]
pub struct CreeOptions {
   pub enable_php: Option<bool>,
   pub php_path: Option<PathBuf>,
}
impl CreeOptions {
   pub fn get_default() -> CreeOptions {
      CreeOptions {
         enable_php: Some(false),
         php_path: None,
      }
   }
}

pub struct FileMeta<'b> {
   pub name: &'b str,
   pub extension: Option<&'b str>,
}

pub fn get_file_meta<'a>(path: &'a PathBuf) -> Result<FileMeta<'a>, Error> {
   let name = path
      .file_stem()
      .and_then(OsStr::to_str)
      .ok_or(Error::new("Invalid file name"))?;
   let extension = path.extension().and_then(OsStr::to_str);

   let meta = FileMeta { name, extension };
   Ok(meta)
}

#[derive(Debug)]
pub struct Error {
   msg: String,
}

impl Error {
   pub fn new(msg: &str) -> Error {
      Error {
         msg: msg.to_owned(),
      }
   }
}
