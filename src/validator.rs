use std::thread;
use crossbeam::channel::{bounded, Receiver, Sender};
use crossbeam::channel::Sender as ReplySender;
use serde::{Deserialize, Serialize};
use crate::{TransactionError};
use ed25519_dalek::{Signature, VerifyingKey, Verifier};

#[derive(Serialize, Deserialize, Debug, )]
pub struct VerificationTask {
    pub sender_name: String,
    pub receiver_name: String,
    pub amount: u64,
    pub timestamp: i64,
    pub signature: Signature,
    pub sequence: u64,
    pub sender_pubkey: [u8; 32],
    // This allows the worker to send the result back to the Ledger thread
    #[serde(skip)]
    pub respond_to: Option<ReplySender<Result<VerificationTask, TransactionError>>>,
    #[serde(skip)]
    pub client_respond_to: Option<ReplySender<Result<(), TransactionError>>>,
}
pub struct WorkerPool {
    pub sender: Sender<VerificationTask>,
    _handles: Vec<thread::JoinHandle<()>>
}

impl WorkerPool {
    pub fn new(num_threads: usize) -> Self {
        let (tx, rx): (Sender<VerificationTask>, Receiver<VerificationTask>) = bounded(1024);
        let mut handles = Vec::new();
        for _ in 0..num_threads {
            let rx_clone = rx.clone();
            let handle = thread::spawn(move || {
                while let Ok(task) = rx_clone.recv() {
                    let responder: Option<Sender<Result<VerificationTask, TransactionError>>> = task.respond_to.clone();
                    let client_responder: Option<Sender<Result<(), TransactionError>>> = task.client_respond_to.clone();

                    // 2. Run the math
                    let result = Self::verify_tx(task);
                    
                    // 3. Send the result (Success or Error) back to the Ledger thread
                    match result {
                        Ok(task) => {
                            if let Some(resp) = responder {
                                let _ = resp.send(Ok(task));
                            }
                            if let Some(cli_resp) = client_responder {
                                let _ = cli_resp.send(Ok(()));
                            }
                        }
                        Err(e) => {
                            if let Some(cli_resp) = client_responder {
                                let _ = cli_resp.send(Err(e));
                            }
                        }
                    }
                }
            });
            handles.push(handle);
        }
        WorkerPool { sender: tx, _handles: handles }
    }
    fn verify_tx(task: VerificationTask) -> Result<VerificationTask, TransactionError> {
        let sender_vk = VerifyingKey::from_bytes(&task.sender_pubkey).map_err(|_| TransactionError::InvalidSignature)?;
        let mut message = Vec::new();
        message.extend_from_slice(task.sender_name.as_bytes());
        message.extend_from_slice(task.receiver_name.as_bytes());
        message.extend_from_slice(&task.amount.to_le_bytes());
        message.extend_from_slice(&task.timestamp.to_le_bytes());
        message.extend_from_slice(&task.sequence.to_le_bytes());
    
        sender_vk.verify(&message, &task.signature).map_err(|_| TransactionError::InvalidSignature)?;
        Ok(task)
    }
}
