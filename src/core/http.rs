use crate::core::codes::get_phrase_from_code;
use crate::core::codes::HTTPStatus;
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct Request {
   pub connection: ReadHalf<TcpStream>,
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
   pub async fn new(
      mut connection: ReadHalf<TcpStream>,
      remote_address: SocketAddr,
   ) -> Result<Request, Error> {
      let mut req_data = Vec::new();
      const BUFFER_SIZE: usize = 128;
      loop {
         let mut buffer = [0; BUFFER_SIZE];
         match connection.read(&mut buffer).await {
            Ok(len) => {
               if len == 0 {
                  break;
               }

               req_data = [req_data, buffer[0..len].to_vec()].concat();
               if len < BUFFER_SIZE {
                  break;
               }
            }
            Err(_) => return Err(Error::new("Error reading the stream.", 1002)),
         }
      }

      let req_data = String::from_utf8(req_data.to_vec()).unwrap();

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
         connection,
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
   pub connection: WriteHalf<TcpStream>,
   pub req: Arc<Request>,
   fulfilled: bool,
   headers: Headers,
}

impl Response {
   pub fn new(connection: WriteHalf<TcpStream>, req: Arc<Request>) -> Response {
      Response {
         connection,
         req,
         fulfilled: false,
         headers: HashMap::new(),
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

   pub async fn write(&mut self, data: &[u8]) -> Result<(), Error> {
      let writable_stream = &mut self.connection;

      if let Err(_) = writable_stream.write_all(&data).await {
         self.close().await?;
         return Err(Error::new("Failed to write data to the stream.", 1003));
      }
      self.close().await?;
      Ok(())
   }

   pub async fn write_response(
      &mut self,
      data: &[u8],
      mut status_code: HTTPStatus,
      insert_newline: bool,
   ) -> Result<(), Error> {
      if self.is_fulfilled() {
         return Err(Error::new(
            "Cannot write to a response that has already been sent.",
            2000,
         ));
      }

      if let Method::Unknown = self.req.method {
         self.set_header("Allow", "GET,HEAD,POST");
         status_code = HTTPStatus::MethodNotAllowed;
      }
      let code = get_phrase_from_code(&status_code).ok_or(Error::new(
         &format!("Invalid status code: {:?}.", &status_code),
         2003,
      ))?;
      let http_header = format!("HTTP/1.1 {} {}\n", code.0, code.1);

      let date = Utc::now().format("%a, %d %b %Y %T %Z");
      let date = format!("{}", date);
      self.set_header("Date", &date);

      let mut headers = [http_header.as_bytes(), self.get_headers().as_bytes()].concat();
      if insert_newline {
         headers.push(0x0A); // newline
      }

      let mut final_data: Vec<u8> = vec![];
      match self.req.method {
         Method::HEAD | Method::Unknown => {
            final_data = headers;
         }
         _ => {
            final_data = [&headers, data].concat();
         }
      };

      self.write(&mut final_data).await?;
      Ok(())
   }
   /// closes the connection
   pub async fn close(&mut self) -> Result<(), Error> {
      if let Err(_) = self.connection.shutdown().await {
         return Err(Error::new("Failed to close the connection.", 1004));
      }
      self.fulfilled = true;
      Ok(())
   }
}

pub async fn construct_http_interface(
   stream: TcpStream,
) -> Result<(Arc<Request>, Response), Error> {
   let remote_address = stream.peer_addr().unwrap();
   let (readable, mut writeable) = tokio::io::split(stream);

   let req = match Request::new(readable, remote_address).await {
      Ok(r) => r,
      Err(e) => {
         if let Err(_) = writeable.shutdown().await {
            return Err(Error::new("Failed to close the connection.", 1004));
         }
         return Err(e);
      }
   };

   let req = Arc::new(req);
   let res = Response::new(writeable, req.clone());

   Ok((req, res))
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
fn parse_request(req: &str) -> Result<ParsedRequest, Error> {
   let req = req.trim();
   let req = req.replace("\r", "");

   let parts: Vec<&str> = req.split("\n\n").collect();
   if parts.len() == 0 {
      return Err(Error::new("Invalid request", 2001));
   }
   let head: Vec<&str> = parts[0].lines().collect();
   if head.len() < 1 {
      return Err(Error::new("Invalid request", 2001));
   }
   let request_line: Vec<&str> = head[0].split_whitespace().collect();

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
