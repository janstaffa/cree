use super::codes::NOT_FOUND;
use super::http::Method;
use crate::core::codes::HTTPStatus;
use crate::core::http::construct_http_interface;
use crate::core::http::Connection;
use crate::core::http::Response;
use crate::extensions::php::PHPVariables;
use crate::extensions::php::{PHPOptions, PHP};
use cree::CreeOptions;
use cree::Error;
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
   pub fn new(root_dir: PathBuf, options: CreeOptions) -> Result<CreeService, Error> {
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
   pub async fn handle_request(&self, socket: TcpStream) -> Result<(), Error> {
      let connection = Mutex::new(Connection::new(socket));
      let (req, mut res) = construct_http_interface(&connection).await;

      if let Method::GET = req.method {
         let concatinated = format!("{}{}", self.root_dir.display(), req.path);
         let final_path = PathBuf::from(&concatinated);
         let abs_root_path = self.root_dir.canonicalize().unwrap();
         if !final_path.exists()
            || !final_path
               .canonicalize()
               .unwrap()
               .starts_with(abs_root_path)
         {
            res.write(NOT_FOUND.as_bytes(), HTTPStatus::NotFound)
               .await
               .unwrap();
            return Ok(());
         }
         if final_path.is_dir() {
            let dir_files = fs::read_dir(&final_path).unwrap();
            for file in dir_files {
               let file = file.unwrap();
               if file.file_name() == "index.html" {
                  res.send_file(file.path(), &self.php_handle, &self.root_dir)
                     .await
                     .unwrap();
                  return Ok(());
               }
            }
         } else if final_path.is_file() {
            res.send_file(final_path, &self.php_handle, &self.root_dir)
               .await
               .unwrap();
            return Ok(());
         }
      }

      res.write(NOT_FOUND.as_bytes(), HTTPStatus::NotFound)
         .await
         .unwrap();
      Ok(())
   }
}
impl<'a> Response<'a> {
   pub async fn send_file(
      &mut self,
      path: PathBuf,
      php_handle: &Option<PHP>,
      root_path: &PathBuf,
   ) -> Result<(), Error> {
      self.set_header("hello", "this is a test ");
      let file_meta = get_file_meta(&path)?;
      let FileMeta { extension, .. } = file_meta;
      let connection = self.connection.lock().await;
      if extension == "php" {
         if let Some(php_handle) = php_handle {
            let variables = PHPVariables {
               request_method: String::from("GET"),
               remote_addr: format!("{:?}", connection.remote_address.ip()),
               query_string: self.req.query.clone(),
               document_root: String::from(root_path.to_str().unwrap()),
               request_protocol: self.req.http_info.clone(),
               request_uri: self.req.uri.clone(),
               http_host: String::new(),
            };
            let data = php_handle.execute(&path, &variables).await?;

            std::mem::drop(connection);
            self.write(&data, HTTPStatus::Ok).await.unwrap();

            return Ok(());
         } else {
            return Err(Error::new("Invalid PHP configuration."));
         }
      }
      if let Ok(file) = fs::read(&path) {
         std::mem::drop(connection);
         self.write(&file, HTTPStatus::Ok).await.unwrap();
      }
      Ok(())
   }
}
