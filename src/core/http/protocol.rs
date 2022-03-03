use crate::core::http::codes::get_phrase_from_code;
use crate::core::http::Encoding;
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use libflate::{deflate::Encoder as DfEncoder, gzip::Encoder as GzEncoder};
use std::collections::HashMap;
use std::io::Error;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::io::WriteHalf;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use super::codes::HTTPStatus;
extern crate proc_macro;

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
    remote_address: SocketAddr,
    time_received: DateTime<Utc>,
    pub method: Method,
    pub path: String,
    pub uri: String,
    body: String,
    pub query_string: String,
    pub query: HashMap<String, String>,
    http_info: String,
    pub headers: Headers,
    pub params: HashMap<String, String>,
}

impl Request {
    pub fn new(
        // mut connection: ReadHalf<TcpStream>,
        req_data: Vec<u8>,
        remote_address: SocketAddr,
    ) -> Result<Request, Error> {
        let req_data = String::from_utf8_lossy(&req_data);

        // parse the request
        let req = req_data.trim();

        let req = req.replace("\r", "");

        // split body and header section
        let parts: Vec<&str> = req.split("\n\n").collect();
        if parts.len() == 0 {
            return Err(Error::new(ErrorKind::InvalidInput, "Invalid request"));
        }
        let head: Vec<&str> = parts[0].lines().collect();
        if head.len() < 1 {
            return Err(Error::new(ErrorKind::InvalidInput, "Invalid request"));
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
            return Err(Error::new(ErrorKind::InvalidInput, "Invalid request"));
        }
        let method: Method = match request_line[0] {
            "GET" => Method::GET,
            "HEAD" => Method::HEAD,
            "POST" => Method::POST,
            _ => Method::Unknown,
        };

        // separate uri and path
        let uri = request_line[1];
        let query_string = if uri.contains("?") {
            let split: Vec<&str> = uri.split("?").collect();

            let mut r = "";
            if split.len() == 2 {
                r = split[1];
            }
            r.to_owned()
        } else {
            "".into()
        };
        let path = uri.replace(&format!("?{}", query_string), "");

        let query: HashMap<String, String> = if query_string.len() > 0 {
            let mut map = HashMap::new();
            let parts: Vec<&str> = query_string.split('&').collect();
            for part in parts {
                if part.len() > 0 {
                    let pair: Vec<&str> = part.split("=").collect();
                    let key = pair[0].to_string();
                    let value = if let Some(value) = pair.get(1) {
                        value.to_string()
                    } else {
                        String::new()
                    };
                    map.insert(key, value);
                }
            }
            map
        } else {
            HashMap::new()
        };
        let req = Request {
            remote_address,
            time_received: Utc::now(),
            method,
            path,
            uri: uri.to_string(),
            body: if parts.len() == 2 {
                parts[1].to_string()
            } else {
                String::from("")
            },
            query,
            query_string: query_string,
            http_info: String::from(request_line[2]),
            headers,
            params: HashMap::new(),
        };
        Ok(req)
    }

    /// The address of the connected peer.
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_address
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
    pub req: Request,
    sent: bool,
    body: Vec<u8>,
    headers: Headers,
    status: HTTPStatus,
}

impl Response {
    /// Create a new Response based on a request.
    pub fn new(req: Request) -> Response {
        Response {
            req,
            sent: false,
            headers: HashMap::new(),
            body: Vec::new(),
            status: HTTPStatus::Accepted,
        }
    }

    // Get all headers formated into a string.
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
    /// Get the current body of the request.
    pub fn get_body(&self) -> &[u8] {
        &self.body
    }
    /// Append data to the request body.
    pub fn write(&mut self, data: &[u8]) {
        self.body.extend(data);
    }
    /// Get a header by key.
    pub fn get_header(&self, key: &str) -> Option<&String> {
        self.headers.get(key)
    }

    /// Add a new header. If a header with this key already exists, the function updates its value.
    pub fn set_header(&mut self, key: &str, value: &str) {
        self.headers.insert(key.to_owned(), value.to_owned());
    }
    /// Remove a header by key.
    pub fn remove_header(&mut self, key: &str) {
        self.headers.remove(key);
    }

    /// Get the current HTTP status.
    pub fn get_status(&self) -> &HTTPStatus {
        &self.status
    }
    /// Set the HTTP status.
    /// Available statuses:
    /// - Accepted
    /// - BadRequest
    /// - Forbidden
    /// - MethodNotAllowed
    /// - NoContent
    /// - Ok
    /// - PartialContent
    /// - RangeNotSatisfiable
    /// - ServerError
    /// - Unauthorized
    pub fn set_status(&mut self, status: HTTPStatus) {
        self.status = status;
    }
}
