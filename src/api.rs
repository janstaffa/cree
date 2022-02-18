use std::{collections::HashMap, io, net::SocketAddr, path::PathBuf, pin::Pin, sync::Arc};

use crate::core::http::Encoding;
use chrono::Utc;
use futures::{future::join_all, Future};
use tokio::{
    io::{AsyncWriteExt, WriteHalf},
    net::{TcpListener, TcpSocket, TcpStream},
    sync::mpsc,
    sync::{mpsc::Receiver, Mutex},
    task::JoinHandle,
};

use crate::core::tcp::PersistentTcpConnection;

use crate::core::{
    http::{
        codes::{get_phrase_from_code, HTTPStatus},
        protocol::{Method, Request, Response},
    },
    tcp::TCP_MAX_MESSAGES,
};

#[derive(Clone)]
pub enum CreeOptions {
    HttpServer,
    HttpsServer {
        certificate: PathBuf,
        private_key: PathBuf,
    },
}
pub struct CreeServer {
    options: CreeOptions,
    address: SocketAddr,
    http_listener_thread: Option<JoinHandle<()>>,
    http_listener_receiver: Option<Receiver<(Request, Response)>>,
}

impl CreeServer {
    pub fn init(options: CreeOptions) -> CreeServer {
        let port: u16 = match options {
            CreeOptions::HttpServer => 80,
            CreeOptions::HttpsServer { .. } => 443,
        };
        CreeServer {
            options,
            address: SocketAddr::from(([0, 0, 0, 0], port)),
            http_listener_thread: None,
            http_listener_receiver: None,
        }
    }
    pub fn listen(&mut self, port: u16) {
        self.address.set_port(port);

        let (tx, rx) = mpsc::channel(TCP_MAX_MESSAGES as usize);
        let address = (self.address).clone();
        let options = self.options.clone();
        let listener_thread = tokio::spawn(async move {
            match options {
                CreeOptions::HttpServer => {
                    let listener = TcpListener::bind(address).await.unwrap();
                    println!("Listening on {}", address);

                    let mut threads = vec![];
                    // listen for new connections
                    while let Ok((socket, _)) = listener.accept().await {
                        let tx = tx.clone();
                        threads.push(tokio::spawn(async move {
                            let mut tcp_connection = PersistentTcpConnection::new(socket).unwrap();
                            while let Ok(message) = tcp_connection.messages().await {
                                let req =
                                    Request::new(message.content, tcp_connection.remote_addr())
                                        .unwrap();

                                let write_handle = tcp_connection.get_write_handle().clone();
                                let res = Response::__new(
                                    write_handle,
                                    req.clone(),
                                    true,
                                    tcp_connection.get_message_count() == TCP_MAX_MESSAGES,
                                );
                                tx.send((req, res)).await;
                            }
                        }));
                    }
                    futures::future::join_all(threads).await;
                }
                CreeOptions::HttpsServer { .. } => {}
            }
        });
        self.http_listener_thread = Some(listener_thread);
        self.http_listener_receiver = Some(rx);
    }

    pub async fn accept(&mut self) -> Result<(Request, Response), ()> {
        if let Some(http_listener_receiver) = &mut self.http_listener_receiver {
            return Ok(http_listener_receiver.recv().await.ok_or(())?);
        }
        Err(())
    }
}
