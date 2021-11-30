use clap::App;
use clap::Arg;
use clap::SubCommand;
use cree::get_file_meta;
use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
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
        let _path = PathBuf::from(&path);
        if !_path.exists() {
            return eprintln!("server error: Path {} is not a valid path", &path);
        }

        let addr = SocketAddr::from(([127, 0, 0, 1], port));

        let make_svc = make_service_fn(move |_conn| {
            let _path = _path.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn(move |_req| file_server(_req, _path.clone())))
            }
        });

        let server = Server::bind(&addr).serve(make_svc);

        println!("Listening on http://{}", addr);

        if let Err(e) = server.await {
            eprintln!("server error: {}", e);
        }
    }
}

async fn file_server(req: Request<Body>, path: PathBuf) -> Result<Response<Body>> {
    if req.method() == Method::GET {
        let raw_request_path = req.uri().path().to_owned();

        let request_path = PathBuf::from(&raw_request_path);

        let concatinated = format!("{}{}", path.display(), request_path.display());
        let mut final_path = PathBuf::from(&concatinated);

        let abs_root_path = path.canonicalize().unwrap();

        if !final_path.exists()
            || !final_path
                .canonicalize()
                .unwrap()
                .starts_with(abs_root_path)
        {
            return Ok(not_found());
        }

        if final_path.is_dir() {
            let dir_files = fs::read_dir(&final_path).unwrap();
            for file in dir_files {
                let file = file.unwrap();

                if file.file_name() == "index.html" {
                    return hadle_file(file.path()).await;
                }
            }
        } else if final_path.is_file() {
            return hadle_file(final_path).await;
        } else {
            return Ok(not_found());
        }
    }
    Ok(not_found())
}

async fn hadle_file(path: PathBuf) -> Result<Response<Body>> {
    let file_name = path.file_name().unwrap().to_str().unwrap();
    let file_meta = get_file_meta(file_name);
    if let (Some(filename), Some(extension)) = file_meta {
        let final_path = &path;
        if extension == "php" {
            let php_result = Command::new("./src/php/x64_win/php.exe")
                .arg(&final_path)
                .output()
                .expect("ls command failed to start");
            let mut tmp_dir = std::env::temp_dir();
            let tmp_path = tmp_dir.to_path_buf().join(filename + ".html");
            let mut tmp_file = fs::File::create(&tmp_path).unwrap();
            tmp_file.write(&php_result.stdout).unwrap();
            return send_file(tmp_path).await;
        }
    }
    return Ok(not_found());
}
async fn send_file(path: PathBuf) -> Result<Response<Body>> {
    if let Ok(file) = File::open(&path).await {
        let stream = FramedRead::new(file, BytesCodec::new());
        let body = Body::wrap_stream(stream);
        return Ok(Response::new(body));
    }
    return Ok(not_found());
}
fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(NOTFOUND.into())
        .unwrap()
}

// TODO: add php support
// TODO: add .htaccess support
// TODO: HTTPS
