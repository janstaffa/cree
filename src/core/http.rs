use crate::core::codes::HTTPStatus;
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use cree::Error;
use futures::lock::Mutex;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[derive(Debug)]
pub enum Method {
   GET,
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
}

impl<'a> Request<'a> {
   pub async fn new(connection: &'a Mutex<Connection>) -> Result<Request<'a>, Error> {
      let mut req_data = String::new();
      let mut conn = connection.lock().await;
      if let Err(_) = conn.stream.read_line(&mut req_data).await {
         return Err(Error::new("Failed to read request."));
      };

      let ParsedRequest {
         method,
         uri,
         path,
         query,
         http_info,
      } = parse_request(&req_data)?;
      let req = Request {
         connection,
         method: method,
         path,
         uri,
         body: req_data,
         query: query,
         http_info: http_info,
      };
      Ok(req)
   }
}

#[derive(Debug)]
pub struct Response<'a> {
   pub connection: &'a Mutex<Connection>,
   pub req: Arc<Request<'a>>,
   fulfilled: bool,
   headers: HashMap<String, String>,
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

      let headers = [
         "HTTP/1.1 200 OK\n".as_bytes(),
         self.get_headers().as_bytes(),
      ]
      .concat();

      let data = [&headers, data].concat();

      if let Err(_) = writable_stream.write_all(&data).await {
         conn.close().await?;
         return Err(Error::new("Failed to write data to the stream."));
      }

      self.fulfilled = true;
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
}
fn parse_request(req: &str) -> Result<ParsedRequest, Error> {
   let split: Vec<&str> = req.split_whitespace().collect();

   if split.len() < 3 {
      return Err(Error::new("Invalid request."));
   }
   let method: Method = match split[0] {
      "GET" => Method::GET,
      "POST" => Method::POST,
      _ => {
         return Err(Error::new("Invalid request method."));
      }
   };

   let uri = split[1];
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
      http_info: String::from(split[2]),
      query: query.to_owned(),
   };
   Ok(parsed)
}
