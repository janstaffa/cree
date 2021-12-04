use std::net::SocketAddr;
use std::path::PathBuf;

use clap::App;
use clap::Arg;
use clap::SubCommand;

use cree::CreeOptions;
use cree::CreeServer;
use hyper::Server;
use std::fs;
use toml::Value;

#[tokio::main]
async fn main() {
    let matches = App::new("Cree")
        .version("0.1.0")
        .subcommand(
            SubCommand::with_name("start")
                .arg(
                    Arg::new("path")
                        .short('p')
                        .long("path")
                        .takes_value(true)
                        .about("Path to the folder you want to serve")
                        .required(true),
                )
                .arg(
                    Arg::new("port")
                        .long("port")
                        .takes_value(true)
                        .about("Port to serve on (default: 80)")
                        .required(false),
                ),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("start") {
        let port: u16 = matches.value_of("port").unwrap_or("80").parse().unwrap();
        let path = matches.value_of("path").unwrap().to_owned();
        let _path = PathBuf::from(&path);
        if !_path.exists() {
            return eprintln!("server error: Path {} is not a valid path", &path);
        }

        let addr = SocketAddr::from(([0, 0, 0, 0], port));

        let cree_server = CreeServer::new(PathBuf::from(path));

        let server = Server::bind(&addr).serve(cree_server);

        println!("Listening on http://{}", addr);

        if let Err(e) = server.await {
            eprintln!("server error: {}", e);
        }
    }
}

// TODO: properly include env variables in php
// TODO: add .htaccess support
// TODO: HTTPS
