use std::io;

use account::Bank;
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

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();

    if args.len() < 2 {
        print!(
            "Usage: {} <csv input file>",
            args.get(0).expect("No program filename")
        );

        std::process::exit(0);
    }

    let input_file = std::fs::File::open(&args[1])?;
    let transaction_iter = transaction::TransactionIterator::from_reader(input_file)?;

    let mut bank = Bank::default();
    for transaction in transaction_iter {
        bank.handle_transaction(transaction?);
    }

    bank.write(std::io::stdout())?;

    Ok(())
}
