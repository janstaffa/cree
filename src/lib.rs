use futures::Future;
use hyper::{service::Service, Body, Method, Request, Response, Result as HyperResult, StatusCode};
use std::io::Write;
use std::{
    fs::{self},
    path::PathBuf,
    pin::Pin,
    process::Command,
    task::{Context, Poll},
};
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

static NOTFOUND: &[u8] = b"Not Found";
/// returns tuple (FILE_NAME, EXTENSION)
pub fn get_file_meta(file_name: &str) -> (Option<String>, Option<String>) {
    let split = file_name.split('.');
    let name_vec = split.collect::<Vec<&str>>();
    let len = name_vec.len();
    if len < 2 {
        return (None, None);
    }
    let file_name = name_vec[..len - 1].join("");
    let extension = name_vec[len - 1];
    (Some(file_name), Some(extension.to_owned()))
}

pub struct CreeService {
    root_dir: PathBuf,
}

impl CreeService {
    pub fn new(root_dir: PathBuf) -> CreeService {
        CreeService { root_dir }
    }
}

impl Service<Request<Body>> for CreeService {
    type Response = Response<Body>;
    type Error = hyper::Error;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        if req.method() == Method::GET {
            let raw_request_path = req.uri().path().to_owned();

            let request_path = PathBuf::from(&raw_request_path);

            let concatinated = format!("{}{}", self.root_dir.display(), request_path.display());
            let final_path = PathBuf::from(&concatinated);

            let abs_root_path = self.root_dir.canonicalize().unwrap();

            if !final_path.exists()
                || !final_path
                    .canonicalize()
                    .unwrap()
                    .starts_with(abs_root_path)
            {
                return Box::pin(async { Ok(not_found()) });
            }

            if final_path.is_dir() {
                let dir_files = fs::read_dir(&final_path).unwrap();
                for file in dir_files {
                    let file = file.unwrap();

                    if file.file_name() == "index.html" {
                        return Box::pin(hadle_file(file.path()));
                    }
                }
            } else if final_path.is_file() {
                return Box::pin(hadle_file(final_path));
            } else {
                return Box::pin(async { Ok(not_found()) });
            }
        }
        Box::pin(async { Ok(not_found()) })
    }
}
pub struct CreeServer {
    root_dir: PathBuf,
}

impl CreeServer {
    pub fn new(root_dir: PathBuf) -> CreeServer {
        CreeServer { root_dir }
    }
}
impl<T> Service<T> for CreeServer {
    type Response = CreeService;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: T) -> Self::Future {
        let root_dir = self.root_dir.clone();
        let cree_service = CreeService::new(root_dir);
        let fut = async move { Ok(cree_service) };
        Box::pin(fut)
    }
}

fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(NOTFOUND.into())
        .unwrap()
}

async fn hadle_file(path: PathBuf) -> HyperResult<Response<Body>> {
    let file_name = path.file_name().unwrap().to_str().unwrap();
    let file_meta = get_file_meta(file_name);
    if let (Some(filename), Some(extension)) = file_meta {
        let mut final_path = path;
        if extension == "php" {
            let php_result = Command::new("./src/php/x64_win/php-cgi.exe")
                .arg(&final_path)
                .output()
                .expect("ls command failed to start");
            let tmp_dir = std::env::temp_dir();
            let tmp_path = tmp_dir.to_path_buf().join(filename + ".html");
            let mut tmp_file = fs::File::create(&tmp_path).unwrap();
            tmp_file.write(&php_result.stdout).unwrap();
            final_path = tmp_path;
        }
        return send_file(final_path).await;
    }
    return Ok(not_found());
}
async fn send_file(path: PathBuf) -> HyperResult<Response<Body>> {
    if let Ok(file) = File::open(&path).await {
        let stream = FramedRead::new(file, BytesCodec::new());
        let body = Body::wrap_stream(stream);
        return Ok(Response::new(body));
    }
    return Ok(not_found());
}
