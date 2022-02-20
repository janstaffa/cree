extern crate cree;

use cree::api::{CreeOptions, CreeServer};
use tokio;

#[tokio::main]
async fn main() {
    // Initialize the server.
    let mut server = CreeServer::init(CreeOptions::HttpServer);

    // This will match any request to /users/.. (ex.: /users/john) and pass the url param to the request struct.
    server
        .get("/users/{user_id}", |req, mut res| {
            Box::pin(async move {
                res.send(
                    format!(
                        "You have requested the page of user: {}",
                        req.params.get("user_id").unwrap()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            })
        })
        .unwrap();

    // You can include multiple parameters in a route pattern.
    server
        .get("/games/{year}/{month}", |req, mut res| {
            Box::pin(async move {
                res.send(
                    format!(
                        "You have requested the list of games in the year: {} and month: {}",
                        req.params.get("year").unwrap(),
                        req.params.get("month").unwrap()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            })
        })
        .unwrap();

    // Start listening on port 81.
    server.listen(81).await;
}
