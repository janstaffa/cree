use super::codes::NOT_FOUND;
use crate::core::codes::HTTPStatus;
use crate::core::http::{Request, Response};
use crate::core::mime::get_mime_type;
use crate::extensions::php::PHPVariables;
use crate::extensions::php::{PHPOptions, PHP};
use cree::{get_file_meta, FileMeta};
use cree::{CreeOptions, Range};
use cree::{Error, M_BYTE};
use std::fs;
use std::io::SeekFrom;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

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

    /// Creates a Response from a Request.
    pub async fn create_response(&self, request: Request) -> Result<Response, Error> {
        // create a blank response
        let mut response = Response::new(request.clone());

        // set some headers known in advance
        response.set_header("Server", "Cree");
        if let Some(headers) = &self.options.headers {
            if let Some(csp) = &headers.content_security_policy {
                response.set_header("Content-Security-Policy", csp);
            }
        }

        // construct the absolute path to the requested file from the request uri
        let concatinated = format!("{}{}", self.root_dir.display(), request.path);
        let path = PathBuf::from(&concatinated);
        let abs_root_path = self.root_dir.canonicalize().unwrap();
        if !path.exists() || !path.canonicalize().unwrap().starts_with(abs_root_path) {
            response.set_status(HTTPStatus::NotFound);
            response.write(NOT_FOUND.as_bytes().to_vec());
            return Ok(response);
        }

        let mut final_path: Option<PathBuf> = None;

        // if the requested uri is a directory, try to find an index file inside
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
            let range = request.headers.get("range");

            // check if the user requested a Range of bytes
            let range: Option<Range> = if let Some(range) = range {
                let mut rg = None;
                let split: Vec<&str> = range.split("=").collect();

                if split.len() == 2 {
                    let (unit, ranges) = (split[0], split[1]);
                    if unit == "bytes" {
                        let values: Vec<&str> = ranges.split("-").collect();
                        if values.len() > 1 {
                            let from = if let Some(from) = values.get(0) {
                                from.parse::<usize>().ok()
                            } else {
                                None
                            };
                            let to = if let Some(to) = values.get(1) {
                                to.parse::<usize>().ok()
                            } else {
                                None
                            };
                            rg = Some(Range::new(from, to))
                        }
                    }
                }

                rg
            } else {
                None
            };

            // this method actually reads the the file contents and handles them accordingly to it's metadata
            let (mut data, content_type, content_length) =
                self.handle_file(&final_path, &response.req, &range).await?;

            // if the user has requested a range of bytes, set the Content-Range header
            if let Some(range) = range {
                let (from, to) = match (range.from, range.to) {
                    (Some(from), Some(to)) => (from, to),
                    (Some(from), None) => (
                        from,
                        std::cmp::min(from + data.len() - 1, content_length - 1),
                    ),
                    (None, Some(to)) => {
                        let from = if to > content_length {
                            content_length
                        } else {
                            content_length - to
                        };
                        (from, content_length)
                    }
                    (None, None) => {
                        response.set_status(HTTPStatus::RangeNotSatisfiable);
                        response.set_header("Content-Range", &format!("*/{}", content_length));
                        return Ok(response);
                    }
                };
                //  let end = if let Some(end) = range.to {
                //      end
                //  } else {
                //      std::cmp::min(range.from + data.len() - 1, content_length - 1)
                //  };

                response.set_header(
                    "Content-Range",
                    &format!("bytes {}-{}/{}", from, to, content_length),
                );
                response.set_status(HTTPStatus::PartialContent);
            }

            let FileMeta { extension, .. } = get_file_meta(&final_path)?;
            response.set_header("Content-type", &content_type);

            // if the file is php, there are extra headers added at the beggining by the php-cgi, which need to get removed and added to the response headers
            // the headers are separated from the content by a double newline(\n\n or \r\n\r\n)
            if extension == Some("php".to_owned()) {
                let mut headers_end = 0;

                // remove all \r bytes
                let bytes: Vec<u8> = data
                    .iter()
                    .filter(|&byte| *byte != 0x0D as u8)
                    .cloned()
                    .collect();

                // find the first double newline
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
                    // parse the headers and add them to the response
                    let headers = &bytes[..headers_end];

                    let headers = String::from_utf8_lossy(headers);
                    let headers: Vec<&str> = headers.lines().collect();
                    for header in headers {
                        let split: Vec<&str> = header.split(":").collect();
                        if split.len() == 2 {
                            response.set_header(split[0].trim(), split[1].trim())
                        }
                    }

                    // replace the old response body with one without the headers
                    data = Vec::from(&bytes[headers_end + 2..]);
                }
            }

            // set the status to 200 OK if it wasnt changed already
            if let HTTPStatus::Accepted = response.get_status() {
                response.set_status(HTTPStatus::Ok);
            }

            // store the body inside the response struct - this doesnt actually send it to the client
            response.write(data);
            return Ok(response);
        }

        response.set_status(HTTPStatus::NotFound);
        response.write(NOT_FOUND.as_bytes().to_vec());
        return Ok(response);
    }

    /// Returns requested content from a file according to a request. Returns (file_data, mime_type, full_file_size)
    async fn handle_file(
        &self,
        path: &PathBuf,
        req: &Request,
        range: &Option<Range>,
    ) -> Result<(Vec<u8>, String, usize), Error> {
        let FileMeta { extension, .. } = get_file_meta(&path)?;

        // check if the file is php, if yes give it to the php-cgi and return the resulting html
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

                    let data_len = data.len();
                    return Ok((data, String::from("text/html"), data_len));
                } else {
                    return Err(Error::new("Invalid PHP configuration.", 3000));
                }
            }
        }

        // open the requested file
        let mut file = File::open(&path)
            .await
            .or(Err(Error::new("Failed to open the file.", 1005)))?;

        let file_size = file.metadata().await.unwrap().len() as usize;

        // get real start and end positions for reading the file, if no range was specified, take the whole file

        let (start, end) = if let Some(rg) = range {
            let chunk = self.options.pc_chunk_size.unwrap_or(M_BYTE);
            match (rg.from, rg.to) {
                (Some(from), Some(to)) => (from, to + 1),
                (Some(from), None) => (from, std::cmp::min(from + chunk, file_size)),
                (None, Some(to)) => {
                    let from = if to > file_size {
                        file_size
                    } else {
                        file_size - to
                    };
                    (from, file_size)
                }
                (None, None) => (0, 0),
            }
            // let to = if let Some(to) = rg.to {
            //     if to > file_size - 1 {
            //         file_size
            //     } else {
            //         to + 1
            //     }
            // } else {
            //     let chunk = self.options.pc_chunk_size.unwrap_or(M_BYTE);
            //     std::cmp::min(rg.from + chunk, file_size)
            // };
            // (rg.from, to)
        } else {
            (0, file_size)
        };

        // read the requested bytes
        let mut read_len = file_size;

        if end > start {
            // jump to range start (if no range requested, 0 is used)
            file.seek(SeekFrom::Start(start as u64)).await;

            read_len = end - start;
        }

        let mut buf = vec![0; read_len];
        file.read_exact(&mut buf).await.unwrap();
        // .or(Err(Error::new("Failed to read the file.", 1005)))?;

        // get mime type and return
        let ext = extension.unwrap_or_default();
        let media_type = get_mime_type(&ext);
        return Ok((buf, media_type, file_size));
    }
}
