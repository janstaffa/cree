extern crate cree;

use cree::api::{CreeOptions, CreeServer};
use tokio;

#[tokio::main]
async fn main() {
    // Initialize the server.
    let mut server = CreeServer::init(CreeOptions::HttpServer);

    // This will respond to all requests with the string "Hello from cree server!".
    server.serve(|req, mut res| {
        Box::pin(async move {
            res.send(b"Hello from Cree server!").await.unwrap();
        })
    });

    // Start listening on port 81.
    server.listen(81).await;
}
