use futures::Future;
use hyper::{service::Service, Body, Method, Request, Response, Result as HyperResult, StatusCode};
use serde_derive::Deserialize;
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
use uuid::Uuid;

static NOTFOUND: &[u8] = b"Not Found";
static SERVERERROR: &[u8] = b"Internal server error";
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

#[derive(Debug, Deserialize, Clone)]
pub struct CreeOptions {
    pub php_path: Option<PathBuf>,
}
impl CreeOptions {
    pub fn get_default() -> CreeOptions {
        CreeOptions { php_path: None }
    }
}
pub struct CreeService {
    root_dir: PathBuf,
    options: CreeOptions,
}

impl CreeService {
    pub fn new(root_dir: PathBuf, options: CreeOptions) -> CreeService {
        let options = options;
        CreeService { root_dir, options }
    }
    fn hadle_file(
        &mut self,
        path: PathBuf,
    ) -> Pin<Box<dyn Future<Output = Result<Response<Body>, hyper::Error>> + Send>> {
        let file_name = path.file_name().unwrap().to_str().unwrap();
        let file_meta = get_file_meta(file_name);
        if let (Some(filename), Some(extension)) = file_meta {
            let mut final_path = path;
            if extension == "php" {
                let php_path = &self.options.php_path;
                let tmp_dir = std::env::temp_dir();

                if let Some(php_path) = php_path {
                    let tmp_php_path = tmp_dir.join(uuid::Uuid::new_v4().to_string() + ".php");
                    {
                        let mut tmp_php = fs::File::create(&tmp_php_path).unwrap();
                        let mut php_content = fs::read(&final_path).unwrap();
                        let include_abs_path =
                            std::env::current_dir().unwrap().join("include/include.php");
                        let include_str = format!(
                            "<?php include_once('{}'); ?>\n",
                            &include_abs_path.display()
                        );
                        let include = include_str.as_bytes();
                        php_content.splice(..0, include.iter().cloned());
                        tmp_php.write(&php_content).unwrap();
                    }
                    let php_result = Command::new(&php_path)
                        .arg("-q")
                        .arg(&tmp_php_path)
                        .output()
                        .expect("php interpreter failed");
                    let tmp_path = tmp_dir.to_path_buf().join(filename + ".html");
                    let mut tmp_file = fs::File::create(&tmp_path).unwrap();
                    tmp_file.write(&php_result.stdout).unwrap();
                    final_path = tmp_path;
                } else {
                    return Box::pin(async { Ok(server_error()) });
                }
            }
            return Box::pin(send_file(final_path));
        }
        return Box::pin(async { Ok(not_found()) });
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
                        return Box::pin(self.hadle_file(file.path()));
                    }
                }
            } else if final_path.is_file() {
                return Box::pin(self.hadle_file(final_path));
            } else {
                return Box::pin(async { Ok(not_found()) });
            }
        }
        Box::pin(async { Ok(not_found()) })
    }
}
pub struct CreeServer {
    root_dir: PathBuf,
    options: CreeOptions,
}

impl CreeServer {
    pub fn new(root_dir: PathBuf) -> CreeServer {
        let mut options = CreeOptions::get_default();
        let conf_file = fs::read(PathBuf::from("cree.toml"));
        if let Ok(f) = conf_file {
            options = toml::from_slice::<CreeOptions>(&f).unwrap();
            if let Some(php_path) = &options.php_path {
                let mut include_file = fs::File::create("include/include.php").unwrap();
                let content = "<?php\n\
                  $_SERVER['PHP_SELF'] = 'test';\n\
                  $_SERVER['REMOTE_ADDR'] = '192.168.1.2';\n\
                  $_SERVER['REQUEST_URI'] = '/test.php';\n\
                  $_SERVER['GATEWAY_INTERFACE'] = 'CGI/1.1';\n\
                  $_SERVER['SERVER_ADDR'] = '78.80.80.214';\n\
                  $_SERVER['HTTP_HOST'] = 'janstaffa.cz';\n\
                  $_SERVER['SERVER_NAME'] = 'janstaffa.cz';\n\
                  $_SERVER['SERVER_SOFTWARE'] = 'Cree';\n\
                  $_SERVER['SERVER_PROTOCOL'] = 'HTTP/1.0';\n\
                  $_SERVER['REQUEST_METHOD'] = 'GET';\n\
                  $_SERVER['DOCUMENT_ROOT'] = 'www';\n\
                  $_SERVER['HTTPS'] = '';";
                include_file.write(content.as_bytes()).unwrap();
            }
        } else {
            println!("No cree conf file found.");
        }
        CreeServer { root_dir, options }
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
        let options = self.options.clone();
        let cree_service = CreeService::new(root_dir, options);
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
fn server_error() -> Response<Body> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(NOTFOUND.into())
        .unwrap()
}

async fn send_file(path: PathBuf) -> HyperResult<Response<Body>> {
    if let Ok(file) = File::open(&path).await {
        let stream = FramedRead::new(file, BytesCodec::new());
        let body = Body::wrap_stream(stream);
        return Ok(Response::new(body));
    }
    return Ok(not_found());
}
