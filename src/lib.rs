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

pub struct FileMeta {
   pub name: String,
   pub extension: String,
}

pub fn get_file_meta(path: &PathBuf) -> Result<FileMeta, Error> {
   let name = path
      .file_stem()
      .and_then(OsStr::to_str)
      .ok_or(Error::new("Invalid file name"))?
      .to_owned();
   let extension = path
      .extension()
      .and_then(OsStr::to_str)
      .unwrap_or("")
      .to_owned();

   // let file_name = path
   //    .file_name()
   //    .ok_or("Invalid file name.")?
   //    .to_str()
   //    .ok_or("Invalid file name.")?;
   // let split: Vec<&str> = file_name.split('.').collect();
   // let len = split.len();
   // if len < 2 {
   //    return Err(String::from("Invalid file name."));
   // }
   // let name = split[..len - 1].join("");
   // let extension = split[len - 1].to_owned();
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
