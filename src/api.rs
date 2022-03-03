use crate::core::http::Encoding;
use crate::core::tls::protocol::{parse_tls_messages, TLSMessage, TLSSession};
use crate::core::tls::{TLSRecord, TLSVersion};
use chrono::{DateTime, Utc};
use std::hash::Hash;
use std::io::{Error, ErrorKind};
use std::time::Duration;
use std::{collections::HashMap, io, net::SocketAddr, path::PathBuf};
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc::Receiver;
use tokio::time;
use tokio::{
    io::{AsyncWriteExt, WriteHalf},
    net::TcpStream,
    sync::mpsc,
    task::JoinHandle,
};

use libflate::{deflate::Encoder as DfEncoder, gzip::Encoder as GzEncoder};

use crate::core::{
    http::{
        codes::get_phrase_from_code,
        protocol::{Method, Request, Response},
    },
    tcp::TCP_MAX_MESSAGES,
};

use std::vec;

#[derive(Clone, Debug, Eq, Hash)]
struct RoutePattern {
    pub full_route: String,
    //                    (index, key_name)
    pub replacements: Vec<(usize, String)>,
}

impl RoutePattern {
    pub fn from(original_route: &str) -> std::io::Result<RoutePattern> {
        let mut route = String::from(original_route);
        let chars: Vec<char> = route.chars().collect();
        if chars.len() == 0 {
            return Err(Error::new(ErrorKind::InvalidInput, "No route specified."));
        }
        if chars[0] != '/' {
            route.insert(0, '/');
        }

        if let Some('/') = chars.last() {
            if chars.len() > 1 {
                route.pop();
            }
        }
        let parts: Vec<&str> = route.split("/").collect();

        let mut replacements = vec![];

        for (idx, part) in parts.iter().enumerate() {
            let chars: Vec<char> = part.chars().collect();
            if chars.len() > 0 {
                if chars[0] == '{' && chars[chars.len() - 1] == '}' {
                    let key_name = String::from_iter(&chars[1..chars.len() - 1]);
                    replacements.push((idx, key_name));
                }
            }
        }

        Ok(RoutePattern {
            full_route: route,
            replacements,
        })
    }

    pub fn match_str(&self, match_str: &str) -> Option<HashMap<String, String>> {
        if match_str.len() == 0 {
            return None;
        }
        let original_parts: Vec<&str> = self.full_route.split("/").collect();
        let parts: Vec<&str> = match_str.split("/").collect();
        if original_parts.len() != parts.len() {
            return None;
        }

        let mut params: HashMap<String, String> = HashMap::new();
        for (idx, part) in parts.iter().enumerate() {
            let replacement = self.replacements.iter().find(|(i, _)| *i == idx);
            if let Some((_, key)) = replacement {
                params.insert(key.to_string(), part.to_string());
                continue;
            }

            if *part != original_parts[idx] {
                return None;
            }
        }
        Some(params)
    }
}

impl PartialEq for RoutePattern {
    fn eq(&self, other: &Self) -> bool {
        self.full_route == other.full_route
    }
}

#[derive(Clone, Copy)]
pub enum TcpApplication {
    Http,
    Https {
        certificate: &'static str,
        private_key: &'static str,
    },
}
impl TcpApplication {
    pub async fn connect(self, socket: TcpStream) -> std::io::Result<TcpConnection> {
        let socket_address = socket.peer_addr().or(Err(Error::new(
            ErrorKind::InvalidData,
            "Failed to obtain remote address.",
        )))?;

        // get read and write handles separetly, read goes to request, write goes to response
        let (mut read_handle, mut write_handle) = tokio::io::split(socket);

        // create a channel to receieve data from a thread
        let (tx, mut rx) = mpsc::channel(TCP_MAX_MESSAGES as usize);

        // create a separate thread to listen for request so the connection thread is not blocked
        let listener_thread = tokio::spawn(async move {
            // this loop keeps the connection running after a request is received
            loop {
                // beaucause a tcp stream doesnt include an end character(like EOF), we try to read a buffer of a limited length in a loop, until there is no data left
                let mut msg_data = Vec::new();
                loop {
                    let mut buffer = [0; BUFFER_SIZE];
                    match read_handle.read(&mut buffer).await {
                        Ok(len) => {
                            msg_data = [msg_data, buffer[0..len].to_vec()].concat();
                            if len < BUFFER_SIZE {
                                break;
                            }
                        }
                        Err(_) => {
                            break;
                        }
                    }
                }
                if msg_data.len() > 0 {
                    if let Err(_) = tx.send(msg_data).await {}
                }
            }
        });

        let mut tls_session = match self {
            Self::Http => None,
            Self::Https {
                certificate,
                private_key,
            } => Some(TLSSession::new(
                PathBuf::from(certificate.clone()),
                PathBuf::from(private_key.clone()),
            )),
        };

        if let TcpApplication::Https { .. } = self {
            let tls_session = tls_session.as_mut().unwrap();
            while !tls_session.handshake_finished {
                if let Ok(Some(raw_message)) =
                    time::timeout(CONNECTION_STALLING_LIMIT, rx.recv()).await
                {
                    let messages = parse_tls_messages(&raw_message).unwrap();

                    for message in messages {
                        let real_message = if tls_session.is_encrypted {
                            tls_session.decrypt_message(message).unwrap()
                        } else {
                            message
                        };
                        match real_message.record {
                            TLSRecord::Handshake | TLSRecord::ChangeCipherSpec => {
                                if !tls_session.handshake_finished {
                                    if let Some(data) =
                                        tls_session.advance_handshake(real_message).await
                                    {
                                        write_handle.write_all(&data).await.unwrap();
                                        continue;
                                    }
                                }
                            }
                            TLSRecord::Alert => {
                                let serverity = real_message.content[0];
                                // the alert IS fatal
                                if serverity == 2 {
                                    if let Err(_) = write_handle.shutdown().await {};
                                }
                            }
                            TLSRecord::Heartbeat => {}
                            _ => continue,
                        }
                    }
                }
            }
        }

        let connection = TcpConnection {
            remote_address: socket_address,
            write_handle: write_handle,
            time_established: Utc::now(),
            messages_count: 0,
            listener_thread,
            listener_receiver: rx,
            application: self,
            tls_session,
        };
        Ok(connection)
    }
}

const CONNECTION_STALLING_LIMIT: Duration = Duration::from_secs(60);
const BUFFER_SIZE: usize = 128;

pub struct TcpConnection {
    remote_address: SocketAddr,
    write_handle: WriteHalf<TcpStream>,
    time_established: DateTime<Utc>,
    messages_count: u32,
    listener_thread: JoinHandle<()>,
    listener_receiver: Receiver<Vec<u8>>,
    application: TcpApplication,
    tls_session: Option<TLSSession>,
}

impl TcpConnection {
    pub async fn messages(&mut self) -> Result<Request, Error> {
        // if a message isnt received within n seconds the connection will be closed
        if let Ok(raw_message) =
            time::timeout(CONNECTION_STALLING_LIMIT, self.listener_receiver.recv()).await
        {
            if let Some(raw_message) = raw_message {
                // keep track of the number of messages send on each connection
                self.messages_count += 1;

                if self.messages_count > TCP_MAX_MESSAGES {
                    self.close().await?;
                    return Err(Error::new(
                        ErrorKind::Other,
                        "Maximum number of messages per TCP connection was reached.",
                    ));
                }

                let req = match self.application {
                    TcpApplication::Http => {
                        Some(Request::new(raw_message, self.remote_addr()).unwrap())
                    }
                    TcpApplication::Https { .. } => {
                        let mut req = None;
                        let messages = parse_tls_messages(&raw_message).unwrap();

                        let socket_addr = self.remote_addr().clone();
                        let tls_session = self.tls_session.as_mut().unwrap();
                        for message in messages {
                            let real_message = if tls_session.is_encrypted {
                                tls_session.decrypt_message(message).unwrap()
                            } else {
                                message
                            };
                            match real_message.record {
                                TLSRecord::Application => {
                                    req = Some(
                                        Request::new(real_message.content, socket_addr).unwrap(),
                                    );
                                }
                                TLSRecord::Alert => {
                                    println!("received ALERT: {:?}", real_message.content);
                                }
                                _ => continue,
                            }
                        }
                        req
                    }
                };
                if let Some(req) = req {
                    return Ok(req);
                }
            }
        } else {
            self.close().await.unwrap();
            return Err(Error::new(
                ErrorKind::TimedOut,
                "Connection stalling limit reached.",
            ));
        }
        Err(Error::new(ErrorKind::Other, "Failed to read the message."))
    }

    pub async fn write(&mut self, data: &[u8]) -> Result<(), Error> {
        self.write_handle.write_all(data).await.or(Err(Error::new(
            ErrorKind::BrokenPipe,
            "Failed to write to the stream.",
        )))?;

        if let Err(_) = self.write_handle.flush().await {
            return Err(Error::new(
                ErrorKind::BrokenPipe,
                "Failed to flush the stream.",
            ));
        };
        Ok(())
    }

    pub async fn close(&mut self) -> Result<(), Error> {
        self.listener_thread.abort();
        if let Err(_) = self.write_handle.shutdown().await {
            return Err(Error::new(
                ErrorKind::BrokenPipe,
                "Failed to close the connection.",
            ));
        }
        Ok(())
    }

    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_address
    }

    pub fn get_message_count(&self) -> u32 {
        self.messages_count
    }

    pub async fn send(&mut self, mut res: Response) -> Result<(), Error> {
        // create all headers
        let status = res.get_status();
        let code = get_phrase_from_code(&status).ok_or(Error::new(
            ErrorKind::InvalidInput,
            format!("Invalid status code: {:?}.", status),
        ))?;
        let http_header = format!("HTTP/1.1 {} {}\n", code.0, code.1);

        let date = Utc::now().format("%a, %d %b %Y %T %Z");
        let date = format!("{}", date);
        res.set_header("Date", &date);

        let mut connection_status = "keep-alive";

        let is_last = false;
        if is_last {
            connection_status = "close";
        }
        res.set_header("Connection", connection_status);

        let mut body = res.get_body().to_vec();

        // use compression if necessary
        let use_compression = true;
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

                let content = res.get_body();
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

        let final_data = match res.req.method {
            Method::HEAD => headers,
            _ => [&headers, &body[..]].concat(),
        };

        let final_data = match self.application {
            TcpApplication::Http => final_data,
            TcpApplication::Https { .. } => {
                let tls_session = self.tls_session.as_mut().unwrap();

                let message =
                    TLSMessage::new(TLSRecord::Application, TLSVersion::TLS1_2, final_data);
                tls_session.encrypt_message(message).unwrap().get_raw()
            }
        };
        self.write(&final_data).await.unwrap();
        Ok(())
    }
}
