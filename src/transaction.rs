use std::io;

use super::{Error, Result};
use rust_decimal::Decimal;
use serde::Deserialize;

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

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct TransactionData {
    pub client: u16,
    pub tx: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct TransactionAmountData {
    pub client: u16,
    pub tx: u32,
    pub amount: Decimal,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Transaction {
    Deposit(TransactionAmountData),
    Withdrawal(TransactionAmountData),
    Dispute(TransactionData),
    Resolve(TransactionData),
    Chargeback(TransactionData),
}

impl Transaction {
    fn from_record(record: &csv::StringRecord, header: &csv::StringRecord) -> Result<Self> {
        // Get header type
        let record_type = record
            .get(0)
            .ok_or_else(|| Error::InvalidRecordType("".to_owned()))?;

        let mut transaction = match record_type {
            "deposit" => Transaction::Deposit(record.deserialize(Some(header))?),
            "withdrawal" => Transaction::Withdrawal(record.deserialize(Some(header))?),
            "dispute" => Transaction::Dispute(record.deserialize(Some(header))?),
            "resolve" => Transaction::Resolve(record.deserialize(Some(header))?),
            "chargeback" => Transaction::Chargeback(record.deserialize(Some(header))?),
            other => return Err(Error::InvalidRecordType(other.to_owned())),
        };

        // Cap amount precision at 4 decimals
        if let Some(amount) = transaction.amount_mut() {
            *amount = amount.round_dp_with_strategy(4, rust_decimal::RoundingStrategy::ToZero);
        }

        Ok(transaction)
    }
    /// Will return a mutable reference to the amount field in the transaction if the transaction is either a deposit or a
    /// withdrawal, will return None otherwise.
    pub fn amount_mut(&mut self) -> Option<&mut Decimal> {
        match self {
            Self::Deposit(data) | Self::Withdrawal(data) => Some(&mut data.amount),
            _ => None,
        }
    }

    /// Returns the client id associated with the transaction
    pub fn client_id(&self) -> u16 {
        match self {
            Transaction::Deposit(data) | Transaction::Withdrawal(data) => data.client,
            Transaction::Dispute(data)
            | Transaction::Resolve(data)
            | Transaction::Chargeback(data) => data.client,
        }
    }
}

pub struct TransactionIterator<R: io::Read> {
    // Inner csv reader
    reader: csv::Reader<R>,
    csv_header: csv::StringRecord,
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

                Some(Transaction::from_record(
                    &self.scratch_record,
                    &self.csv_header,
                ))
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

        let mut csv_header = reader.headers()?.to_owned();
        csv_header.trim();
        if &csv_header == Self::EXPECTED_HEADER {
            Ok(Self {
                reader,
                csv_header,
                scratch_record: Default::default(),
            })
        } else {
            Err(Error::InvalidHeader(csv_header.to_owned()))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rust_decimal_macros::dec;

    /// Test deserialization of all tranaction types
    #[test]
    fn successfully_serialize_test() {
        let input = r#"
            type, client, tx, amount
            deposit, 1, 1, 1.0
            deposit, 2, 2, 2.0
            deposit, 1, 3, 2.0
            withdrawal, 1, 4, 1.5
            withdrawal, 2, 5, 3.0
            dispute, 1, 1,
            resolve, 2, 2,
            chargeback, 3, 3,
        "#;

        let expected_result = [
            Transaction::Deposit(TransactionAmountData {
                client: 1,
                tx: 1,
                amount: dec!(1.0),
            }),
            Transaction::Deposit(TransactionAmountData {
                client: 2,
                tx: 2,
                amount: dec!(2.0),
            }),
            Transaction::Deposit(TransactionAmountData {
                client: 1,
                tx: 3,
                amount: dec!(2.0),
            }),
            Transaction::Withdrawal(TransactionAmountData {
                client: 1,
                tx: 4,
                amount: dec!(1.5),
            }),
            Transaction::Withdrawal(TransactionAmountData {
                client: 2,
                tx: 5,
                amount: dec!(3.0),
            }),
            Transaction::Dispute(TransactionData { client: 1, tx: 1 }),
            Transaction::Resolve(TransactionData { client: 2, tx: 2 }),
            Transaction::Chargeback(TransactionData { client: 3, tx: 3 }),
        ];

        // Create transaction reader
        let t_reader = TransactionIterator::from_reader(std::io::Cursor::new(input)).unwrap();

        // Compare the each read record with the expected record
        for (result, expected) in t_reader.zip(expected_result.into_iter()) {
            assert_eq!(result.unwrap(), expected)
        }
    }

    /// Test handling of bad header
    #[test]
    fn errornous_header_test() {
        let input = r#"
            type, tx, client, amount
            deposit, 1, 1, 1.0,
        "#;

        let reader_result = TransactionIterator::from_reader(std::io::Cursor::new(input));

        assert!(matches!(reader_result, Err(Error::InvalidHeader(_))))
    }

    /// Test handling of input with overly precise amount input
    #[test]
    fn too_high_precision_test() {
        let input = r#"
            type, client, tx, amount
            deposit, 1, 1, 1.12345
        "#;

        let mut transaction_iter =
            TransactionIterator::from_reader(std::io::Cursor::new(input)).unwrap();

        match transaction_iter.next() {
            Some(Ok(Transaction::Deposit(TransactionAmountData { amount, .. }))) => {
                assert_eq!(amount, dec!(1.1234))
            }
            other => panic!("Unexpected transaction type: {:#?}", other),
        }
    }

    /// Test handling of input with low precision amount input
    #[test]
    fn low_precision_test() {
        let input = r#"
            type, client, tx, amount
            deposit, 1, 1, 1.1
        "#;

        let mut transaction_iter =
            TransactionIterator::from_reader(std::io::Cursor::new(input)).unwrap();

        match transaction_iter.next() {
            Some(Ok(Transaction::Deposit(TransactionAmountData { amount, .. }))) => {
                assert_eq!(amount, dec!(1.1000))
            }
            other => panic!("Unexpected transaction type: {:#?}", other),
        }
    }
}
