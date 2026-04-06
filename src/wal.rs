use std::io::{Write, BufWriter};
use std::thread;
use crossbeam::channel::{Sender, bounded, tick};
use crossbeam::select;

pub struct Wal {
    // writer: BufWriter<File>,
    tx: Sender<Vec<u8>>
}

impl Wal {
    pub fn new(path: &str) -> std::io::Result<Self> {
        // Ok(Self { writer: BufWriter::new(file) })
        let (tx, rx) = bounded::<Vec<u8>>(10000);
        let file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;

        thread::spawn(move || {
            let mut writer = BufWriter::new(file);
            let ticker = tick(std::time::Duration::from_millis(10));
            let mut buffer: Vec<Vec<u8>> = Vec::with_capacity(100);

            loop {
                let mut should_flush = false;
                select! {
                    recv(rx) -> trans => {
                        let trans: Vec<u8> = match trans {
                            Ok(t) => t,
                            Err(_) => {
                                for entry in &buffer {
                                    writer.write_all(entry).expect("Disk write failed");
                                }
                                writer.flush().expect("Flush failed");
                                writer.get_mut().sync_all().expect("Sync failed");
                                buffer.clear();
                                break;
                            },
                        };
                        buffer.push(trans);
                        if buffer.len() >= 100 {
                            should_flush = true;
                        }
                    }

                    recv(ticker) -> _ => {
                        if !buffer.is_empty() {
                            should_flush = true;
                        }
                    }
                }
                if should_flush == true {
                    for entry in &buffer {
                        writer.write_all(entry).expect("Disk write failed");
                    }
                    writer.flush().expect("Flush failed");
                    writer.get_mut().sync_all().expect("Sync failed");
                    buffer.clear();
                }
            }
        });
        Ok(Self { tx })
    }

    pub fn append(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.tx.send(data.to_vec()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "Wal thread crashed")
        })?;
        Ok(())
    }
}

/*{
        let receiver_key = self
            .accounts
            .name_to_key
            .get(&task.receiver_name)
            .ok_or(TransactionError::AccountNotFound)?;

        let sender_bal = self.accounts.balances.get(&task.sender_pubkey).ok_or(TransactionError::AccountNotFound)?;

        if task.amount > *sender_bal {
            return Err(TransactionError::NotEnoughBalance);
        }

        let current_seq = self.accounts.sequences.get(&task.sender_pubkey).copied().unwrap_or(0);
        let max_future_gap = 50;
        // if task.sequence != current_seq + 1 {
        //     // You might need a new error variant: InvalidSequence
        //     return Err(TransactionError::IoError); 
        // }     
        if task.sequence > current_seq + max_future_gap {
            return Err(TransactionError::SequenceTooFar);
        }

        let user_queue = self.pending_queue.entry(task.sender_pubkey).or_insert_with(BTreeMap::new);

        if user_queue.len() >= 20 {
            return Err(TransactionError::QueueFull);
        }

        if task.sequence > current_seq + 1 {
            self.pending_queue.entry(task.sender_pubkey).or_default().insert(task.sequence, task);
            return Ok(());
        }

        let receiver_vk = VerifyingKey::from_bytes(receiver_key).map_err(|_| TransactionError::IoError)?;


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
        self.wal.append(&bytes).map_err(|_| TransactionError::IoError)?;
        self.apply_transaction(tx);
}*/