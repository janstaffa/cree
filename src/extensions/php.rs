use cree::Error;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Clone, Debug)]
pub struct PHP {
   interpreter_path: PathBuf,
}

pub struct PHPOptions {
   pub php_path: Option<PathBuf>,
}

pub struct PHPVariables {
   pub request_method: String,
   pub post_data: String,
   pub content_type: String,
   pub content_length: String,
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
      let mut cmd = Command::new(&self.interpreter_path);
      cmd.env("REDIRECT_STATUS", "true")
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
         .env("CONTENT_LENGTH", &variables.content_length)
         .env("CONTENT_TYPE", &variables.content_type) //"application/x-www-form-urlencoded")
         .env("HTTP_HOST", &variables.http_host);

      if variables.request_method == "POST" {
         let process = match cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn() {
            Err(why) => return Err(Error::new(&format!("Couldn't spawn php: {}", why))),
            Ok(process) => process,
         };

         if let Err(e) = process
            .stdin
            .unwrap()
            .write_all(&variables.post_data.as_bytes())
         {
            return Err(Error::new(&format!("Couldn't write to php stdin: {}", e)));
         }

         let mut s: Vec<u8> = Vec::new();
         match process.stdout.unwrap().read_to_end(&mut s) {
            Err(e) => Err(Error::new(&format!("Couldn't read php stdout: {}", e))),
            Ok(_) => Ok(s),
         }
      } else {
         let output = match cmd.output() {
            Err(e) => return Err(Error::new(&format!("Couldn't read php stdout: {}", e))),
            Ok(r) => r,
         };

         Ok(output.stdout)
      }
   }
}
