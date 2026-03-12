use std::{collections::HashMap, fs, io::BufReader, sync::mpsc::{Receiver, Sender}};
use bincode::Options;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
mod wal;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Transaction {
    pub sender: VerifyingKey,
    pub receiver: VerifyingKey,
    pub amount: u64,
    pub timestamp: i64,
    pub signature: Signature,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum WalEntry {
    CreateProfile {
        name: String,
        key: [u8; 32],
        balance: u64
    },
    Transfer(Transaction),
}

#[derive(Debug, Clone)]
struct Profiles {
    balances: HashMap<[u8; 32], u64>,
    names: HashMap<[u8; 32], String>,
    name_to_key: HashMap<String, [u8; 32]>,
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
        signature: Signature,
        respond_to: Sender<Result<(), TransactionError>>
    },
    ListTransaction {
        respond_to: Sender<Vec<Transaction>>
    },
    Profile {
        name: String,
        key: [u8; 32],
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
    InvalidSignature,
}

impl Transaction {
    pub fn new(sender: VerifyingKey, receiver: VerifyingKey, amount: u64, timestamp: i64, signature: Signature) -> Result<Self, TransactionError> {
        if amount == 0 {
            return Err(TransactionError::ZeroAmount)
        }
        if sender == receiver {
            return Err(TransactionError::SameSenderReceiver)
        }
        Ok(Transaction { sender, receiver, amount, timestamp, signature})
    }
}

impl Ledger {
    pub fn new(path: &str) -> std::io::Result<Self> {
        let wal = wal::Wal::new(path)?;
        Ok(Ledger {accounts: Profiles { balances: HashMap::new(), names: HashMap::new(), name_to_key: HashMap::new() }, wal})
    }

    pub fn run(mut self, rx: Receiver<LedgerRequest>) {
        self.recover().ok();
        while let Ok(msg) = rx.recv() {
            match msg {
                LedgerRequest::AddTransaction { sender, receiver, amount, timestamp, signature, respond_to } => {
                    let result = self.add(sender, receiver, amount, timestamp, signature);
                    let _ = respond_to.send(result);
                }

                LedgerRequest::ListTransaction { respond_to } => {
                    let result = self.list_transactions();
                    let _ = respond_to.send(result);
                }

                LedgerRequest::Profile { name, balance, key, respond_to } => {
                    let result = self.profile(name, balance, key);
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
        let s_bytes = tx.sender.to_bytes();
        let r_bytes = tx.receiver.to_bytes();

        if let Some(bal) = self.accounts.balances.get_mut(&s_bytes) {
           *bal = bal.saturating_sub(tx.amount);
        }
        if let Some(bal) = self.accounts.balances.get_mut(&r_bytes) {
            *bal = bal.saturating_add(tx.amount);
        }
    }

    pub fn add(&mut self, sender: String, receiver: String, amount: u64, timestamp: i64, signature: Signature) -> Result<(), TransactionError> {
        
        let sender_key = self
            .accounts
            .name_to_key
            .get(&sender)
            .ok_or(TransactionError::AccountNotFound)?;

        let receiver_key = self
            .accounts
            .name_to_key
            .get(&receiver)
            .ok_or(TransactionError::AccountNotFound)?;

        let sender_bal = self.accounts.balances.get(sender_key).ok_or(TransactionError::AccountNotFound)?;

        if amount > *sender_bal {
            return Err(TransactionError::NotEnoughBalance);
        }

        let sender_vk = VerifyingKey::from_bytes(sender_key).map_err(|_| TransactionError::IoError)?;
        let receiver_vk = VerifyingKey::from_bytes(receiver_key).map_err(|_| TransactionError::IoError)?;

        let mut message = Vec::new();
        message.extend_from_slice(sender.as_bytes());
        message.extend_from_slice(receiver.as_bytes());
        message.extend_from_slice(&amount.to_le_bytes());
        message.extend_from_slice(&timestamp.to_le_bytes());

        sender_vk.verify(&message, &signature).map_err(|_| TransactionError::InvalidSignature)?;

        let tx = Transaction::new(sender_vk, receiver_vk, amount, timestamp, signature)?;
        let entry = WalEntry::Transfer(tx.clone());
        let config = bincode::DefaultOptions::new().with_fixint_encoding().allow_trailing_bytes();
        let bytes = config.serialize(&entry).map_err(|_| TransactionError::IoError)?;
        self.wal.append(&bytes).map_err(|_| TransactionError::IoError)?;
        self.apply_transaction(tx);
        Ok(())
    }

    pub fn profile(&mut self, name: String, balance: u64, key: [u8; 32]) -> Result<(), TransactionError> {
        
        if self.accounts.name_to_key.contains_key(&name) {
            return Err(TransactionError::AccountAlreadyExists);
        }
        
        let entry = WalEntry::CreateProfile { name: name.clone(), key, balance };

        let config = bincode::DefaultOptions::new().with_fixint_encoding().allow_trailing_bytes();
        let bytes = config.serialize(&entry).map_err(|_| TransactionError::IoError)?;
        self.wal.append(&bytes).map_err(|_| TransactionError::IoError)?;
        

        self.accounts.name_to_key.insert(name.clone(), key);
        self.accounts.names.insert(key, name);
        self.accounts.balances.insert(key, balance);

        Ok(())
    }
    
    pub fn get_balance(&self, name: &str) -> Result<u64, TransactionError> {
        let key = self.accounts.name_to_key.get(name).ok_or(TransactionError::AccountNotFound)?;
        self.accounts.balances.get(key).copied().ok_or(TransactionError::AccountNotFound)
    }

    pub fn recover(&mut self) -> std::io::Result<()> {
        let file = std::fs::File::open("ledger.log").map_err(|e| {
            println!("DEBUG: No ledger.bin found! Creating fresh state.");
            e
        })?;
        let mut reader = std::io::BufReader::new(file);

        let config = bincode::DefaultOptions::new().with_fixint_encoding().allow_trailing_bytes();

        loop {
            let result: bincode::Result<WalEntry> = config.deserialize_from(&mut reader);

            match result {
                Ok(entry) => {
                    match entry {
                        WalEntry::CreateProfile { name, key, balance } => {
                            self.accounts.name_to_key.insert(name.clone(), key);
                            self.accounts.names.insert(key, name.clone());
                            self.accounts.balances.insert(key, balance);
                            println!("DEBUG: Successfully recovered {}", name.clone());
                        }
                        WalEntry::Transfer(tx) => {
                            self.apply_transaction(tx);
                        }
                    }
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