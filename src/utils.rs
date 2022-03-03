use bytes::Buf;
use std::io::{Error, ErrorKind, Read};

pub fn join_bytes(bytes: &[u8]) -> Result<u64, Error> {
    if bytes.len() > 8 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Invalid input. (max length is 8)",
        ));
    }
    let mut full_bytes = vec![0u8; 8 - bytes.len()];
    full_bytes.extend(bytes);

    let mut bytes = [0u8; 8];
    full_bytes.reader().read(&mut bytes).or(Err(Error::new(
        ErrorKind::InvalidInput,
        "Failed to read the bytes.",
    )))?;

    Ok(u64::from_be_bytes(bytes))
}
