use futures::lock::Mutex;
use std::net::SocketAddr;
use std::path::PathBuf;
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
}

impl Connection {
   pub fn new(stream: TcpStream) -> Connection {
      let remote_address = stream.peer_addr().unwrap();
      let reader = BufReader::new(stream);
      Connection {
         stream: reader,
         remote_address,
         is_alive: true,
      }
   }
   pub fn is_alive(&self) -> bool {
      self.is_alive
   }
   pub async fn close(&mut self) -> Result<(), String> {
      if let Err(_) = self.stream.get_mut().shutdown().await {
         return Err(String::from("Failed to close the connection."));
      }
      self.is_alive = false;
      Ok(())
   }
}
#[derive(Debug)]
pub struct Request<'a> {
   pub connection: &'a Mutex<Connection>,
   pub method: Method,
   pub path: PathBuf,
   pub body: String,
}

impl<'a> Request<'a> {
   pub async fn new(connection: &'a Mutex<Connection>) -> Result<Request<'a>, String> {
      let mut req_data = String::new();
      let mut conn = connection.lock().await;
      if let Err(_) = conn.stream.read_line(&mut req_data).await {
         return Err(String::from("Failed to read request."));
      };

      let parsed = parse_request(&req_data)?;
      let req = Request {
         connection,
         method: parsed.method,
         path: parsed.path,
         body: req_data,
      };
      Ok(req)
   }
}

#[derive(Debug)]
pub struct Response<'a> {
   pub connection: &'a Mutex<Connection>,
   fulfilled: bool,
}

impl<'a> Response<'a> {
   pub fn new(connection: &'a Mutex<Connection>) -> Response<'a> {
      Response {
         connection,
         fulfilled: false,
      }
   }

   pub fn is_fulfilled(&self) -> bool {
      self.fulfilled
   }
}

pub async fn construct_http_interface<'a>(
   connection: &'a Mutex<Connection>,
) -> (Request<'a>, Response<'a>) {
   let req = Request::new(connection).await.unwrap();
   let res = Response::new(connection);

   (req, res)
}

#[derive(Debug)]
struct ParsedRequest {
   method: Method,
   path: PathBuf,
   http_info: String,
}
fn parse_request(req: &str) -> Result<ParsedRequest, String> {
   let split: Vec<&str> = req.split_whitespace().collect();

   if split.len() < 3 {
      return Err(String::from("Invalid request."));
   }
   let method: Method = match split[0] {
      "GET" => Method::GET,
      "POST" => Method::POST,
      _ => {
         return Err(String::from("Invalid request method."));
      }
   };

   let parsed = ParsedRequest {
      method,
      path: PathBuf::from(split[1]),
      http_info: String::from(split[2]),
   };
   Ok(parsed)
}
