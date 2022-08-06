use std::io;

use thiserror::Error;

mod account;
mod transaction;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO Error: {0}")]
    IO(#[from] io::Error),
    #[error("Failed to read input data: {0}")]
    CSV(csv::Error),
    #[error("Invalid record type: {0}")]
    InvalidRecordType(String),
    #[error("Incorrect csv header: {0:?}")]
    InvalidHeader(csv::StringRecord),
}

type Result<T> = std::result::Result<T, Error>;

fn main() {
    println!("Hello, world!");
}
