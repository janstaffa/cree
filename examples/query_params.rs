extern crate cree;

use cree::api::{CreeOptions, CreeServer};
use tokio;

#[tokio::main]
async fn main() {
    // Initialize the server.
    let mut server = CreeServer::init(CreeOptions::HttpServer);


    // This will match all requests to "/search" and give different output depending on query parameters.
    server
    .get("/search", |req, mut res| {
           let topics = ["Health", "Education", "Money", "Jobs", "Nature", "World", "Science", "Technology", "People", "Rust"];
            Box::pin(async move {
                if let Some(question) = req.query.get("q") {
                  let results: Vec<&str> = topics.iter().filter(|t| t.to_lowercase().contains(&question.to_lowercase())).cloned().collect();
                     let mut result_list = String::new();
                     for result in &results {
                        result_list.push_str(result);
                        result_list.push('\n');
                     }
                     res.send(
                        format!(
                            "Found {} results: \n{}",
                            results.len(),
                            result_list
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
                    return;
                };
                res.send(b"Invalid input").await;
            })
        })
        .unwrap();

    // Start listening on port 81.
    server.listen(81).await;
}
