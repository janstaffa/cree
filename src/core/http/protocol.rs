use crate::core::http::codes::get_phrase_from_code;
use crate::core::http::Encoding;
use crate::Error;
use async_trait::async_trait;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use libflate::{deflate::Encoder as DfEncoder, gzip::Encoder as GzEncoder};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use super::codes::HTTPStatus;

#[derive(Debug, Clone)]
pub enum Method {
    GET,
    HEAD,
    POST,
    Unknown,
}
impl Method {
    pub fn to_string(&self) -> Option<String> {
        if let Method::Unknown = self {
            return None;
        }
        Some(format!("{:?}", self))
    }
}

impl PartialEq for Method {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Debug, Clone)]
pub struct Request {
    // pub connection: ReadHalf<TcpStream>,
    pub remote_address: SocketAddr,
    time_received: DateTime<Utc>,
    pub method: Method,
    pub path: String,
    pub uri: String,
    pub body: String,
    pub query: String,
    pub http_info: String,
    pub headers: Headers,
}

impl Request {
    pub fn new(
        // mut connection: ReadHalf<TcpStream>,
        req_data: Vec<u8>,
        remote_address: SocketAddr,
    ) -> Result<Request, Error> {
        let req_data = String::from_utf8_lossy(&req_data);

        let ParsedRequest {
            method,
            uri,
            path,
            query,
            http_info,
            body,
            headers,
        } = parse_request(&req_data)?;
        let req = Request {
            // connection,
            remote_address,
            time_received: Utc::now(),
            method,
            path,
            uri,
            body,
            query: query,
            http_info,
            headers,
        };
        Ok(req)
    }

    /// DateTime of when the connection was established
    pub fn time_received(&self) -> DateTime<Utc> {
        self.time_received
    }

    /// Duration of the connection as of calling this function
    pub fn duration(&self) -> Duration {
        Utc::now() - self.time_received
    }
}

type Headers = HashMap<String, String>;
#[derive(Debug)]
pub struct Response {
    write_handle: Arc<Mutex<WriteHalf<TcpStream>>>,
    req: Request,
    sent: bool,
    headers: Headers,
    status: HTTPStatus,
    use_compression: bool,
    is_last: bool,
}

impl Response {
    pub fn __new(
        write_handle: Arc<Mutex<WriteHalf<TcpStream>>>,
        req: Request,
        use_compression: bool,
        is_last: bool,
    ) -> Response {
        Response {
            write_handle,
            req,
            sent: false,
            headers: HashMap::new(),
            status: HTTPStatus::Accepted,
            use_compression,
            is_last,
        }
    }
    pub fn get_headers(&mut self) -> String {
        let mut headers = String::new();
        for (key, value) in &self.headers {
            let mut header = String::new();
            header.push_str(&key);
            if value.len() > 0 {
                header.push_str(": ");
            }
            header.push_str(&value);
            header.push_str("\n");
            headers.push_str(&header);
        }
        headers
    }
    pub fn get_header(&self, key: &str) -> Option<&String> {
        self.headers.get(key)
    }
    pub fn set_header(&mut self, key: &str, value: &str) {
        self.headers.insert(key.to_owned(), value.to_owned());
    }
    pub fn remove_header(&mut self, key: &str) {
        self.headers.remove(key);
    }

    pub fn get_status(&self) -> &HTTPStatus {
        &self.status
    }
    pub fn set_status(&mut self, status: HTTPStatus) {
        self.status = status;
    }

    pub async fn send(&mut self, data: &[u8]) -> Result<(), String> {
        if self.sent {
            return Err("Cannot write to a response that has already been sent.".into());
        }
        self.sent = true;

        // create all headers
        let status = self.get_status();
        let code =
            get_phrase_from_code(&status).ok_or(format!("Invalid status code: {:?}.", status))?;
        let http_header = format!("HTTP/1.1 {} {}\n", code.0, code.1);

        let date = Utc::now().format("%a, %d %b %Y %T %Z");
        let date = format!("{}", date);
        self.set_header("Date", &date);

        let mut connection_status = "keep-alive";
        if self.is_last {
            connection_status = "close";
        }
        self.set_header("Connection", connection_status);

        let mut body = data.to_vec();

        // use compression if necessary
        if self.use_compression {
            let accept_encoding = self.req.headers.get("accept-encoding");
            if let Some(accept_encoding) = accept_encoding {
                let accept_encoding: Vec<&str> = accept_encoding.split(",").collect();
                let accept_encoding: Vec<String> = accept_encoding
                    .iter()
                    .map(|&i| i.trim().to_lowercase())
                    .collect();
                let has_gzip = accept_encoding.contains(&String::from("gzip"));
                let has_deflate = accept_encoding.contains(&String::from("deflate"));

                let content = &data;
                let mut content_encoding: Option<Encoding> = None;

                // Gzip usually increases file size in files with less than 1000 bytes
                if content.len() > 1000 && has_gzip {
                    content_encoding = Some(Encoding::Gzip);
                } else if has_deflate {
                    content_encoding = Some(Encoding::Deflate);
                };

                if let Some(content_encoding) = content_encoding {
                    let (encoded_data, encoding_name) = match content_encoding {
                        Encoding::Gzip => {
                            let mut encoder = GzEncoder::new(Vec::new()).unwrap();
                            std::io::copy(&mut &content[..], &mut encoder).unwrap();
                            (encoder.finish().into_result().unwrap(), "gzip")
                        }
                        Encoding::Deflate => {
                            let mut encoder = DfEncoder::new(Vec::new());
                            std::io::copy(&mut &content[..], &mut encoder).unwrap();
                            (encoder.finish().into_result().unwrap(), "deflate")
                        }
                    };

                    self.set_header("Content-Encoding", encoding_name);
                    body = encoded_data;
                }
            }
        }

        self.set_header("Content-Length", &body.len().to_string());

        let mut headers = [http_header.as_bytes(), self.get_headers().as_bytes()].concat();

        headers.push(0x0A);

        let final_data = match self.req.method {
            Method::HEAD => headers,
            _ => [&headers, &body[..]].concat(),
        };

        let mut connection = self.write_handle.lock().await;

        connection.write_all(&final_data).await.unwrap();
        Ok(())
    }
}

#[derive(Debug)]
struct ParsedRequest {
    method: Method,
    uri: String,
    path: String,
    http_info: String,
    query: String,
    body: String,
    headers: Headers,
}

// takes the raw UTF-8 request and extracts message data from it
fn parse_request(req: &str) -> Result<ParsedRequest, Error> {
    let req = req.trim();

    let req = req.replace("\r", "");

    // split body and header section
    let parts: Vec<&str> = req.split("\n\n").collect();
    if parts.len() == 0 {
        return Err(Error::new("Invalid request", 2001));
    }
    let head: Vec<&str> = parts[0].lines().collect();
    if head.len() < 1 {
        return Err(Error::new("Invalid request", 2001));
    }
    let request_line: Vec<&str> = head[0].split_whitespace().collect();

    // extract headers
    let raw_headers = &head[1..];
    let mut headers: Headers = HashMap::new();
    if raw_headers.len() > 0 {
        for header in raw_headers {
            let parts: Vec<&str> = header.split(":").collect();
            if parts.len() != 2 {
                continue;
            }

            headers.insert(parts[0].trim().to_lowercase(), parts[1].trim().to_owned());
        }
    }

    if request_line.len() < 3 {
        return Err(Error::new("Invalid request.", 2001));
    }
    let method: Method = match request_line[0] {
        "GET" => Method::GET,
        "HEAD" => Method::HEAD,
        "POST" => Method::POST,
        _ => {
            Method::Unknown
            // return Err(Error::new("Invalid request method.", 2002));
        }
    };

    // separate uri and path
    let uri = request_line[1];
    let query: &str = if uri.contains("?") {
        let split: Vec<&str> = uri.split("?").collect();

        let mut r = "";
        if split.len() == 2 {
            r = split[1];
        }
        r
    } else {
        ""
    };
    let path = uri.replace(&format!("?{}", query), "");
    let parsed = ParsedRequest {
        method,
        uri: uri.to_owned(),
        path,
        http_info: String::from(request_line[2]),
        query: query.to_owned(),
        body: if parts.len() == 2 {
            parts[1].to_owned()
        } else {
            String::from("")
        },
        headers,
    };
    Ok(parsed)
}

// #[derive(Debug)]
// pub struct Response {
//     pub req: Request,
//     fulfilled: bool,
//     headers: Headers,
//     body: Vec<u8>,
//     status: HTTPStatus,
// }

// impl Response {
//     pub fn new(req: Request) -> Response {
//         Response {
//             req,
//             fulfilled: false,
//             headers: HashMap::new(),
//             body: Vec::new(),
//             status: HTTPStatus::Accepted,
//         }
//     }

//     pub fn is_fulfilled(&self) -> bool {
//         self.fulfilled
//     }
//     pub fn get_headers(&mut self) -> String {
//         let mut headers = String::new();
//         for (key, value) in &self.headers {
//             let mut header = String::new();
//             header.push_str(&key);
//             if value.len() > 0 {
//                 header.push_str(": ");
//             }
//             header.push_str(&value);
//             header.push_str("\n");
//             headers.push_str(&header);
//         }
//         headers
//     }
//     pub fn get_header(&self, key: &str) -> Option<&String> {
//         self.headers.get(key)
//     }
//     pub fn set_header(&mut self, key: &str, value: &str) {
//         self.headers.insert(key.to_owned(), value.to_owned());
//     }
//     pub fn remove_header(&mut self, key: &str) {
//         self.headers.remove(key);
//     }

//     pub fn get_status(&self) -> &HTTPStatus {
//         &self.status
//     }
//     pub fn set_status(&mut self, status: HTTPStatus) {
//         self.status = status;
//     }
//     pub fn read_body(&self) -> &Vec<u8> {
//         &self.body
//     }
//     pub fn write(&mut self, data: &[u8]) {
//         self.body.extend(data);
//     }

//     pub fn construct(&mut self, use_compression: bool) -> Result<Vec<u8>, Error> {
//         if self.is_fulfilled() {
//             return Err(Error::new(
//                 "Cannot write to a response that has already been sent.",
//                 2000,
//             ));
//         }

//         // create all headers
//         let status = self.get_status();
//         let code = get_phrase_from_code(&status).ok_or(Error::new(
//             &format!("Invalid status code: {:?}.", status),
//             2003,
//         ))?;
//         let http_header = format!("HTTP/1.1 {} {}\n", code.0, code.1);

//         let date = Utc::now().format("%a, %d %b %Y %T %Z");
//         let date = format!("{}", date);
//         self.set_header("Date", &date);

//         let mut connection_status = "keep-alive";
//         //   if self.request_count == MAX_REQUESTS {
//         //       connection_status = "close";
//         //   }
//         self.set_header("Connection", connection_status);

//         let mut body = self.read_body().to_vec();

//         // use compression if necessary
//         if use_compression {
//             let accept_encoding = self.req.headers.get("accept-encoding");
//             if let Some(accept_encoding) = accept_encoding {
//                 let accept_encoding: Vec<&str> = accept_encoding.split(",").collect();
//                 let accept_encoding: Vec<String> = accept_encoding
//                     .iter()
//                     .map(|&i| i.trim().to_lowercase())
//                     .collect();
//                 let has_gzip = accept_encoding.contains(&String::from("gzip"));
//                 let has_deflate = accept_encoding.contains(&String::from("deflate"));

//                 let content = self.read_body();
//                 let mut content_encoding: Option<Encoding> = None;

//                 // Gzip usually increases file size in files with less than 1000 bytes
//                 if content.len() > 1000 && has_gzip {
//                     content_encoding = Some(Encoding::Gzip);
//                 } else if has_deflate {
//                     content_encoding = Some(Encoding::Deflate);
//                 };

//                 if let Some(content_encoding) = content_encoding {
//                     let (encoded_data, encoding_name) = match content_encoding {
//                         Encoding::Gzip => {
//                             let mut encoder = GzEncoder::new(Vec::new()).unwrap();
//                             std::io::copy(&mut &content[..], &mut encoder).unwrap();
//                             (encoder.finish().into_result().unwrap(), "gzip")
//                         }
//                         Encoding::Deflate => {
//                             let mut encoder = DfEncoder::new(Vec::new());
//                             std::io::copy(&mut &content[..], &mut encoder).unwrap();
//                             (encoder.finish().into_result().unwrap(), "deflate")
//                         }
//                     };

//                     self.set_header("Content-Encoding", encoding_name);
//                     body = encoded_data;
//                 }
//             }
//         }

//         self.set_header("Content-Length", &body.len().to_string());

//         let mut headers = [http_header.as_bytes(), self.get_headers().as_bytes()].concat();

//         headers.push(0x0A);

//         let mut final_data: Vec<u8>;
//         match self.req.method {
//             Method::HEAD => {
//                 final_data = headers;
//             }
//             _ => {
//                 final_data = [&headers, &body[..]].concat();
//             }
//         };
//         Ok(final_data)
//     }
// }
