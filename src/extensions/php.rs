use std::fs::{self, File};
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Debug)]
pub struct PHP {
   interpreter_path: PathBuf,
   include_file_path: PathBuf,
}

pub struct PHPOptions {
   pub php_path: Option<PathBuf>,
}

pub struct PHPVariables {
   pub remote_addr: SocketAddr,
}
impl PHP {
   pub fn setup(options: &PHPOptions) -> Result<PHP, String> {
      if let Some(php_path) = &options.php_path {
         let include_file_path = PathBuf::from("include/include.php").canonicalize().unwrap();
         let mut include_file = File::create(&include_file_path).unwrap();
         let content = format!(
            "<?php\n\
               $_SERVER['SERVER_ADDR'] = 'NOT_IMPLEMENTED';\n\
               $_SERVER['GATEWAY_INTERFACE'] = 'CGI/1.1';\n\
               $_SERVER['DOCUMENT_ROOT'] = 'www';\n\
               $_SERVER['SERVER_SOFTWARE'] = 'Cree';\n\
               $_SERVER['HTTPS'] = '';",
         );
         include_file.write(content.as_bytes()).unwrap();
         let php = PHP {
            interpreter_path: php_path.clone(),
            include_file_path,
         };
         return Ok(php);
      }
      Err(String::from("PHP setup failed."))
   }

   pub async fn execute(
      &self,
      path: &PathBuf,
      variables: &PHPVariables,
   ) -> Result<Vec<u8>, String> {
      let tmp_dir = std::env::temp_dir();

      let tmp_php_path = tmp_dir.join(uuid::Uuid::new_v4().to_string() + ".php");

      let mut tmp_php = fs::File::create(&tmp_php_path).unwrap();
      let mut php_content = fs::read(path).unwrap();
      let include_abs_path = std::env::current_dir()
         .unwrap()
         .join(&self.include_file_path);
      let include_str = format!(
         "<?php 
            include_once('{}'); \n\
            $_SERVER['PHP_SELF'] = '{}';\n\
            $_SERVER['REMOTE_ADDR'] = '{}';\n\
            $_SERVER['REQUEST_URI'] = '/test.php';\n\
            $_SERVER['HTTP_HOST'] = 'janstaffa.cz';\n\
            $_SERVER['SERVER_NAME'] = 'janstaffa.cz';\n\
            $_SERVER['SERVER_PROTOCOL'] = 'HTTP/1.0';\n\
            $_SERVER['REQUEST_METHOD'] = 'GET';\n\
         ?>",
         include_abs_path.to_str().unwrap().replace("\\\\?\\", ""),
         path.display(),
         variables.remote_addr.ip()
      );
      let include = include_str.as_bytes();
      php_content.splice(..0, include.iter().cloned());
      tmp_php.write(&php_content).unwrap();
      let php_result = Command::new(&self.interpreter_path)
         .arg("-q")
         .arg(&tmp_php_path)
         .output()
         .or(Err(String::from("PHP interpreter failed.")))?;
      return Ok(php_result.stdout);
   }
}
