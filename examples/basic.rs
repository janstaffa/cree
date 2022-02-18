extern crate cree;
use cree::api::{CreeOptions, CreeServer};
use tokio;

#[tokio::main]
async fn main() {
    let mut server = CreeServer::init(CreeOptions::HttpServer);
    server.listen(81);

    while let Ok((req, mut res)) = server.accept().await {
        res.send(b"Hello from cree server!").await.unwrap();
    }
}
