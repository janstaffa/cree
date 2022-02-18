use crate::Error;
use chrono::{DateTime, Utc};
use tokio::sync::Mutex;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::WriteHalf;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, Receiver};
use tokio::task::JoinHandle;
use tokio::time;

pub const TCP_MAX_MESSAGES: u32 = 1024;
const CONNECTION_STALLING_LIMIT: Duration = Duration::from_secs(60);
const BUFFER_SIZE: usize = 128;

pub struct TcpMessage {
    pub time_received: DateTime<Utc>,
    pub content: Vec<u8>,
}
pub struct PersistentTcpConnection {
    remote_address: SocketAddr,
    write_handle: Arc<Mutex<WriteHalf<TcpStream>>>,
    time_established: DateTime<Utc>,
    messages_count: u32,
    listener_thread: JoinHandle<()>,
    listener_receiver: Receiver<Vec<u8>>,
}

impl PersistentTcpConnection {
    pub fn new(tcp_socket: TcpStream) -> Result<PersistentTcpConnection, Error> {
        let socket_address = tcp_socket
            .peer_addr()
            .or(Err(Error::new("Failed to obtain remote address.", 4001)))?;

        // get read and write handles separetly, read goes to request, write goes to response
        let (mut read_handle, write_handle) = tokio::io::split(tcp_socket);

        // create a channel to receieve data from a thread
        let (tx, rx) = mpsc::channel(TCP_MAX_MESSAGES as usize);

        // create a separate thread to listen for request so the connection thread is not blocked
        let listener_thread = tokio::spawn(async move {
            // this loop keep the connection running after a request is received
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
        let connection = PersistentTcpConnection {
            remote_address: socket_address,
            write_handle: Arc::new(Mutex::new(write_handle)),
            time_established: Utc::now(),
            messages_count: 0,
            listener_thread,
            listener_receiver: rx,
        };
        Ok(connection)
    }
    pub async fn messages(&mut self) -> Result<TcpMessage, Error> {
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
                        "Maximum number of messages per TCP connection was reached.",
                        4002,
                    ));
                }

                let message = TcpMessage {
                    time_received: Utc::now(),
                    content: raw_message,
                };
                return Ok(message);
            }
        } else {
            self.close().await.unwrap();
            return Err(Error::new("Connection stalling limit reached.", 4003));
        }
        Err(Error::new("Failed to read the message.", 1000))
    }

    pub async fn write(&mut self, data: &[u8]) -> Result<(), Error> {
        let mut handle = self
            .write_handle
            .try_lock()
            .or(Err(Error::new("Failed to obtain write lock.", 1000)))?;

        handle
            .write_all(data)
            .await
            .or(Err(Error::new("Failed to write to the stream.", 1003)))?;

        if let Err(_) = handle.flush().await {
            return Err(Error::new("Failed to flush the stream.", 1006));
        };
        Ok(())
    }

    pub async fn close(&mut self) -> Result<(), Error> {
        let mut handle = self
            .write_handle
            .try_lock()
            .or(Err(Error::new("Failed to obtain write lock.", 1000)))?;

        self.listener_thread.abort();
        if let Err(_) = handle.shutdown().await {
            return Err(Error::new("Failed to close the connection.", 1004));
        }
        Ok(())
    }

    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_address
    }

    pub fn get_write_handle(&self) -> &Arc<Mutex<WriteHalf<TcpStream>>> {
        &self.write_handle
    }
    pub fn get_message_count(&self) -> u32 {
        self.messages_count
    }
}
