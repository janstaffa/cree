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
    // read cli arguments
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
        let port = matches.value_of("port");
        let path = matches.value_of("path");

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

        // check if port was passed in cli, if not check the config file or use a default(80)
        let port = if let Some(p) = port {
            p.parse().unwrap_or(80)
        } else {
            if let Some(p) = options.port {
                p
            } else {
                80
            }
        };

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

            // socket address to listen on
            let addr = SocketAddr::from(([0, 0, 0, 0], port));

            let listener = TcpListener::bind(addr).await.unwrap();

            // the service holds information about the server configuration and is cloned into every connections thread

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

            // listen for new connections
            while let Ok((socket, _)) = listener.accept().await {
                let service = service.clone();

                // spawn a new thread and move in the service
                tokio::spawn(async move {
                    // construct an HTTPConnection which wraps the connection streams and reveals higher level API
                    let mut tcp_connection = HTTPConnection::new(socket).unwrap();

                    // listen for individual requests on the connection (this makes persistent connections work)
                    // the listen_for_requests() method will halt until a request is receieved
                    while let Ok(req) = tcp_connection.listen_for_requests().await {
                        if let Method::Unknown = &req.method {
                            let mut res = Response::new(req);
                            res.set_header("Allow", "GET,HEAD,POST");
                            res.set_status(HTTPStatus::MethodNotAllowed);
                            tcp_connection.write_response(res, true).await.unwrap();
                            continue;
                        }

                        // println!("request: {}", req.uri);

                        // construct a response according to the request
                        let res = service.create_response(req).await.unwrap();

                        let use_compression = if let Some(uc) = options.use_compression {
                            uc
                        } else {
                            false
                        };

                        // send the response to the client
                        tcp_connection
                            .write_response(res, use_compression)
                            .await
                            .unwrap();
                    }

                    // the above loop will exit if an error is returned from listen_for_requests(), which means the connection has to be closed
                    tcp_connection.close().await.unwrap();
                    //   println!("connection closed: {:?}", remote_addr);
                });
            }
            // futures::future::join_all(threads).await;
        } else {
            eprintln!("No root directory specified.");
            return;
        }
    }
}

// TODO: Add custom error page option to cree.toml.
// TODO: REDIRECT_STATUS in php should be dynamic (200, 400, 500,...)
// TODO: add logging option to CreeServer
// TODO: HTTPS
// TODO: add more options to 'cree.toml'
// TODO: change all Vec<u8> to Bytes
// TODO: add partial content streaming.
// TODO: support pipelining
