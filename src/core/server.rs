use super::service::CreeService;
use cree::Error;

use crate::CreeOptions;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;

pub struct CreeServer {
   root_dir: PathBuf,
   options: CreeOptions,
}
impl CreeServer {
   pub async fn bind(addr: SocketAddr, root_dir: PathBuf) -> Result<(), Error> {
      let mut options = CreeOptions::get_default();
      let conf_file = fs::read(PathBuf::from("cree.toml"));
      if let Ok(f) = conf_file {
         options = toml::from_slice::<CreeOptions>(&f)
            .or(Err(Error::new("Failed to read configuration file.")))?;
      } else {
         println!("No cree conf file found.");
      }
      let listener = TcpListener::bind(addr).await.unwrap();

      let service = Arc::new(CreeService::new(root_dir, options)?);

      let mut threads = vec![];

      while let Ok((socket, _)) = listener.accept().await {
         let service = service.clone();
         let thread = tokio::spawn(async move {
            let _ = service.handle_request(socket).await;
         });
         threads.push(thread);
      }
      futures::future::join_all(threads).await;
      Ok(())
   }
}
