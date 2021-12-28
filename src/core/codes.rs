pub const NOT_FOUND: &str = "404 - Page not found";
pub const SERVER_ERROR: &str = "500 - Server error";

pub enum HTTPStatus {
   Ok,
   NotFound,
   ServerError,
}
