use cree::Error;
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Debug)]
pub struct PHP {
   interpreter_path: PathBuf,
}

pub struct PHPOptions {
   pub php_path: Option<PathBuf>,
}

pub struct PHPVariables {
   pub request_method: String,
   pub remote_addr: String,
   pub query_string: String,
   pub document_root: String,
   pub http_host: String,
   pub request_protocol: String,
   pub request_uri: String,
}
impl PHP {
   pub fn setup(options: &PHPOptions) -> Result<PHP, Error> {
      if let Some(php_path) = &options.php_path {
         let php = PHP {
            interpreter_path: php_path.clone(),
         };
         return Ok(php);
      }
      Err(Error::new("PHP setup failed."))
   }

   pub async fn execute(&self, path: &PathBuf, variables: &PHPVariables) -> Result<Vec<u8>, Error> {
      let mut php_result = Command::new(&self.interpreter_path);
      php_result.arg("-q");
      php_result
         .env("REDIRECT_STATUS", "200")
         .env("REQUEST_METHOD", &variables.request_method)
         .env("SCRIPT_FILENAME", path)
         .env("SCRIPT_NAME", path.file_name().unwrap())
         .env("SERVER_NAME", &variables.http_host)
         .env("SERVER_PROTOCOL", &variables.request_protocol)
         .env("REQUEST_URI", &variables.request_uri)
         .env("SERVER_SOFTWARE", "Cree")
         .env("REMOTE_ADDR", &variables.remote_addr)
         .env("DOCUMENT_ROOT", &variables.document_root)
         .env("QUERY_STRING", &variables.query_string)
         .env("HTTP_HOST", &variables.http_host);
      let result = php_result
         .output()
         .or(Err(Error::new("PHP interpreter failed.")))?;
      return Ok(result.stdout);
   }
}
