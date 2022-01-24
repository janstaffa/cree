use crate::core::codes::get_phrase_from_code;
use crate::core::codes::HTTPStatus;
use crate::HTTPConnection;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use cree::Error;
use futures::lock::Mutex;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::TcpStream;

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
    pub req: Request,
    fulfilled: bool,
    headers: Headers,
    body: Vec<u8>,
    status: HTTPStatus,
}

impl Response {
    pub fn new(req: Request) -> Response {
        Response {
            req,
            fulfilled: false,
            headers: HashMap::new(),
            body: Vec::new(),
            status: HTTPStatus::Accepted,
        }
    }

    pub fn is_fulfilled(&self) -> bool {
        self.fulfilled
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
    pub fn read_body(&self) -> &Vec<u8> {
        &self.body
    }
    pub fn write(&mut self, mut data: Vec<u8>) {
        self.body.append(&mut data);
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
