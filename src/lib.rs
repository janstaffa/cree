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
pub fn get_file_meta(path: &PathBuf) -> Result<FileMeta, String> {
   let name = path
      .file_stem()
      .and_then(OsStr::to_str)
      .ok_or("Invalid file name")?
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

pub async fn write_to_stream(stream: &mut BufReader<TcpStream>, data: &[u8]) -> Result<(), String> {
   if let Err(_) = stream.write_all(data).await {
      return Err(String::from("Failed to write data to the stream."));
   }
   Ok(())
}
