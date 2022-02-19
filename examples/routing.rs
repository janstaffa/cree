extern crate cree;

use cree::api::{CreeOptions, CreeServer};
use tokio;

#[tokio::main]
async fn main() {
    // Initialize the server.
    let mut server = CreeServer::init(CreeOptions::HttpServer);
    
    // Attach route listeners before calling "server.listen()".
    server.get("/", |req, mut res| {
        Box::pin(async move {
            res.send(b"This is the home page.").await.unwrap();
        })
    });
    server.get("/contact", |req, mut res| {
        Box::pin(async move {
            res.send(b"This is the contact page.").await.unwrap();
        })
    });
    server.post("/login", |req, mut res| {
        Box::pin(async move {
            res.send(b"Login successful.").await.unwrap();
        })
    });
    // Start listening on port 81.
    server.listen(81).await;

}
