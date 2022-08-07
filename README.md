# Building and running
To build the project run the command:

    cargo build --release

Or to run the project directly, run the command:

    cargo run --release -- test.csv > output.csv

# Correctness and tests
Both modules `account` and `transaction` contains unit tests that exercises most parts of the program,
especically in the `transaction` module I have tried to cover all cases of the transaction logic. In addition
I have included a small test.csv input file in the repo which should produce the following out:

    $ cargo run --release -- test.csv
    client,available,held,total,locked
    1,1.5,0.0000,1.5,false
    2,2,0.0000,2,false

To run the unit tests, execute the following command:

    cargo test

To help with the correctness of the input transactions I have made a `Enum` where each variant correspond to a
transaction type and where each variant only exactly holds the data needed for that given type, i.e. no
`amount` field in a `dispute` transaction.

# Quirks
## Semi manual deserialization of Transaction 
In the transaction module I deserialize every record/row in the input file to different variants of the same
enum, depending on type field. However this is not supported by the csv crate as described in the following
[GitHub issue](https://github.com/BurntSushi/rust-csv/issues/211). To overcome this I semi manually deserialize
the csv records in the function `Transaction::from_record`, where I manually look at the `type` field to determine
how to serialize the rest of the csv record.

# Unsafe
This crate does not directly contain any unsafe code.

# Threading and/or async
## Async
I chose not to use `async` since the program can only get data from one input file, compared to e.g. multiple files or
network streams, which defeat the purpose of `async` and would only add overhead.
## Threading
I could have threaded the program such that I had one thread reading the file and another thread process the data.
However I found that it would be overkill - and my gutfilling tells me that it's IO bound for most cases anyway.

# Assumptions
The only thing I felt I had to assume and was not directly stated in the problem, was the consequence of a account being
locked/frozen. I Therefor assume that when an account is locked, all further transactions belonging to the account
are ignored.
