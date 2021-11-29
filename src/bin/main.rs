use clap::App;
use clap::Arg;
use clap::SubCommand;
use std::net::SocketAddr;
use std::path::Path;
use tokio::fs::File;

use tokio_util::codec::{BytesCodec, FramedRead};

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Result, Server, StatusCode};

static NOTFOUND: &[u8] = b"Not Found";

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
      let addr = SocketAddr::from(([127, 0, 0, 1], port));

      let make_svc = make_service_fn(move |_conn| {
         let path = path.clone();
         async move { Ok::<_, hyper::Error>(service_fn(move |_req| file_server(_req, path.clone()))) }
      });

      let server = Server::bind(&addr).serve(make_svc);

      println!("Listening on http://{}", addr);

      if let Err(e) = server.await {
         eprintln!("server error: {}", e);
      }
   }
}

async fn file_server(req: Request<Body>, path: String) -> Result<Response<Body>> {
   if req.method() == Method::GET {
      let request_path = req.uri().path();

      let concatinated = format!("{}{}", &path, request_path);
      let final_path = Path::new(&concatinated);

      if let Ok(file) = File::open(final_path).await {
         let stream = FramedRead::new(file, BytesCodec::new());
         let body = Body::wrap_stream(stream);
         return Ok(Response::new(body));
      } else {
         return Ok(not_found());
      }
   }
   Ok(not_found())
}

fn not_found() -> Response<Body> {
   Response::builder()
      .status(StatusCode::NOT_FOUND)
      .body(NOTFOUND.into())
      .unwrap()
}
