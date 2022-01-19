use crate::core::codes::HTTPStatus;
use crate::core::http::{Method, Response};
use crate::core::service::CreeService;
use clap::App;
use clap::Arg;
use clap::SubCommand;
use cree::Error;
use tokio::time::Duration;

use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time;

use cree::{close_socket, CreeOptions};
use tokio::sync::mpsc;
mod core;
use crate::core::connection::HTTPConnection;
use std::fs;

mod extensions;

use libflate::{deflate::Encoder as DfEncoder, gzip::Encoder as GzEncoder};

#[tokio::main]
async fn main() {
    let matches = App::new("Cree")
        .version("0.1.0")
        .subcommand(
            SubCommand::with_name("start")
                .arg(
                    Arg::with_name("path")
                        .short('p')
                        .long("path")
                        .takes_value(true)
                        .required(false),
                )
                .arg(
                    Arg::with_name("port")
                        .long("port")
                        .takes_value(true)
                        .required(false),
                ),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("start") {
        let port: u16 = matches.value_of("port").unwrap_or("80").parse().unwrap();
        let path = matches.value_of("path");

        let addr = SocketAddr::from(([0, 0, 0, 0], port));

        let mut options = CreeOptions::get_default();
        let conf_file = fs::read(PathBuf::from("cree.toml"));
        if let Ok(f) = conf_file {
            options = match toml::from_slice::<CreeOptions>(&f)
                .or(Err(Error::new("Failed to read configuration file.", 1005)))
            {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("{}", e.msg);
                    return;
                }
            }
        } else {
            eprintln!("No cree conf file found.");
        }

        if let Some(path) = path {
            options.root_directory = Some(PathBuf::from(&path));
        }

        if let Some(path) = &options.root_directory {
            if !path.exists() {
                return eprintln!(
                    "server error: Path {} is not a valid path",
                    &path.to_str().unwrap()
                );
            }
            let listener = TcpListener::bind(addr).await.unwrap();
            let service = match CreeService::new(path, &options) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{}", e.msg);
                    return;
                }
            };
            let service = Arc::new(service);
            // let mut threads = vec![];

            println!("Listening on {}", addr);
            while let Ok((socket, _)) = listener.accept().await {
                let service = service.clone();
                //  println!("new connection");

                tokio::spawn(async move {
                    let mut tcp_connection = HTTPConnection::new(socket).unwrap();

                    while let Ok(req) = tcp_connection.listen_for_requests().await {
                        if let Method::Unknown = &req.method {
                            let mut res = Response::new(req);
                            res.set_header("Allow", "GET,HEAD,POST");
                            res.set_status(HTTPStatus::MethodNotAllowed);
                            tcp_connection.write_response(res, true).await.unwrap();
                            continue;
                        }

                        let res = service.create_response(req).await.unwrap();

                        let use_compression = if let Some(uc) = options.use_compression {
                            uc
                        } else {
                            false
                        };
                        tcp_connection
                            .write_response(res, use_compression)
                            .await
                            .unwrap();
                    }
                    tcp_connection.close().await.unwrap();
                    //   println!("connection closed");
                });
            }
            // futures::future::join_all(threads).await;
        } else {
            eprintln!("No root directory specified.");
            return;
        }
    }
}

// TODO: write headers separetly, then write content
// TODO: Add custom error page option to cree.toml.
// TODO: REDIRECT_STATUS in php should be dynamic (200, 400, 500,...)
// TODO: add logging option to CreeServer
// TODO: HTTPS
// TODO: add more options to 'cree.toml'
// TODO: change all Vec<u8> to Bytes
// TODO: add partial content streaming.
// TODO: support pipelining
