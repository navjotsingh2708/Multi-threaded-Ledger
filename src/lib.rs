use std::sync::mpsc::{Sender, Receiver};

#[derive(Debug, Clone)]
pub struct Transaction {
    pub sender: String,
    pub receiver: String,
    pub amount: u32,
    pub timestamp: i64,
}

#[derive(Debug, Clone)]
pub struct Ledger {
    pub entries: Vec<Transaction>,
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
        Ledger { entries: vec![] }
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
}