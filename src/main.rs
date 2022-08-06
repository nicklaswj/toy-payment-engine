use std::io;

use csv::StringRecord;
use rust_decimal::Decimal;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
enum Error {
    #[error("IO Error: {0}")]
    IO(#[from] io::Error),
    #[error("Failed to read input data: {0}")]
    CSV(csv::Error),
    #[error("Incorrect csv header: {0:?}")]
    IncorrectHeader(StringRecord),
}

// We implement From csv::Error to Error manually so if the underlying error is an IO error we will
// unpack the IO error and place it in the Error::IO variant in our Error enum.
impl From<csv::Error> for Error {
    fn from(csv_error: csv::Error) -> Self {
        if csv_error.is_io_error() {
            match csv_error.into_kind() {
                // Unpack the internal IO error and convert it to an Error::IO
                csv::ErrorKind::Io(io_error) => io_error.into(),
                // This should never happens since we have already checked if the error is an IO
                // error, see https://docs.rs/csv/latest/csv/struct.Error.html#method.is_io_error
                _ => unreachable!(),
            }
        } else {
            // It was not an IO error, return the csv error in hte Error::CSV variant
            Self::CSV(csv_error)
        }
    }
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum TransactionType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Transaction {
    transaction_type: TransactionType,
    client: u16,
    tx: u32,
    amount: Option<Decimal>,
}

struct TransactionIterator<R: io::Read> {
    // Inner csv reader
    reader: csv::Reader<R>,
    // scratch record to avoid allocation of SringRecord per call to <Self as Iterator>::next
    scratch_record: csv::StringRecord,
}

impl<R: io::Read> Iterator for TransactionIterator<R> {
    type Item = Result<Transaction>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.reader.read_record(&mut self.scratch_record) {
            // Successful read of record. Map error to correct error type and wrap the result in
            // Some
            Ok(true) => {
                // Trim all record fields for whitespaces
                self.scratch_record.trim();
                Some(
                    self.scratch_record
                        .deserialize::<'_, Transaction>(None)
                        .map_err(Error::from)
                        .map(|record| {
                            // Per the documentation the amount field has a max precision of four decimals. So
                            // if we get an input amount with higher precision than four we will assume it's an
                            // error and round it down, i.e. basically cut off the rest of the decimals.
                            record.amount.map(|amount| {
                                amount.round_dp_with_strategy(
                                    4,
                                    rust_decimal::RoundingStrategy::ToZero,
                                )
                            });

                            record
                        }),
                )
            }
            // Reached EOF
            Ok(false) => None,
            // Error
            Err(e) => Some(Err(e.into())),
        }
    }
}

impl<R: io::Read> TransactionIterator<R> {
    const EXPECTED_HEADER: &'static [&'static str] = &["type", "client", "tx", "amount"];
    /// Takes a reader R for csv formatted data and returns a TransactionIterator.
    ///
    /// Returns an error if the csv header is not parsed to:
    /// type, client, tx, amount
    pub fn from_reader(reader: R) -> Result<Self> {
        let mut reader = csv::Reader::from_reader(reader);

        let mut headers = reader.headers()?.to_owned();
        headers.trim();
        if &headers == Self::EXPECTED_HEADER {
            Ok(Self {
                reader,
                scratch_record: Default::default(),
            })
        } else {
            Err(Error::IncorrectHeader(headers.to_owned()))
        }
    }
}

fn main() {
    println!("Hello, world!");
}

#[cfg(test)]
mod test {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn successfully_serialize_test() {
        let input = r#"
            type, client, tx, amount
            deposit, 1, 1, 1.0
            deposit, 2, 2, 2.0
            deposit, 1, 3, 2.0
            withdrawal, 1, 4, 1.5
            withdrawal, 2, 5, 3.0
        "#;

        let expected_result = [
            Transaction {
                transaction_type: TransactionType::Deposit,
                client: 1,
                tx: 1,
                amount: Some(dec!(1.0)),
            },
            Transaction {
                transaction_type: TransactionType::Deposit,
                client: 2,
                tx: 2,
                amount: Some(dec!(2.0)),
            },
            Transaction {
                transaction_type: TransactionType::Deposit,
                client: 1,
                tx: 3,
                amount: Some(dec!(2.0)),
            },
            Transaction {
                transaction_type: TransactionType::Withdrawal,
                client: 1,
                tx: 4,
                amount: Some(dec!(1.5)),
            },
            Transaction {
                transaction_type: TransactionType::Withdrawal,
                client: 2,
                tx: 5,
                amount: Some(dec!(3.0)),
            },
        ];

        // Create transaction reader
        let t_reader = TransactionIterator::from_reader(std::io::Cursor::new(input)).unwrap();

        // Compare the each read record with the expected record
        for (result, expected) in t_reader.zip(expected_result.into_iter()) {
            assert_eq!(result.unwrap(), expected)
        }
    }

    #[test]
    fn errornous_header_test() {
        let input = r#"
            type, tx, client, amount
            deposit, 1, 1, 1.0
        "#;

        let reader_result = TransactionIterator::from_reader(std::io::Cursor::new(input));

        assert!(matches!(reader_result, Err(Error::IncorrectHeader(_))))
    }
}
