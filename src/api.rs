use crate::core::http::Encoding;
use chrono::Utc;
use futures::{future::join_all, Future};
use std::hash::Hash;
use std::io::{Error, ErrorKind};
use std::{collections::HashMap, io, net::SocketAddr, path::PathBuf, pin::Pin, sync::Arc};
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

type EndpointFuntion = fn(Request, Response) -> Pin<Box<dyn Future<Output = ()> + Send>>;
pub struct CreeServer {
    options: CreeOptions,
    address: SocketAddr,
    endpoints: HashMap<RoutePattern, (Method, EndpointFuntion)>,
    service: Option<EndpointFuntion>,
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
            endpoints: HashMap::new(),
            service: None,
        }
    }
    pub async fn listen(&mut self, port: u16) -> std::io::Result<()> {
        self.address.set_port(port);

        match self.options {
            CreeOptions::HttpServer => {
                let address = self.address;
                let listener = TcpListener::bind(address).await.unwrap();
                println!("Listening on {}", address);

                let mut threads = vec![];
                // listen for new connections
                while let Ok((socket, _)) = listener.accept().await {
                    let endpoints = self.endpoints.clone();
                    let service = self.service.clone();
                    threads.push(tokio::spawn(async move {
                        let mut tcp_connection = PersistentTcpConnection::new(socket).unwrap();
                        'message_loop: while let Ok(message) = tcp_connection.messages().await {
                            let mut req =
                                Request::new(message.content, tcp_connection.remote_addr())
                                    .unwrap();

                            let write_handle = tcp_connection.get_write_handle().clone();
                            let mut res = Response::__new(
                                write_handle,
                                req.clone(),
                                true,
                                tcp_connection.get_message_count() == TCP_MAX_MESSAGES,
                            );

                            for (pattern, (method, function)) in &endpoints {
                                if &req.method == method {
                                    let matched = pattern.match_str(&req.path);
                                    if let Some(matched) = matched {
                                        req.params = matched;
                                        function(req, res).await;
                                        continue 'message_loop;
                                    }
                                }
                            }
                            if let Some(service) = &service {
                                service(req, res).await;
                                continue;
                            }

                            res.send(b"Not found").await.unwrap();
                        }
                    }));
                }
                futures::future::join_all(threads).await;
            }
            CreeOptions::HttpsServer { .. } => {}
        }
        Ok(())
    }

    pub fn get(&mut self, route: &str, function: EndpointFuntion) -> std::io::Result<()> {
        let pattern = RoutePattern::from(route)?;
        self.endpoints.insert(pattern, (Method::GET, function));
        Ok(())
    }
    pub fn post(&mut self, route: &str, function: EndpointFuntion) -> std::io::Result<()> {
        let pattern = RoutePattern::from(route)?;
        self.endpoints.insert(pattern, (Method::POST, function));
        Ok(())
    }
    pub fn serve(&mut self, function: EndpointFuntion) {
        self.service = Some(function);
    }
}

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
