use bytes::Buf;
use libflate::{deflate::Encoder as DfEncoder, gzip::Encoder as GzEncoder};
use serde_derive::Deserialize;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::io::Read;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

pub mod api;
mod core;
mod utils;
