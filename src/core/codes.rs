pub const NOT_FOUND: &str = "404 - Page not found";
pub const SERVER_ERROR: &str = "500 - Server error";

#[derive(Debug)]
pub enum HTTPStatus {
   Ok,
   Accepted,
   NotFound,
   ServerError,
   MethodNotAllowed,
   BadRequest,
   Unauthorized,
   Forbidden,
   NoContent,
   PartialContent,
}

pub fn get_phrase_from_code(status: &HTTPStatus) -> Option<(u16, String)> {
   match status {
      HTTPStatus::Ok => Some((200, String::from("OK"))),
      HTTPStatus::Accepted => Some((202, String::from("ACCEPTED"))),
      HTTPStatus::NoContent => Some((204, String::from("NO_CONTENT"))),
      HTTPStatus::PartialContent => Some((206, String::from("PARTIAL_CONTENT"))),
      HTTPStatus::BadRequest => Some((400, String::from("BAD_REQUEST"))),
      HTTPStatus::NotFound => Some((404, String::from("NOT_FOUND"))),
      HTTPStatus::MethodNotAllowed => Some((405, String::from("METHOD_NOT_ALLOWED"))),
      HTTPStatus::Unauthorized => Some((401, String::from("UNAUTHORIZED"))),
      HTTPStatus::Forbidden => Some((403, String::from("FORBIDDEN"))),
      HTTPStatus::ServerError => Some((500, String::from("SERVER_ERROR"))),
   }
}
