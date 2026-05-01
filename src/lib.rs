use std::{collections::{BTreeMap, HashMap}, fs, io::BufReader};
use ed25519_dalek::{Signature, VerifyingKey};
use crossbeam::{channel::{Receiver, Sender, select}};
use serde::{Deserialize, Serialize};
use crate::validator::VerificationTask;
use bincode::Options;
use std::fmt;
pub mod validator;
pub mod crypto;
pub mod threadpool;
mod wal;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Transaction {
    pub sender: VerifyingKey,
    pub receiver: VerifyingKey,
    pub amount: u64,
    pub timestamp: i64,
    pub signature: Signature,
    pub sequence: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum WalEntry {
    CreateProfile {
        name: String,
        key: [u8; 32],
        balance: u64,
        last_sequence: u64,
    },
    Transfer(Transaction),
}

#[derive(Debug, Clone)]
struct Profiles {
    balances: HashMap<[u8; 32], u64>,
    names: HashMap<[u8; 32], String>,
    name_to_key: HashMap<String, [u8; 32]>,
    sequences: HashMap<[u8; 32], u64>,
}
pub struct Ledger {
    accounts: Profiles,
    wal: wal::Wal,
    pending_queue: HashMap<[u8; 32], BTreeMap<u64, VerificationTask>>,
}

#[derive(Debug)]
pub enum LedgerRequest {
    ListTransaction {
        sender: Option<String>,
        respond_to: Sender<Result<Vec<Transaction>, TransactionError>>
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
    GetSequence {
        account: String,
        respond_to: Sender<Result<u64, TransactionError>>
    },
    ShutDown,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ClientRequest {
    Transfer(VerificationTask),
    CreateProfile {name: String, balance: u64, key: [u8; 32]},
    GetBalance {name: String},
    ShutDown,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ServerResponse {
    Success,
    Balance(u64),
    Error(String),
    Queued,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum LedgerConfirm {
    Committed,
    Queued,
}

#[derive(Debug)]
pub enum TransactionError {
    ZeroAmount,
    SameSenderReceiver,
    AccountNotFound,
    AccountAlreadyExists,
    NotEnoughBalance,
    IoError,
    WalError,
    InvalidSignature,
    SequenceTooFar,
    QueueFull,
    DuplicateSequence,
    OldSequence,
}

impl fmt::Display for TransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroAmount => write!(f, "\nTransaction amount must be greater than zero."),
            Self::SameSenderReceiver => write!(f, "\nSender and receiver cannot be the same account."),
            Self::AccountNotFound => write!(f, "\nThe specified account was not found in the ledger."),
            Self::AccountAlreadyExists => write!(f, "\nAn account with this name already exists."),
            Self::NotEnoughBalance => write!(f, "\nInsufficient funds to complete this transaction."),
            Self::IoError => write!(f, "\nA disk I/O error occurred while accessing the ledger log."),
            Self::WalError => write!(f, "\nA WAL error occurred while writing to the ledger log."),
            Self::InvalidSignature => write!(f, "\nCryptographic signature verification failed."),
            Self::SequenceTooFar => write!(f, "\nTransaction Sequence is too ahead in future."),
            Self::QueueFull => write!(f, "\nYour queue of future transactions is full."),
            Self::DuplicateSequence => write!(f, "\nThe sequence is a Duplicate of a pending sequence"),
            Self::OldSequence => write!(f, "\nThe sequence is a Old")
        }
    }
}
impl std::error::Error for TransactionError {}

impl Transaction {
    pub fn new(sender: VerifyingKey, receiver: VerifyingKey, amount: u64, timestamp: i64, signature: Signature, sequence: u64) -> Result<Self, TransactionError> {
        if amount == 0 {
            return Err(TransactionError::ZeroAmount)
        }
        if sender == receiver {
            return Err(TransactionError::SameSenderReceiver)
        }
        Ok(Transaction { sender, receiver, amount, timestamp, signature, sequence })
    }
}

impl Ledger {
    pub fn new(path: &str, shutdown_tx: Sender<()>) -> std::io::Result<Self> {
        let wal = wal::Wal::new(path, shutdown_tx)?;
        Ok(Ledger {accounts: Profiles { balances: HashMap::new(), names: HashMap::new(), name_to_key: HashMap::new(), sequences: HashMap::new() }, wal, pending_queue: HashMap::new()})
    }

    pub fn set_test_account(&mut self, name: String, key: [u8; 32], balance: u64) {
        self.accounts.name_to_key.insert(name.clone(), key);
        self.accounts.names.insert(key, name);
        self.accounts.balances.insert(key, balance);
        self.accounts.sequences.insert(key, 0);
    }

    pub fn reset_test_sequence(&mut self, key: &[u8; 32]) {
        self.accounts.sequences.insert(*key, 0);
    }

    pub fn run(mut self, cli_rx: Receiver<LedgerRequest>, verified_rx: Receiver<Result<VerificationTask, TransactionError>>) {
        self.recover().ok();

        loop {
            select! {
                recv(cli_rx) -> msg => {
                    match msg {
                        Ok(LedgerRequest::ListTransaction { sender, respond_to }) => {
                            match sender {
                                Some(name) => {
                                    let result = self.find_by_sender(&name);
                                    let _ = respond_to.send(result);
                                },
                                None => {
                                    let result = self.list_transactions();
                                    let _ = respond_to.send(Ok(result));
                                }
                            }
                        }
            
                        Ok(LedgerRequest::Profile { name, balance, key, respond_to }) => {
                            let result = self.profile(name, balance, key, 0);
                            let _ = respond_to.send(result);
                        }
                        
                        Ok(LedgerRequest::GetBalance { name, respond_to }) => {
                            let result = self.get_balance(&name);
                            let _ = respond_to.send(result);
                        }
            
                        Ok(LedgerRequest::GetSequence { account, respond_to }) => {
                            let result = self.get_sequence(&account);
                            let _ = respond_to.send(result);
                        }
            
                        Ok(LedgerRequest::ShutDown) => {
                            println!("Ledger shutting down...");
                            return;
                        }
                        Err(e) => {
                            eprintln!("Ledger channel disconnected: {:?}", e);
                            break;
                        }
                        _ => {eprintln!("Ledger received unexpected request");}
                    }
                }
                recv(verified_rx) -> task_res => {
                    if let Ok(result) = task_res {
                        match result {
                            Ok(task) => {
                                let _ = self.add(task);
                            },
                            Err(e) => eprintln!("Worker rejected transaction: {:?}", e),
                        }
                    }
                }
            }
        }
    }

    fn process_single_task(&mut self, task: VerificationTask) -> Result<(), TransactionError> {
        
        let sender_bal = self.accounts.balances.get(&task.sender_pubkey).ok_or(TransactionError::AccountNotFound)?;

        if task.amount > *sender_bal {
            return Err(TransactionError::NotEnoughBalance);
        }

        let receiver_key = self
            .accounts
            .name_to_key
            .get(&task.receiver_name)
            .ok_or(TransactionError::AccountNotFound)?;
        let receiver_vk = VerifyingKey::from_bytes(receiver_key).map_err(|_| TransactionError::InvalidSignature)?;


        let tx = Transaction {
            sender: VerifyingKey::from_bytes(&task.sender_pubkey).unwrap(),
            receiver: receiver_vk,
            amount: task.amount,
            timestamp: task.timestamp,
            signature: task.signature,
            sequence: task.sequence,
        };

        let entry = WalEntry::Transfer(tx.clone());
        let config = bincode::DefaultOptions::new().with_fixint_encoding().allow_trailing_bytes();
        let bytes = config.serialize(&entry).map_err(|_| TransactionError::IoError)?;
        self.wal.append(bytes).map_err(|_| TransactionError::WalError)?;
        self.apply_transaction(tx);

        if let Some(cli_resp) = task.client_respond_to {
            let _ = cli_resp.send(Ok(LedgerConfirm::Committed));
        }
        Ok(())
    }

    fn apply_transaction(&mut self, tx: Transaction) {
        let s_bytes = tx.sender.to_bytes();
        let r_bytes = tx.receiver.to_bytes();
        
        let current_seq = self.accounts.sequences.get(&s_bytes).copied().unwrap_or(0);
        if tx.sequence <= current_seq {
            println!("DEBUG: Skipping duplicate or old transaction seq: {}", tx.sequence);
            return; 
        }

        if let Some(bal) = self.accounts.balances.get_mut(&s_bytes) {
           *bal = bal.saturating_sub(tx.amount);
        }
        if let Some(bal) = self.accounts.balances.get_mut(&r_bytes) {
            *bal = bal.saturating_add(tx.amount);
        }
        self.accounts.sequences.insert(s_bytes, tx.sequence);
    }

    pub fn add(&mut self, task: VerificationTask) -> Result<(), TransactionError> {
        let current_seq = self.accounts.sequences.get(&task.sender_pubkey).copied().unwrap_or(0);
        
        if task.sequence <= current_seq {
            println!("DEBUG: Droping old seq: {}", task.sequence);
            if let Some(cli_resp) = task.client_respond_to {
                let _ = cli_resp.send(Err(TransactionError::OldSequence));
            }
            return Ok(());
        }

        let max_future_gap = 50;
        if task.sequence > current_seq + max_future_gap {
            if let Some(cli_resp) = task.client_respond_to {
                let _ = cli_resp.send(Err(TransactionError::SequenceTooFar));
            }
            return Ok(());
        }

        if task.sequence > current_seq + 1 {
            let user_queue = self.pending_queue.entry(task.sender_pubkey).or_default();
            if user_queue.contains_key(&task.sequence) {
                println!("\nDEBUG: Dropping duplicate pending sequence: {}", task.sequence);
                if let Some(cli_resp) = task.client_respond_to {
                    let _ = cli_resp.send(Err(TransactionError::DuplicateSequence));
                }
                return Ok(()); // Drop it. We already have a candidate for this sequence.
            }
            if user_queue.len() >= 20 {
                if let Some(r) = task.client_respond_to {
                    let _ = r.send(Err(TransactionError::QueueFull));
                }
                return Ok(());
            }
            let client_resp = task.client_respond_to.clone();
            user_queue.insert(task.sequence, task);
            if let Some(r) = client_resp {
                let _ = r.send(Ok(LedgerConfirm::Queued));
            }
            return Ok(());
        }

        if task.sequence == current_seq + 1 {
            let pub_key = task.sender_pubkey;
            self.process_single_task(task)?;

            loop {
                let next_expected_seq = self.accounts.sequences.get(&pub_key).copied().unwrap_or(0) + 1;
                let next_task = {
                    if let Some(queue) = self.pending_queue.get_mut(&pub_key) {
                        queue.remove(&next_expected_seq)
                    } else {
                        None
                    }
                };

                match next_task {
                    Some(t) => {
                        if let Err(e) = self.process_single_task(t) {
                            println!("\nDEBUG: Pending transaction failed execution: {:?}", e);
                            break;
                        }
                    }
                    None => break,
                }
            }
        }

        Ok(())
    }

    pub fn profile(&mut self, name: String, balance: u64, key: [u8; 32], last_sequence: u64) -> Result<(), TransactionError> {
        
        if self.accounts.name_to_key.contains_key(&name) {
            return Err(TransactionError::AccountAlreadyExists);
        }
        
        let entry = WalEntry::CreateProfile { name: name.clone(), key, balance, last_sequence };

        let config = bincode::DefaultOptions::new().with_fixint_encoding().allow_trailing_bytes();
        let bytes = config.serialize(&entry).map_err(|_| TransactionError::IoError)?;
        self.wal.append(bytes).map_err(|_| TransactionError::WalError)?;
        

        self.accounts.name_to_key.insert(name.clone(), key);
        self.accounts.names.insert(key, name);
        self.accounts.balances.insert(key, balance);
        self.accounts.sequences.insert(key, last_sequence);

        Ok(())
    }
    
    pub fn get_balance(&self, name: &str) -> Result<u64, TransactionError> {
        let key = self.accounts.name_to_key.get(name).ok_or(TransactionError::AccountNotFound)?;
        self.accounts.balances.get(key).copied().ok_or(TransactionError::AccountNotFound)
    }

    pub fn recover(&mut self) -> std::io::Result<()> {
        let file = std::fs::File::open("ledger.log").map_err(|e| {
            println!("\nDEBUG: No ledger.log found! Creating fresh state.");
            e
        })?;
        let mut reader = std::io::BufReader::new(file);

        let config = bincode::DefaultOptions::new().with_fixint_encoding().allow_trailing_bytes();

        loop {
            let result: bincode::Result<WalEntry> = config.deserialize_from(&mut reader);

            match result {
                Ok(entry) => {
                    match entry {
                        WalEntry::CreateProfile { name, key, balance , last_sequence} => {
                            self.accounts.name_to_key.insert(name.clone(), key);
                            self.accounts.names.insert(key, name);
                            self.accounts.balances.insert(key, balance);
                            self.accounts.sequences.insert(key, last_sequence);
                            // println!("DEBUG: Successfully recovered {}", name.clone());
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

        let file = match fs::File::open("ledger.log") {
            Ok(f) => f,
            Err(_) => return vec![],
        };

        let mut reader = BufReader::new(file);
        let config = bincode::DefaultOptions::new().with_fixint_encoding().allow_trailing_bytes();

        while let Ok(entry) = config.deserialize_from::<_, WalEntry>(&mut reader) {
            if let WalEntry::Transfer(tx) = entry {
                list.push(tx);
            }
        }

        list
    }

    pub fn get_sequence(&self, name: &str) -> Result<u64, TransactionError> {
        let key = self.accounts.name_to_key.get(name).ok_or(TransactionError::AccountNotFound)?;
        let seq = self.accounts.sequences.get(key).copied().ok_or(TransactionError::AccountNotFound)?;
        Ok(seq + 1)
    }

    pub fn find_by_sender(&self, name: &str) -> Result<Vec<Transaction>, TransactionError> {
        let key = self.accounts.name_to_key.get(name).ok_or(TransactionError::AccountNotFound)?;
        let mut list = Vec::new();
        let file = match fs::File::open("ledger.log") {
            Ok(f) => f,
            Err(_) => return Err(TransactionError::IoError),
        };
        let mut reader = BufReader::new(file);
        let config = bincode::DefaultOptions::new().with_fixint_encoding().allow_trailing_bytes();

        while let Ok(e) = config.deserialize_from::<_, WalEntry>(&mut reader) {
            if let WalEntry::Transfer(tx) = e {
                if tx.sender.to_bytes() == *key {
                    list.push(tx);
                }
            }
        }

        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // helper — creates a test ledger with two accounts
    // called at the start of every test that needs accounts
    fn setup_ledger(filename: &str) -> Ledger {
        let mut ledger = Ledger::new(filename, None)
            .expect("Failed to create test ledger");
        
        let alice_key = crypto::setup("alice").expect("Crypto alice error"); // fake public key, all 1s
        let bob_key   = crypto::setup("bob").expect("Crypto bob error"); // fake public key, all 2s
        
        ledger.set_test_account("alice".into(), alice_key.to_bytes(), 1000);
        ledger.set_test_account("bob".into(),   bob_key.to_bytes(),   500);
        ledger
    }
    #[test]
    fn test_basic_transfer_update_balances() {
        let mut ledger = setup_ledger("T1.log");
        let task = VerificationTask {
            sender_name: "alice".into(),
            receiver_name: "bob".into(),
            amount: 100,
            client_respond_to: None,
            timestamp: 0,
            signature: Signature::from_bytes(&[0u8; 64]),
            sequence: 1,
            sender_pubkey: *ledger.accounts.name_to_key.get("alice").expect("Alice NO Sender PUB KEY"),
            respond_to: None,
        };
        let _ = ledger.add(task);
        assert_eq!(ledger.get_balance("alice").expect("Alice GET BALANCE ERROR"), 900);
        assert_eq!(ledger.get_balance("bob").expect("BOB GET BALANCE ERROR"), 600);
    }
    
    #[test]
    fn test_future_sequence_then_applied() {
        let mut ledger = setup_ledger("T2.log");
        let task = VerificationTask {
            sender_name: "alice".into(),
            receiver_name: "bob".into(),
            amount: 100,
            client_respond_to: None,
            timestamp: 0,
            signature: Signature::from_bytes(&[0u8; 64]),
            sequence: 2,
            sender_pubkey: *ledger.accounts.name_to_key.get("alice").expect("Alice NO Sender PUB KEY"),
            respond_to: None,
        };
        let _ = ledger.add(task);
        assert_eq!(ledger.get_balance("alice").expect("Alice GET BALANCE ERROR"), 1000);
        assert_eq!(ledger.get_balance("bob").expect("BOB GET BALANCE ERROR"), 500);

        let task = VerificationTask {
            sender_name: "alice".into(),
            receiver_name: "bob".into(),
            amount: 100,
            client_respond_to: None,
            timestamp: 0,
            signature: Signature::from_bytes(&[0u8; 64]),
            sequence: 3,
            sender_pubkey: *ledger.accounts.name_to_key.get("alice").expect("Alice NO Sender PUB KEY"),
            respond_to: None,
        };
        let _ = ledger.add(task);
        assert_eq!(ledger.get_balance("alice").expect("Alice GET BALANCE ERROR"), 1000);
        assert_eq!(ledger.get_balance("bob").expect("BOB GET BALANCE ERROR"), 500);
        
        let task = VerificationTask {
            sender_name: "alice".into(),
            receiver_name: "bob".into(),
            amount: 100,
            client_respond_to: None,
            timestamp: 0,
            signature: Signature::from_bytes(&[0u8; 64]),
            sequence: 1,
            sender_pubkey: *ledger.accounts.name_to_key.get("alice").expect("Alice NO Sender PUB KEY"),
            respond_to: None,
        };
        let _ = ledger.add(task);
        assert_eq!(ledger.get_balance("alice").expect("Alice GET BALANCE ERROR"), 700);
        assert_eq!(ledger.get_balance("bob").expect("BOB GET BALANCE ERROR"), 800);
    }

}