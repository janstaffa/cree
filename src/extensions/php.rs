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

pub struct PHPVariables<'a> {
   pub request_method: &'a str,
   pub post_data: Option<&'a str>,
   pub content_type: Option<&'a String>,
   pub content_length: Option<usize>,
   pub remote_addr: &'a str,
   pub query_string: &'a str,
   pub document_root: &'a str,
   pub http_host: &'a str,
   pub request_protocol: &'a str,
   pub request_uri: &'a str,
}
impl PHP {
   pub fn setup(options: &PHPOptions) -> Result<PHP, Error> {
      if let Some(php_path) = &options.php_path {
         let php = PHP {
            interpreter_path: php_path.clone(),
         };
         return Ok(php);
      }
      Err(Error::new("PHP setup failed.", 3000))
   }

   pub async fn execute<'a>(
      &self,
      path: &PathBuf,
      variables: &PHPVariables<'a>,
   ) -> Result<Vec<u8>, Error> {
      let mut cmd = Command::new(&self.interpreter_path);
      cmd.env("REDIRECT_STATUS", "200")
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

      if let Some(content_length) = &variables.content_length {
         cmd.env("CONTENT_LENGTH", content_length.to_string());
      }
      if let Some(content_type) = &variables.content_type {
         cmd.env("CONTENT_TYPE", content_type);
      }

      if variables.request_method == "POST" {
         if let Some(post_data) = variables.post_data {
            let process = match cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn() {
               Err(why) => return Err(Error::new(&format!("Couldn't spawn php: {}", why), 3000)),
               Ok(process) => process,
            };
            if let Err(e) = process.stdin.unwrap().write_all(&post_data.as_bytes()) {
               return Err(Error::new(
                  &format!("Couldn't write to php stdin: {}", e),
                  3000,
               ));
            }
            let mut s: Vec<u8> = Vec::new();
            match process.stdout.unwrap().read_to_end(&mut s) {
               Err(e) => Err(Error::new(
                  &format!("Couldn't read php stdout: {}", e),
                  3000,
               )),
               Ok(_) => Ok(s),
            }
         } else {
            Err(Error::new("Invalid request.", 2001))
         }
      } else {
         let output = match cmd.output() {
            Err(e) => {
               return Err(Error::new(
                  &format!("Couldn't read php stdout: {}", e),
                  3000,
               ))
            }
            Ok(r) => r,
         };

         Ok(output.stdout)
      }
   }
}
