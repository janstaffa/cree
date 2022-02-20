extern crate cree;

use std::pin::Pin;

use cree::api::{CreeOptions, CreeServer};
use tokio;

#[tokio::main]
async fn main() {
    // Initialize the server.
    let mut server = CreeServer::init(CreeOptions::HttpServer);

    // Attach route listeners before calling "server.listen()".
    server
        .get("/", |_, mut res| {
            Box::pin(async move {
                res.send(b"This is the home page.").await.unwrap();
            })
        })
        .unwrap();
    server
        .get("/contact", |_, mut res| {
            Box::pin(async move {
                res.send(b"This is the contact page.").await.unwrap();
            })
        })
        .unwrap();
    server
        .post("/login", |_, mut res| {
            Box::pin(async move {
                res.send(b"Login successful.").await.unwrap();
            })
        })
        .unwrap();

    // This will get run if no route pattern is matched.
    server.serve(|_, mut res| {
        Box::pin(async move {
            res.send(b"Page not found.").await.unwrap();
        })
    });
    // Start listening on port 81.
    server.listen(81).await;
}
