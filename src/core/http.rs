use crate::core::codes::get_code_from_status;
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
use tokio::net::TcpStream;

#[derive(Debug)]
pub enum Method {
   GET,
   HEAD,
   POST,
}

#[derive(Debug)]
pub struct Connection {
   pub stream: BufReader<TcpStream>,
   pub remote_address: SocketAddr,
   is_alive: bool,
   time_established: DateTime<Utc>,
}

impl Connection {
   pub fn new(stream: TcpStream) -> Connection {
      let remote_address = stream.peer_addr().unwrap();
      let reader = BufReader::new(stream);
      Connection {
         stream: reader,
         remote_address,
         is_alive: true,
         time_established: Utc::now(),
      }
   }
   pub fn is_alive(&self) -> bool {
      self.is_alive
   }

   /// DateTime of when the connection was established
   pub fn time_established(&self) -> DateTime<Utc> {
      self.time_established
   }

   /// Duration of the connection as of calling this function
   pub fn duration(&self) -> Duration {
      Utc::now() - self.time_established
   }

   /// closes the connection
   pub async fn close(&mut self) -> Result<(), Error> {
      if let Err(_) = self.stream.get_mut().shutdown().await {
         return Err(Error::new("Failed to close the connection."));
      }
      self.is_alive = false;
      Ok(())
   }

   pub async fn read_all(&mut self, buf: &mut Vec<u8>) -> Result<(), Error> {
      const BUFFER_SIZE: usize = 128;
      loop {
         let mut buffer = [0; BUFFER_SIZE];
         match self.stream.read(&mut buffer).await {
            Ok(len) => {
               if len == 0 {
                  break;
               }

               *buf = [buf.to_owned(), buffer.to_vec()].concat();
               if len < BUFFER_SIZE {
                  break;
               }
            }
            Err(_) => return Err(Error::new("Error reading the stream.")),
         }
      }
      Ok(())
   }
}
#[derive(Debug)]
pub struct Request<'a> {
   pub connection: &'a Mutex<Connection>,
   pub method: Method,
   pub path: String,
   pub uri: String,
   pub body: String,
   pub query: String,
   pub http_info: String,
   pub headers: Headers,
}

impl<'a> Request<'a> {
   pub async fn new(connection: &'a Mutex<Connection>) -> Result<Request<'a>, Error> {
      let mut conn = connection
         .try_lock()
         .ok_or(Error::new("Couldn't aquire a lock over response stream."))?;
      let mut req_data: Vec<u8> = vec![];
      conn.read_all(&mut req_data).await?;

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
         method: method,
         path,
         uri,
         body,
         query: query,
         http_info: http_info,
         headers,
      };
      Ok(req)
   }
}

type Headers = HashMap<String, String>;
#[derive(Debug)]
pub struct Response<'a> {
   pub connection: &'a Mutex<Connection>,
   pub req: Arc<Request<'a>>,
   fulfilled: bool,
   headers: Headers,
}

impl<'a> Response<'a> {
   pub fn new(connection: &'a Mutex<Connection>, req: Arc<Request<'a>>) -> Response<'a> {
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

   pub async fn write(&mut self, data: &[u8], status_code: HTTPStatus) -> Result<(), Error> {
      if self.is_fulfilled() {
         return Err(Error::new(
            "Cannot write to a response that has already been sent.",
         ));
      }
      let mut conn = self
         .connection
         .try_lock()
         .ok_or(Error::new("Couldn't aquire a lock over response stream."))?;
      let writable_stream = &mut conn.stream;

      let code = get_code_from_status(&status_code).ok_or(Error::new(&format!(
         "Invalid status code: {:?}.",
         &status_code
      )))?;
      let http_header = format!("HTTP/1.1 {} {}\n", code.0, code.1);

      let date = Utc::now().format("%a, %d %b %Y %T %Z");
      let date = format!("{}", date);
      self.set_header("Date", &date);

      let headers = [http_header.as_bytes(), self.get_headers().as_bytes(), b"\n"].concat();

      let mut final_data: Vec<u8> = vec![];
      match self.req.method {
         Method::HEAD => {
            final_data = headers;
         }
         _ => {
            final_data = [&headers, data].concat();
         }
      }

      if let Err(_) = writable_stream.write_all(&final_data).await {
         conn.close().await?;
         return Err(Error::new("Failed to write data to the stream."));
      }
      self.fulfilled = true;
      conn.close().await?;
      Ok(())
   }
}

pub async fn construct_http_interface<'a>(
   connection: &'a Mutex<Connection>,
) -> (Arc<Request<'a>>, Response<'a>) {
   let req = Arc::new(Request::new(connection).await.unwrap());
   let res = Response::new(connection, req.clone());

   (req, res)
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
      return Err(Error::new("Invalid request"));
   }
   let head: Vec<&str> = parts[0].lines().collect();
   if head.len() < 1 {
      return Err(Error::new("Invalid request"));
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

         headers.insert(parts[0].trim().to_owned(), parts[1].trim().to_owned());
      }
   }

   if request_line.len() < 3 {
      return Err(Error::new("Invalid request."));
   }
   let method: Method = match request_line[0] {
      "GET" => Method::GET,
      "HEAD" => Method::HEAD,
      "POST" => Method::POST,
      _ => {
         return Err(Error::new("Invalid request method."));
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
