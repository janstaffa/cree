use super::http::Method;
use super::responses::NOT_FOUND;
use crate::core::http::construct_http_interface;
use crate::core::http::Connection;
use crate::core::http::Response;
use crate::extensions::php::PHPVariables;
use crate::extensions::php::{PHPOptions, PHP};
use cree::write_to_stream;
use cree::CreeOptions;
use cree::{get_file_meta, FileMeta};
use futures::lock::Mutex;
use std::fs;
use std::path::PathBuf;
use tokio::net::TcpStream;

#[derive(Clone)]
pub struct CreeService {
   root_dir: PathBuf,
   options: CreeOptions,
   php_handle: Option<PHP>,
}

impl CreeService {
   pub fn new(root_dir: PathBuf, options: CreeOptions) -> Result<CreeService, String> {
      let mut php_handle: Option<PHP> = None;
      if let Some(true) = &options.enable_php {
         let options = options.clone();
         let php_options = PHPOptions {
            php_path: options.php_path,
         };
         php_handle = Some(PHP::setup(&php_options)?);
      };
      let service = CreeService {
         root_dir,
         options,
         php_handle,
      };
      Ok(service)
   }
   pub async fn handle_request(&self, socket: TcpStream) -> Result<(), String> {
      let connection = Mutex::new(Connection::new(socket));
      let (req, mut res) = construct_http_interface(&connection).await;

      if let Method::GET = req.method {
         let concatinated = format!("{}{}", self.root_dir.display(), req.path.display());
         let final_path = PathBuf::from(&concatinated);
         let abs_root_path = self.root_dir.canonicalize().unwrap();
         if !final_path.exists()
            || !final_path
               .canonicalize()
               .unwrap()
               .starts_with(abs_root_path)
         {
            res.write(NOT_FOUND.as_bytes()).await.unwrap();
            return Ok(());
         }
         if final_path.is_dir() {
            let dir_files = fs::read_dir(&final_path).unwrap();
            for file in dir_files {
               let file = file.unwrap();
               if file.file_name() == "index.html" {
                  res.send_file(file.path(), &self.php_handle).await.unwrap();
                  return Ok(());
               }
            }
         } else if final_path.is_file() {
            res.send_file(final_path, &self.php_handle).await.unwrap();
            return Ok(());
         }
      }

      res.write(NOT_FOUND.as_bytes()).await.unwrap();
      Ok(())
   }
}
impl<'a> Response<'a> {
   pub async fn write(&mut self, data: &[u8]) -> Result<(), String> {
      if self.is_fulfilled() {
         return Err(String::from(
            "Cannot write to a response that has already been sent.",
         ));
      }
      let mut conn = self.connection.lock().await;
      let writable_stream = &mut conn.stream;

      if let Err(e) = write_to_stream(writable_stream, data).await {
         conn.close().await?;
         return Err(e);
      }
      Ok(())
   }
   pub async fn send_file(
      &mut self,
      path: PathBuf,
      php_handle: &Option<PHP>,
   ) -> Result<(), String> {
      let file_meta = get_file_meta(&path)?;
      let FileMeta { extension, .. } = file_meta;
      let mut connection = self.connection.lock().await;
      if extension == "php" {
         if let Some(php_handle) = php_handle {
            let variables = PHPVariables {
               remote_addr: connection.remote_address.clone(),
            };
            let data = php_handle.execute(&path, &variables).await?;

            if let Err(e) = write_to_stream(&mut connection.stream, &data).await {
               connection.close().await?;
               return Err(e);
            }
            return Ok(());
         } else {
            return Err(String::from("Invalid PHP configuration."));
         }
      }
      if let Ok(file) = fs::read(&path) {
         if let Err(e) = write_to_stream(&mut connection.stream, &file).await {
            connection.close().await?;
            return Err(e);
         }
      }
      Ok(())
   }
}
