use std::{collections::HashMap, sync::mpsc::{Receiver, Sender}};
mod wal;

#[derive(Debug, Clone)]
#[derive(serde::Serialize)]
pub struct Transaction {
    pub sender: String,
    pub receiver: String,
    pub amount: u64,
    pub timestamp: i64
}

#[derive(Debug, Clone)]
struct Profiles {
    accounts: HashMap<String, u64>,
}
pub struct Ledger {
    pub entries: Vec<Transaction>,
    accounts: Profiles
}

#[derive(Debug)]

pub enum LedgerRequest {
    AddTransaction {
        sender: String,
        receiver: String,
        amount: u64,
        timestamp: i64,
        respond_to: Sender<Result<(), TransactionError>>
    },
    ListTransaction {
        respond_to: Sender<Vec<Transaction>>
    },
    Profile {
        name: String,
        balance: u64,
        respond_to: Sender<Result<(), TransactionError>>
    },
    GetBalance {
        name: String,
        respond_to: Sender<Result<u64, TransactionError>>
    },
    ShutDown,
}

#[derive(Debug)]
pub enum TransactionError {
    ZeroAmount,
    SameSenderReceiver,
    AccountNotFound,
    AccountAlreadyExists,
    NotEnoughBalance,
}

impl Transaction {
    pub fn new(sender: &str, receiver: &str, amount: u64, timestamp: i64) -> Result<Self, TransactionError> {
        if amount == 0 {
            return Err(TransactionError::ZeroAmount)
        }
        if sender == receiver {
            return Err(TransactionError::SameSenderReceiver)
        }
        Ok(Transaction { sender: sender.to_string(), receiver: receiver.to_string(), amount, timestamp })
    }
}

impl Ledger {
    pub fn new() -> Self {
        Ledger { entries: vec![] , accounts: Profiles { accounts: HashMap::new() }}
    }

    pub fn run(mut self, rx: Receiver<LedgerRequest>) {
        while let Ok(msg) = rx.recv() {
            match msg {
                LedgerRequest::AddTransaction { sender, receiver, amount, timestamp, respond_to } => {
                    let result = self.add(sender, receiver, amount, timestamp);
                    let _ = respond_to.send(result);
                }

                LedgerRequest::ListTransaction { respond_to } => {
                    let result = self.entries.clone();
                    let _ = respond_to.send(result);
                }

                LedgerRequest::Profile { name, balance, respond_to } => {
                    let result = self.profile(name, balance);
                    let _ = respond_to.send(result);
                }
                
                LedgerRequest::GetBalance { name, respond_to } => {
                    let result = self.get_balance(&name);
                    let _ = respond_to.send(result);
                }

                LedgerRequest::ShutDown => {
                    break;
                }
            }
        }
    }

    pub fn add(&mut self, sender: String, receiver: String, amount: u64, timestamp: i64) -> Result<(), TransactionError> {
        let transaction = Transaction::new(&sender, &receiver, amount, timestamp)?;
        
        let sender_balance = self
            .accounts
            .accounts
            .get(&sender)
            .ok_or(TransactionError::AccountNotFound)?;

        let receiver_exists = self
            .accounts
            .accounts
            .contains_key(&receiver);

        if !receiver_exists {
            return Err(TransactionError::AccountNotFound);
        }

        if amount > *sender_balance {
            return Err(TransactionError::NotEnoughBalance);
        }
        
        *self.accounts.accounts.get_mut(&sender).unwrap() -= amount;
        *self.accounts.accounts.get_mut(&receiver).unwrap() += amount;
        
        self.entries.push(transaction);
        Ok(())
    }

    pub fn profile(&mut self, name: String, balance: u64) -> Result<(), TransactionError> {
        if self.accounts.accounts.contains_key(&name) {
            return Err(TransactionError::AccountAlreadyExists);
        }
        self.accounts.accounts.insert(name, balance);
        Ok(())
    }
    
    pub fn get_balance(&self, name: &str) -> Result<u64, TransactionError> {
        self.accounts.accounts.get(name).copied().ok_or(TransactionError::AccountNotFound)
    }
}