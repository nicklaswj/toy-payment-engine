use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

use super::Result;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::transaction;

#[derive(Default, Debug, PartialEq, Eq, Clone)]
struct Account {
    available: Decimal,
    held: Decimal,
    locked: bool,
}

#[derive(Serialize)]
pub struct SerializableAccount {
    client: u16,
    available: String,
    held: String,
    total: String,
    locked: bool,
}

impl SerializableAccount {
    fn from_account(client: u16, account: &Account) -> Self {
        Self {
            client,
            available: account.available.round_dp(4).to_string(),
            held: account.held.round_dp(4).to_string(),
            total: (account.available.round_dp(4) + account.held.round_dp(4)).to_string(),
            locked: account.locked,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct Bank {
    /// Map from client id to account details
    accounts: BTreeMap<u16, Account>,
    /// Map from transaction id to deposit transaction data
    deposits: BTreeMap<u32, transaction::TransactionAmountData>,
    /// Set of transaction id of disputed deposits
    disputes: BTreeSet<u32>,
}

impl Bank {
    /// Update the internal state of the bank according to the given transaction
    pub fn handle_transaction(&mut self, transaction: transaction::Transaction) {
        // Get the client's account
        let mut account = self.accounts.entry(transaction.client_id()).or_default();

        // Check that the account isn't locked
        if account.locked {
            return;
        }

        match transaction {
            transaction::Transaction::Deposit(data) => {
                account.available += data.amount;
                // deposit into the available funds
                self.deposits.insert(data.tx, data);
            }
            transaction::Transaction::Withdrawal(data) => {
                // Only apply withdraw if the account has enough available funds
                if data.amount <= account.available {
                    // Withdraw the funds
                    account.available -= data.amount
                }
            }
            transaction::Transaction::Dispute(data) => {
                // Get the disputed deposit, ignore if it doesn't exist
                if let Some(disputed_deposit) = self.deposits.get(&data.tx) {
                    // Check that the dispute client id is the same as the deposit client id,
                    // otherwise ignore
                    if data.client == disputed_deposit.client {
                        // Move the deposit into the held funds
                        account.available -= disputed_deposit.amount;
                        account.held += disputed_deposit.amount;

                        self.disputes.insert(data.tx);
                    }
                }
            }
            transaction::Transaction::Resolve(data) => {
                // Check that the transaction is actually under dispute, ignore if it is not
                if self.disputes.remove(&data.tx) {
                    // Get the disputed deposit, ignore if it doesn't exist
                    if let Some(disputed_deposit) = self.deposits.get(&data.tx) {
                        // Check that the dispute client id is the same as the resolve client id,
                        // otherwise ignore
                        if data.client == disputed_deposit.client {
                            // Reverse the dispute
                            account.held -= disputed_deposit.amount;
                            account.available += disputed_deposit.amount;
                        }
                    }
                }
            }
            transaction::Transaction::Chargeback(data) => {
                // Check that the transaction is actually under dispute, ignore if it is not
                if self.disputes.remove(&data.tx) {
                    // Get the disputed deposit, ignore if it doesn't exist
                    if let Some(disputed_deposit) = self.deposits.get(&data.tx) {
                        // Check that the dispute client id is the same as the chargeback client id,
                        // otherwise ignore
                        if data.client == disputed_deposit.client {
                            // Reverse the deposit
                            account.held -= disputed_deposit.amount;
                            account.locked = true;
                        }
                    }
                }
            }
        }
    }

    /// Serialize and write the current accounts out to writer
    pub fn write<W: io::Write>(&self, writer: W) -> Result<()> {
        // Construct csv writer
        let mut writer = csv::Writer::from_writer(writer);

        // Write the accounts
        for (&client, account) in &self.accounts {
            writer.serialize(SerializableAccount::from_account(client, &account))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::transaction::{TransactionAmountData, TransactionData};
    use rust_decimal_macros::dec;

    use super::*;

    const ALICE: u16 = 1;
    const BOB: u16 = 2;
    const CHRIS: u16 = 3;

    #[derive(Default)]
    struct TestBank {
        next_tx: u32,
        bank: Bank,
    }

    impl TestBank {
        fn deposit(&mut self, client: u16, amount: Decimal) -> u32 {
            let tx = self.next_tx;
            self.bank
                .handle_transaction(transaction::Transaction::Deposit(TransactionAmountData {
                    client,
                    tx,
                    amount,
                }));

            self.next_tx += 1;

            tx
        }

        fn withdraw(&mut self, client: u16, amount: Decimal) -> u32 {
            let tx = self.next_tx;
            self.bank
                .handle_transaction(transaction::Transaction::Withdrawal(
                    TransactionAmountData { client, tx, amount },
                ));

            self.next_tx += 1;

            tx
        }

        fn dispute(&mut self, client: u16, tx: u32) {
            self.bank
                .handle_transaction(transaction::Transaction::Dispute(TransactionData {
                    client,
                    tx,
                }));
        }

        fn resolve(&mut self, client: u16, tx: u32) {
            self.bank
                .handle_transaction(transaction::Transaction::Resolve(TransactionData {
                    client,
                    tx,
                }));
        }

        fn chargeback(&mut self, client: u16, tx: u32) {
            self.bank
                .handle_transaction(transaction::Transaction::Chargeback(TransactionData {
                    client,
                    tx,
                }));
        }
    }

    #[test]
    fn simple_deposit_and_withdrawal_test() {
        let mut bank = TestBank::default();

        bank.deposit(ALICE, dec!(10.0));

        bank.deposit(BOB, dec!(10.0));
        bank.deposit(BOB, dec!(10.0));

        bank.deposit(CHRIS, dec!(10.0));
        bank.deposit(CHRIS, dec!(10.0));
        bank.deposit(CHRIS, dec!(10.0));

        assert_eq!(bank.bank.accounts.get(&ALICE).unwrap().available, dec!(10));
        assert_eq!(bank.bank.accounts.get(&BOB).unwrap().available, dec!(20));
        assert_eq!(bank.bank.accounts.get(&CHRIS).unwrap().available, dec!(30));
    }

    #[test]
    fn simple_withdrawal_test() {
        let mut bank = TestBank::default();

        bank.deposit(ALICE, dec!(10.0));

        bank.deposit(BOB, dec!(10.0));
        bank.deposit(BOB, dec!(10.0));

        bank.deposit(CHRIS, dec!(10.0));
        bank.deposit(CHRIS, dec!(10.0));
        bank.deposit(CHRIS, dec!(10.0));

        bank.withdraw(ALICE, dec!(5));
        bank.withdraw(BOB, dec!(5));
        bank.withdraw(CHRIS, dec!(5));

        assert_eq!(bank.bank.accounts.get(&ALICE).unwrap().available, dec!(5));
        assert_eq!(bank.bank.accounts.get(&BOB).unwrap().available, dec!(15));
        assert_eq!(bank.bank.accounts.get(&CHRIS).unwrap().available, dec!(25));
    }

    #[test]
    fn overdraw_test() {
        let mut bank = TestBank::default();

        bank.deposit(ALICE, dec!(10.0));
        bank.withdraw(ALICE, dec!(20.0));

        assert_eq!(
            bank.bank.accounts.get(&ALICE).unwrap().available,
            dec!(10.0)
        );
    }

    #[test]
    fn dispute_withdrawal_no_effect_test() {
        let mut bank = TestBank::default();

        bank.deposit(ALICE, dec!(10.0));
        let withdraw_tx = bank.withdraw(ALICE, dec!(5.0));

        let bank_clone = bank.bank.clone();

        bank.dispute(ALICE, withdraw_tx);

        assert_eq!(bank.bank, bank_clone);
    }

    #[test]
    fn dispute_client_transaction_id_mismatch_test() {
        let mut bank = TestBank::default();

        let deposit_tx = bank.deposit(ALICE, dec!(10.0));
        bank.deposit(BOB, dec!(10.));

        let bank_clone = bank.bank.clone();

        bank.dispute(BOB, deposit_tx);

        assert_eq!(bank.bank, bank_clone);
    }

    #[test]
    fn dispute_to_resolve_test() {
        let mut bank = TestBank::default();

        bank.deposit(ALICE, dec!(10.0));
        let deposit_tx = bank.deposit(ALICE, dec!(10.0));

        bank.dispute(ALICE, deposit_tx);
        assert!(bank.bank.disputes.contains(&deposit_tx));
        assert_eq!(bank.bank.accounts.get(&ALICE).unwrap().held, dec!(10.0));

        bank.resolve(ALICE, deposit_tx);

        assert_eq!(
            bank.bank.accounts.get(&ALICE).unwrap().available,
            dec!(20.0)
        );
    }

    #[test]
    fn dispute_to_chargeback_test() {
        let mut bank = TestBank::default();

        bank.deposit(ALICE, dec!(10.0));
        let deposit_tx = bank.deposit(ALICE, dec!(10.0));

        bank.dispute(ALICE, deposit_tx);
        assert!(bank.bank.disputes.contains(&deposit_tx));
        assert_eq!(bank.bank.accounts.get(&ALICE).unwrap().held, dec!(10.0));

        bank.chargeback(ALICE, deposit_tx);

        let account = bank.bank.accounts.get(&ALICE).unwrap();
        assert_eq!(account.available, dec!(10.0));
        assert!(account.locked);
    }
}
