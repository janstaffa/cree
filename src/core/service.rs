use super::codes::NOT_FOUND;
use super::http::Method;
use crate::core::codes::HTTPStatus;
use crate::core::http::construct_http_interface;
use crate::core::http::Response;
use crate::core::mime::get_mime_type;
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
      // let (mut read, mut write) = tokio::io::split(socket);
      // let connection = Connection::new(socket);
      let (req, mut res) = construct_http_interface(socket).await?;
      // println!("req {:?}", req);

      res.set_header("Connection", "close");
      res.set_header("Server", "Cree");
      if let Some(headers) = &self.options.headers {
         if let Some(csp) = &headers.content_security_policy {
            res.set_header("Content-Security-Policy", csp);
         }
      }

      let concatinated = format!("{}{}", self.root_dir.display(), req.path);
      let final_path = PathBuf::from(&concatinated);
      let abs_root_path = self.root_dir.canonicalize().unwrap();
      if !final_path.exists()
         || !final_path
            .canonicalize()
            .unwrap()
            .starts_with(abs_root_path)
      {
         res.write_response(NOT_FOUND.as_bytes(), HTTPStatus::NotFound, true)
            .await
            .unwrap();
         return Ok(());
      }
      if final_path.is_dir() {
         let dir_files = fs::read_dir(&final_path).unwrap();
         for file in dir_files {
            let file = file.unwrap();
            let path = file.path();
            let FileMeta { name, extension } = get_file_meta(&path)?;
            if name == "index" {
               if let Some(extension) = extension {
                  if let "html" | "htm" | "php" = &extension[..] {
                     res.send_file(file.path(), &self.php_handle, &self.root_dir)
                        .await
                        .unwrap();
                     return Ok(());
                  }
               }
            }
         }
      } else if final_path.is_file() {
         res.send_file(final_path, &self.php_handle, &self.root_dir)
            .await
            .unwrap();
         return Ok(());
      }

      res.write_response(NOT_FOUND.as_bytes(), HTTPStatus::NotFound, true)
         .await
         .unwrap();
      Ok(())
   }
}
impl Response {
   pub async fn send_file(
      &mut self,
      path: PathBuf,
      php_handle: &Option<PHP>,
      root_path: &PathBuf,
   ) -> Result<(), Error> {
      let file_meta = get_file_meta(&path)?;
      let FileMeta { extension, .. } = file_meta;

      if let Some(extension) = &extension {
         if extension == "php" {
            if let Some(php_handle) = php_handle {
               let variables = PHPVariables {
                  request_method: &self.req.method.to_string().unwrap(),
                  post_data: Some(&self.req.body),
                  content_length: Some(self.req.body.as_bytes().len()),
                  content_type: self.req.headers.get("content-type"),
                  remote_addr: &format!("{:?}", self.req.remote_address.ip()),
                  query_string: &self.req.query,
                  document_root: root_path.to_str().unwrap(),
                  request_protocol: &self.req.http_info,
                  request_uri: &self.req.uri,
                  http_host: "",
               };
               let data = php_handle.execute(&path, &variables).await?;

               self.set_header("Content-Type", "text/html");
               self
                  .write_response(&data, HTTPStatus::Ok, false)
                  .await
                  .unwrap();

               return Ok(());
            } else {
               return Err(Error::new("Invalid PHP configuration.", 3000));
            }
         }
      }
      if let Ok(file) = fs::read(&path) {
         let ext = extension.unwrap_or_default();
         let media_type = get_mime_type(&ext);
         self.set_header("Content-Type", &media_type);
         self
            .write_response(&file, HTTPStatus::Ok, true)
            .await
            .unwrap();
         return Ok(());
      }
      Err(Error::new("Something went wrong.", 1000))
   }
}
