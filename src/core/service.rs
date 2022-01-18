use super::codes::NOT_FOUND;
use super::http::Method;
use crate::core::codes::HTTPStatus;
use crate::core::http::{Request, Response};
use crate::core::mime::get_mime_type;
use crate::extensions::php::PHPVariables;
use crate::extensions::php::{PHPOptions, PHP};
use crate::HTTPConnection;
use cree::CreeOptions;
use cree::Error;
use cree::{get_file_meta, FileMeta};
use futures::lock::Mutex;
use std::fs;
use std::path::PathBuf;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

#[derive(Clone)]
pub struct CreeService {
    root_dir: PathBuf,
    options: CreeOptions,
    php_handle: Option<PHP>,
}

impl CreeService {
    pub fn new(root_dir: &PathBuf, options: &CreeOptions) -> Result<CreeService, Error> {
        let mut php_handle: Option<PHP> = None;
        if let Some(true) = &options.enable_php {
            let options = options.clone();
            let php_options = PHPOptions {
                php_path: options.php_path,
            };
            php_handle = Some(PHP::setup(&php_options)?);
        };
        let service = CreeService {
            root_dir: root_dir.clone(),
            options: options.clone(),
            php_handle,
        };
        Ok(service)
    }

    pub async fn create_response(&self, request: Request) -> Result<Response, Error> {
        let mut response = Response::new(request);

        response.set_header("Server", "Cree");
        if let Some(headers) = &self.options.headers {
            if let Some(csp) = &headers.content_security_policy {
                response.set_header("Content-Security-Policy", csp);
            }
        }

        let concatinated = format!("{}{}", self.root_dir.display(), response.req.path);
        let final_path = PathBuf::from(&concatinated);
        let abs_root_path = self.root_dir.canonicalize().unwrap();
        if !final_path.exists()
            || !final_path
                .canonicalize()
                .unwrap()
                .starts_with(abs_root_path)
        {
            response.set_status(HTTPStatus::NotFound);
            response.write(NOT_FOUND.as_bytes().to_vec());
            return Ok(response);
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
                            let (data, content_type) =
                                self.handle_file(file.path(), &response.req).await?;
                            response.set_header("Content-Type", &content_type);
                            response.set_status(HTTPStatus::Ok);
                            response.write(data);
                            return Ok(response);
                        }
                    }
                }
            }
        } else if final_path.is_file() {
            let (data, content_type) = self.handle_file(final_path, &response.req).await?;
            response.set_header("Content-Type", &content_type);
            response.set_status(HTTPStatus::Ok);
            response.write(data);
            return Ok(response);
        }

        response.set_status(HTTPStatus::NotFound);
        response.write(NOT_FOUND.as_bytes().to_vec());
        Ok(response)
    }

    /// returns: (file_data: Vec<u8>, content_type: String)
    async fn handle_file(&self, path: PathBuf, req: &Request) -> Result<(Vec<u8>, String), Error> {
        let file_meta = get_file_meta(&path)?;
        let FileMeta { extension, .. } = file_meta;

        if let Some(extension) = &extension {
            if extension == "php" {
                if let Some(php_handle) = &self.php_handle {
                    let Request {
                        body,
                        method,
                        headers,
                        remote_address,
                        query,
                        http_info,
                        uri,
                        ..
                    } = req;
                    let variables = PHPVariables {
                        request_method: &method.to_string().unwrap(),
                        post_data: Some(&body),
                        content_length: Some(body.as_bytes().len()),
                        content_type: headers.get("content-type"),
                        remote_addr: &format!("{:?}", remote_address.ip()),
                        query_string: &query,
                        document_root: &self.root_dir.to_str().unwrap(),
                        request_protocol: &http_info,
                        request_uri: &uri,
                        http_host: "",
                    };
                    let data = php_handle.execute(&path, &variables).await?;

                    return Ok((data, String::from("text/html")));
                } else {
                    return Err(Error::new("Invalid PHP configuration.", 3000));
                }
            }
        }
        if let Ok(file) = fs::read(&path) {
            let ext = extension.unwrap_or_default();
            let media_type = get_mime_type(&ext);
            return Ok((file, media_type));
        }
        Err(Error::new("Something went wrong.", 1000))
    }
}
// impl Response {
//    pub async fn write_response(
//       &mut self,
//       data: &[u8],
//       mut status_code: HTTPStatus,
//       insert_newline: bool,
//    ) -> Result<(), Error> {
//       if self.is_fulfilled() {
//          return Err(Error::new(
//             "Cannot write to a response that has already been sent.",
//             2000,
//          ));
//       }

//       if let Method::Unknown = self.req.method {
//          self.set_header("Allow", "GET,HEAD,POST");
//          status_code = HTTPStatus::MethodNotAllowed;
//       }
//       let code = get_phrase_from_code(&status_code).ok_or(Error::new(
//          &format!("Invalid status code: {:?}.", &status_code),
//          2003,
//       ))?;
//       let http_header = format!("HTTP/1.1 {} {}\n", code.0, code.1);

//       let date = Utc::now().format("%a, %d %b %Y %T %Z");
//       let date = format!("{}", date);
//       self.set_header("Date", &date);

//       let mut headers = [http_header.as_bytes(), self.get_headers().as_bytes()].concat();
//       if insert_newline {
//          headers.push(0x0A); // newline
//       }

//       let mut final_data: Vec<u8> = vec![];
//       match self.req.method {
//          Method::HEAD | Method::Unknown => {
//             final_data = headers;
//          }
//          _ => {
//             final_data = [&headers, data].concat();
//          }
//       };

//       self.connection.write(&mut final_data).await?;
//       Ok(())
//    }
// }
