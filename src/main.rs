use std::net::SocketAddr;
use std::path::PathBuf;

use clap::App;
use clap::Arg;
use clap::SubCommand;

use cree::CreeOptions;

mod core;
use crate::core::server;

mod extensions;

#[tokio::main]
async fn main() {
   let matches = App::new("Cree")
      .version("0.1.0")
      .subcommand(
         SubCommand::with_name("start")
            .arg(
               Arg::with_name("path")
                  .short("p")
                  .long("path")
                  .takes_value(true)
                  // .about("Path to the folder you want to serve")
                  .required(true),
            )
            .arg(
               Arg::with_name("port")
                  .long("port")
                  .takes_value(true)
                  // .about("Port to serve on (default: 80)")
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

      println!("Listening on {}", addr);

      let server = server::CreeServer::bind(addr, PathBuf::from(path));
      server.await.unwrap();
   }
}

// TODO: Add custom error page option to cree.toml.
// TODO: REDIRECT_STATUS in php should be dynamic (200, 400, 500,...)
// TODO: add logging option to CreeServer
// TODO: remove CreeServer struct
// TODO: HTTPS
// TODO: add more options to 'cree.toml'
// TODO: change all Vec<u8> to Bytes
// TODO: add partial content streaming.
