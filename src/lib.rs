use std::{collections::HashMap, error::Error, sync::mpsc::{Receiver, Sender}};

#[derive(Debug, Clone)]
pub struct Transaction {
    pub sender: String,
    pub receiver: String,
    pub amount: u32,
    pub timestamp: i64
}

#[derive(Debug, Clone)]
struct Profiles {
    accounts: HashMap<String, i32>,
}
pub struct Ledger {
    pub entries: Vec<Transaction>,
    accounts: Profiles
}

pub enum LedgerRequest {
    AddTransaction {
        sender: String,
        receiver: String,
        amount: u32,
        respond_to: Sender<Result<(), TransactionError>>
    },
    ListTransaction {
        respond_to: Sender<Vec<Transaction>>
    },
    Profile {
        name: String,
        balance: i32,
        respond_to: Sender<Result<(), Box<dyn Error>>>
    },
    Contains {
        sender: String,
        receiver: String,
        respond_to: Sender<Result<(), Box<dyn Error>>>
    },
    ShutDown,
}

#[derive(Debug)]
pub enum TransactionError {
    ZeroAmount,
    SameSenderReceiver,
}

impl Transaction {
    pub fn new(sender: String, receiver: String, amount: u32, timestamp: i64) -> Result<Self, TransactionError> {
        if amount == 0 {
            return Err(TransactionError::ZeroAmount)
        }
        if sender == receiver {
            return Err(TransactionError::SameSenderReceiver)
        }
        Ok(Transaction { sender, receiver, amount, timestamp })
    }
}

impl Ledger {
    pub fn new() -> Self {
        Ledger { entries: vec![] , accounts: Profiles { accounts: HashMap::new() }}
    }

    pub fn run(mut self, rx: Receiver<LedgerRequest>) {
        while let Ok(msg) = rx.recv() {
            match msg {
                LedgerRequest::AddTransaction { sender, receiver, amount, respond_to } => {
                    let result = self.add(sender, receiver, amount);
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

                LedgerRequest::Contains { sender, receiver, respond_to } => {
                    let result = self.contains(sender, receiver);
                    let _ = respond_to.send(result);
                }
                
                LedgerRequest::ShutDown => {
                    break;
                }
            }
        }
    }

    pub fn add(&mut self, sender: String, receiver: String, amount: u32) -> Result<(), TransactionError> {
        let timestamp = chrono::Utc::now().timestamp();
        let transaction = Transaction::new(sender, receiver, amount, timestamp)?;
        self.entries.push(transaction);
        Ok(())
    }

    pub fn profile(&mut self, name: String, balance: i32) -> Result<(), Box<dyn Error>> {
        if self.accounts.accounts.contains_key(&name) {
            return Err("The account already exists".into());
        }
        self.accounts.accounts.insert(name, balance);
        Ok(())
    }
    
    pub fn contains(&self, sender: String, receiver: String) -> Result<(), Box<dyn Error>> {
        if !self.accounts.accounts.contains_key(&sender) {
            return Err("Sender's account doesn't exists".into())
        }
        if !self.accounts.accounts.contains_key(&receiver) {
            return Err("Receiver's account doesn't exists".into())
        }
        Ok(())
    }
}