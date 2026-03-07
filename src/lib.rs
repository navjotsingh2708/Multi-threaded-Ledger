use std::{collections::HashMap, fs, io::BufReader, sync::mpsc::{Receiver, Sender}};
mod wal;
use bincode::Options;

#[derive(Debug, Clone)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Transaction {
    pub sender: String,
    pub receiver: String,
    pub amount: u64,
    pub timestamp: i64,
}

#[derive(Debug, Clone)]
struct Profiles {
    accounts: HashMap<String, u64>,
}
pub struct Ledger {
    accounts: Profiles,
    wal: wal::Wal,
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
    IoError,
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
    pub fn new(path: &str) -> std::io::Result<Self> {
        let wal = wal::Wal::new(path)?;
        Ok(Ledger {accounts: Profiles { accounts: HashMap::new() }, wal})
    }

    pub fn run(mut self, rx: Receiver<LedgerRequest>) {
        self.recover().ok();
        while let Ok(msg) = rx.recv() {
            match msg {
                LedgerRequest::AddTransaction { sender, receiver, amount, timestamp, respond_to } => {
                    let result = self.add(sender, receiver, amount, timestamp);
                    let _ = respond_to.send(result);
                }

                LedgerRequest::ListTransaction { respond_to } => {
                    let result = self.list_transactions();
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

    fn apply_transaction(&mut self, tx: Transaction) {
        let sender_bal = self.accounts.accounts.entry(tx.sender).or_insert(0);
        *sender_bal = sender_bal.saturating_sub(tx.amount);
        
        let rec_bal = self.accounts.accounts.entry(tx.receiver).or_insert(0);
        *rec_bal = rec_bal.saturating_add(tx.amount);
    }

    pub fn add(&mut self, sender: String, receiver: String, amount: u64, timestamp: i64, signature: vec<u8>) -> Result<(), TransactionError> {

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

        let config = bincode::DefaultOptions::new().with_fixint_encoding().allow_trailing_bytes();
        let bytes = config.serialize(&transaction).map_err(|_| TransactionError::IoError)?;
        self.wal.append(&bytes).map_err(|_| TransactionError::IoError)?;
        self.apply_transaction(transaction);
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

    pub fn recover(&mut self) -> std::io::Result<()> {
        let file = std::fs::File::open("ledger.bin")?;
        let mut reader = std::io::BufReader::new(file);

        let config = bincode::DefaultOptions::new().with_fixint_encoding().allow_trailing_bytes();

        loop {
            let result: bincode::Result<Transaction> = config.deserialize_from(&mut reader);

            match result {
                Ok(tx) => {
                    self.apply_transaction(tx);
                }
                Err(e) => {
                    match *e {
                        bincode::ErrorKind::Io(ref io_err)
                        if io_err.kind() == std::io::ErrorKind::UnexpectedEof => {
                            break;
                        }
                        _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                    }
                }
            }

        }

        Ok(())
    }

    pub fn list_transactions(&self) -> Vec<Transaction> {
        let mut list = Vec::new();

        let file = match fs::File::open("Ledger.bin") {
            Ok(f) => f,
            Err(_) => return vec![],
        };

        let mut reader = BufReader::new(file);
        let config = bincode::DefaultOptions::new().with_fixint_encoding().allow_trailing_bytes();

        while let Ok(tx) = config.deserialize_from::<_, Transaction>(&mut reader) {
            list.push(tx);
        }

        list
    }
}