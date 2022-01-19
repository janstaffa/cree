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
        let path = PathBuf::from(&concatinated);
        let abs_root_path = self.root_dir.canonicalize().unwrap();
        if !path.exists() || !path.canonicalize().unwrap().starts_with(abs_root_path) {
            response.set_status(HTTPStatus::NotFound);
            response.write(NOT_FOUND.as_bytes().to_vec());
            return Ok(response);
        }

        let mut final_path: Option<PathBuf> = None;
        if path.is_dir() {
            let dir_files = fs::read_dir(&path).unwrap();
            for file in dir_files {
                let file = file.unwrap();
                let path = file.path();
                let FileMeta { name, extension } = get_file_meta(&path)?;
                if name == "index" {
                    if let Some(extension) = extension {
                        if let "html" | "htm" | "php" = &extension[..] {
                            final_path = Some(file.path());
                            break;
                        }
                    }
                }
            }
        } else if path.is_file() {
            final_path = Some(path);
        }

        if let Some(final_path) = final_path {
            let (mut data, content_type) = self.handle_file(&final_path, &response.req).await?;
            let FileMeta { extension, .. } = get_file_meta(&final_path)?;
            response.set_header("Content-type", &content_type);

            if extension == Some("php".to_owned()) {
                let mut headers_end = 0;

                let bytes: Vec<u8> = data
                    .iter()
                    .filter(|&byte| *byte != 0x0D as u8)
                    .cloned()
                    .collect();
                for (idx, byte) in bytes.iter().enumerate() {
                    if *byte == 0x0A as u8 {
                        if let Some(next_byte) = bytes.get(idx + 1) {
                            if *next_byte == 0x0A as u8 {
                                headers_end = idx;
                                break;
                            }
                        }
                    }
                }
                if headers_end > 0 {
                    let headers = &bytes[..headers_end];

                    let headers = String::from_utf8_lossy(headers);
                    let headers: Vec<&str> = headers.lines().collect();
                    for header in headers {
                        let split: Vec<&str> = header.split(":").collect();
                        if split.len() == 2 {
                            response.set_header(split[0].trim(), split[1].trim())
                        }
                    }

                    data = Vec::from(&bytes[headers_end + 2..]);
                }
            }

            response.set_status(HTTPStatus::Ok);

            response.write(data);
            return Ok(response);
        }

        response.set_status(HTTPStatus::NotFound);
        response.write(NOT_FOUND.as_bytes().to_vec());
        return Ok(response);
    }

    /// returns: (file_data: Vec<u8>, content_type: String)
    async fn handle_file(&self, path: &PathBuf, req: &Request) -> Result<(Vec<u8>, String), Error> {
        let FileMeta { extension, .. } = get_file_meta(&path)?;

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
