extern crate cree;

use std::{net::SocketAddr, path::PathBuf};

use cree::api::TcpApplication;
use cree::Response;
use tokio::{self, net::TcpListener};

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 443)))
        .await
        .unwrap();

    println!("Listening...");
    let application = TcpApplication::Https {
        certificate: "certs\\cert.crt",
        private_key: "certs\\private.key",
    };
    while let Ok((socket, _)) = listener.accept().await {
        tokio::spawn(async move {
            let mut connection = application.connect(socket).await.unwrap();

            // This will respond to all requests with the string "Hello from Cree server!".
            while let Ok(req) = connection.messages().await {
                let mut res = Response::new(req);
                res.write(b"Hello from Cree server!");
                connection.send(res).await.unwrap();
            }
        });
    }
}
