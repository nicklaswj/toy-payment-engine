use std::collections::{BTreeMap, BTreeSet};

use rust_decimal::Decimal;

use crate::transaction;

#[derive(Default)]
struct Account {
    available: Decimal,
    held: Decimal,
    locked: bool,
}

#[derive(Default)]
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
                    // Move the deposit into the held funds
                    account.available -= disputed_deposit.amount;
                    account.held += disputed_deposit.amount;

                    self.disputes.insert(data.tx);
                }
            }
            transaction::Transaction::Resolve(data) => {
                // Check that the transaction is actually under dispute, ignore if it is not
                if self.disputes.remove(&data.tx) {
                    // Get the disputed deposit, ignore if it doesn't exist
                    if let Some(disputed_deposit) = self.deposits.get(&data.tx) {
                        // Reverse the dispute
                        account.held -= disputed_deposit.amount;
                        account.available += disputed_deposit.amount;
                    }
                }
            }
            transaction::Transaction::Chargeback(data) => {
                // Check that the transaction is actually under dispute, ignore if it is not
                if self.disputes.remove(&data.tx) {
                    // Get the disputed deposit, ignore if it doesn't exist
                    if let Some(disputed_deposit) = self.deposits.get(&data.tx) {
                        // Reverse the deposit
                        account.held -= disputed_deposit.amount;
                        account.locked = true;
                    }
                }
            }
        }
    }
}
