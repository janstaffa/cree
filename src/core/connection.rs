use super::http::Method;
use crate::core::codes::get_phrase_from_code;
use crate::core::http::Request;
use crate::Response;
use chrono::{DateTime, Utc};
use cree::Encoding;
use cree::Error;
use libflate::deflate::EncodeOptions;
use libflate::{deflate::Encoder as DfEncoder, gzip::Encoder as GzEncoder};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::WriteHalf;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, Receiver};
use tokio::task::JoinHandle;
use tokio::time;

const MAX_REQUESTS: usize = 1024;
const CONNECTION_STALLING_LIMIT: Duration = Duration::from_secs(60);

pub struct HTTPConnection {
    remote_address: SocketAddr,
    write_handle: WriteHalf<TcpStream>,
    time_established: DateTime<Utc>,
    request_count: usize,
    listener_thread: JoinHandle<()>,
    listener_receiver: Receiver<Vec<u8>>,
}

impl HTTPConnection {
    pub fn new(tcp_socket: TcpStream) -> Result<HTTPConnection, Error> {
        let socket_address = tcp_socket
            .peer_addr()
            .or(Err(Error::new("Failed to obtain remote address.", 4001)))?;

        // get read and write handles separetly, read goes to request, write goes to response
        let (mut read_handle, write_handle) = tokio::io::split(tcp_socket);

        // create a channel to receieve data from a thread
        let (tx, rx) = mpsc::channel(MAX_REQUESTS);

        // create a separate thread to listen for request so the connection thread is not blocked
        let listener_thread = tokio::spawn(async move {
            // this loop keep the connection running after a request is receieved
            loop {
                // beaucause a tcp stream doesnt include an end character(like EOF), we try to read a buffer of 128 in a loop, until there is no data left
                const BUFFER_SIZE: usize = 128;
                let mut req_data = Vec::new();
                loop {
                    let mut buffer = [0; BUFFER_SIZE];
                    match read_handle.read(&mut buffer).await {
                        Ok(len) => {
                            req_data = [req_data, buffer[0..len].to_vec()].concat();
                            if len < BUFFER_SIZE {
                                break;
                            }
                        }
                        Err(_) => {
                            break;
                        }
                    }
                }
                if req_data.len() > 0 {
                    if let Err(_) = tx.send(req_data).await {}
                }
            }
        });
        let connection = HTTPConnection {
            remote_address: socket_address,
            write_handle,
            time_established: Utc::now(),
            request_count: 0,
            listener_thread,
            listener_receiver: rx,
        };
        Ok(connection)
    }
    pub async fn listen_for_requests(&mut self) -> Result<Request, Error> {
        // if a request isnt receieved within n seconds the connection will be closed
        if let Ok(raw_request) =
            time::timeout(CONNECTION_STALLING_LIMIT, self.listener_receiver.recv()).await
        {
            if let Some(raw_request) = raw_request {
                if self.request_count + 1 == MAX_REQUESTS {
                    return Err(Error::new(
                        "Maximum number of requests per connection was reached.",
                        2004,
                    ));
                }

                let req = Request::new(raw_request, self.remote_address)?;

                // keep track of the number of requests send on each connection
                self.request_count += 1;

                return Ok(req);
            }
        } else {
            self.close().await;
            return Err(Error::new("Connection stalling limit reached.", 2004));
        }
        Err(Error::new("Failed to read the request.", 1000))
    }

    pub async fn write(&mut self, data: &[u8]) -> Result<(), Error> {
        self.write_handle
            .write_all(data)
            .await
            .or(Err(Error::new("Failed to write to the stream.", 1003)))?;

        if let Err(_) = self.write_handle.flush().await {
            return Err(Error::new("Failed to flush the stream.", 1006));
        };
        Ok(())
    }

    pub async fn close(&mut self) -> Result<(), Error> {
        self.listener_thread.abort();
        if let Err(_) = self.write_handle.shutdown().await {
            return Err(Error::new("Failed to close the connection.", 1004));
        }
        Ok(())
    }

    // this method is responsible for creating the actual response bytes from a Response instance and sending them to the client
    pub async fn write_response(
        &mut self,
        mut res: Response,
        use_compression: bool,
    ) -> Result<(), Error> {
        if res.is_fulfilled() {
            return Err(Error::new(
                "Cannot write to a response that has already been sent.",
                2000,
            ));
        }

        // create all headers
        let status = res.get_status();
        let code = get_phrase_from_code(&status).ok_or(Error::new(
            &format!("Invalid status code: {:?}.", status),
            2003,
        ))?;
        let http_header = format!("HTTP/1.1 {} {}\n", code.0, code.1);

        let date = Utc::now().format("%a, %d %b %Y %T %Z");
        let date = format!("{}", date);
        res.set_header("Date", &date);

        let mut connection_status = "keep-alive";
        if self.request_count == MAX_REQUESTS {
            connection_status = "close";
        }
        res.set_header("Connection", connection_status);

        let mut body = res.read_body().to_vec();

        // use compression if necessary
        if use_compression {
            let accept_encoding = res.req.headers.get("accept-encoding");
            if let Some(accept_encoding) = accept_encoding {
                let accept_encoding: Vec<&str> = accept_encoding.split(",").collect();
                let accept_encoding: Vec<String> = accept_encoding
                    .iter()
                    .map(|&i| i.trim().to_lowercase())
                    .collect();
                let has_gzip = accept_encoding.contains(&String::from("gzip"));
                let has_deflate = accept_encoding.contains(&String::from("deflate"));

                let content = res.read_body();
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

                    res.set_header("Content-Encoding", encoding_name);
                    body = encoded_data;
                }
            }
        }

        res.set_header("Content-Length", &body.len().to_string());

        let mut headers = [http_header.as_bytes(), res.get_headers().as_bytes()].concat();

        headers.push(0x0A);

        let mut final_data: Vec<u8>;
        match res.req.method {
            Method::HEAD => {
                final_data = headers;
            }
            _ => {
                final_data = [&headers, &body[..]].concat();
            }
        };

        self.write(&mut final_data).await?;
        Ok(())
    }
}
