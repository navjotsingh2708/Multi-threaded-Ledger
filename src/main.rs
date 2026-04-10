use std::{error::Error, io::{self, Write}, thread};
use ed25519_dalek::{ed25519::signature::SignerMut};
use multi_threaded_ledger::{Ledger};
use multi_threaded_ledger::TransactionError;
use multi_threaded_ledger::crypto::{load_private_key, load_public_key, setup};
use crossbeam::channel::{unbounded};
use multi_threaded_ledger::validator::{VerificationTask, WorkerPool};

fn read_cli(command: &str) -> Result<String, Box<dyn Error>> {
    print!("{command}");
    io::stdout().flush()?;
    let mut c = String::new();
    io::stdin().read_line(&mut c)?;
    let c = c.trim().to_string();
    Ok(c)
}

fn main() -> Result<(), Box<dyn Error>> {
    let pool = WorkerPool::new(8);
    let (verified_tx, verified_rx) = crossbeam::channel::bounded(1024);
    let (tx, rx) = unbounded::<multi_threaded_ledger::LedgerRequest>();
    let path = "ledger.log";
    let ledger = Ledger::new(path).expect("Failed to open Ledger WAL file");
    let handle = thread::spawn(move || {
        ledger.run(rx, verified_rx);
    });
    loop {
        let input = read_cli("> ")?;
        match input.as_str() {
            "1" | "add" => {
                let sender = read_cli("Sender - ")?;
                let amount = read_cli("Amount - ")?;
                let receiver = read_cli("Receiver - ")?;
                let sequence = read_cli("Sequence - ")?;
                let timestamp = chrono::Utc::now().timestamp();
                let amount: u64 = match amount.parse::<u64>() {
                    Ok(v) => v,
                    Err(_) => {
                        println!("\nInvalid amount");
                        continue;
                    }
                };
                let sequence: u64 = match sequence.parse::<u64>() {
                    Ok(v) => v,
                    Err(_) => {
                        println!("\nInvalid amount");
                        continue;
                    }
                };
                
                let (seq_tx, seq_rx) = crossbeam::channel::bounded::<Result<u64, TransactionError>>(1);
                tx.send(multi_threaded_ledger::LedgerRequest::GetSequence { account: sender.clone(), respond_to: seq_tx })?;

                let next_sq = seq_rx.recv()?.map_err(|_| TransactionError::AccountNotFound)?;
                println!("\nNext valid sequence for {}: {}", sender, next_sq);
                
                let mut message = Vec::new();

                message.extend_from_slice(sender.as_bytes());
                message.extend_from_slice(receiver.as_bytes());
                message.extend_from_slice(&amount.to_le_bytes());
                message.extend_from_slice(&timestamp.to_le_bytes());
                message.extend_from_slice(&sequence.to_le_bytes());



                let mut key = load_private_key(&sender).expect("Failed to load wallet");

                let signature = key.sign(&message);
                let sender_pubkey = load_public_key(&sender)?;
                let sender_pubkey = sender_pubkey.to_bytes();

                let task = VerificationTask {
                    sender_name: sender.clone(),
                    receiver_name: receiver,
                    amount,
                    timestamp,
                    signature,
                    sequence,
                    sender_pubkey,
                    respond_to: Some(verified_tx.clone()),
                };

                pool.sender.send(task).expect("\nWorket pool is down");
                println!("\nTransaction submitted for verification.");
                
            }

            "2" | "list" => {
                let input = read_cli("Enter Sender's name (press enter for all): ")?;
                let sender = if input.trim().is_empty() {
                    None
                } else {
                    Some(input.trim().to_string())
                };

                let (resp_tx, resp_rx) = crossbeam::channel::bounded(1);
                tx.send(multi_threaded_ledger::LedgerRequest::ListTransaction { sender, respond_to: resp_tx })?;
                match resp_rx.recv().map_err(|_| "\nChannel error".to_string()).and_then(|res| res.map_err(|e| e.to_string())) {
                   Ok(transactions) => {
                        println!("{:-<50}", "");
                        println!("{:<10} | {:<10} | {:<8} | {:<4}", "Sender", "Receiver", "Amount", "Seq");
                        println!("{:-<50}", "");
                        for tx in transactions {
                            // We use the first 8 bytes of the key as a "short ID"
                            let s_id = hex::encode(&tx.sender.to_bytes()[..4]);
                            let r_id = hex::encode(&tx.receiver.to_bytes()[..4]);
                            println!("{:<10} | {:<10} | {:<8} | {:<4}", s_id, r_id, tx.amount, tx.sequence);
                        }
                        println!("{:-<50}", "");
                    },
                   Err(e) => println!("{e}"), 
                }
            }

            "3" | "profile" => {
                let name = read_cli("Name - ")?;
                let balance = read_cli("balance - ")?;
                let balance:u64 = match balance.parse::<u64>() {
                    Ok(b) => b,
                    Err(_) => {
                        println!("\nInvalid balance");
                        continue;
                    }
                };

                let key = setup(&name).expect("\nSetup failed");

                let (resp_tx, resp_rx) = crossbeam::channel::bounded(1);

                tx.send(multi_threaded_ledger::LedgerRequest::Profile { name, balance, key: key.to_bytes(), respond_to: resp_tx })?;

                match resp_rx.recv()? {
                    Ok(_) => println!("\nAccount created."),
                    Err(e) => println!("\nError: {:?}", e),
                }
            }

            "4" | "balance" => {
                let name = read_cli("Name - ")?;
                let (resp_tx, resp_rx) = crossbeam::channel::bounded(1);
                tx.send(multi_threaded_ledger::LedgerRequest::GetBalance { name, respond_to: resp_tx })?;
                match resp_rx.recv()? {
                    Ok(b) => println!("Balance - {b}"),
                    Err(e) => println!("\nError: {:?}", e),
                }
            }

            "5" | "shutdown" => {
                tx.send(multi_threaded_ledger::LedgerRequest::ShutDown)?;
                break;

            }
            _ => println!("\nUnknown command.")
        }
    }
    handle.join().map_err(|e| {
        if let Some(msg) = e.downcast_ref::<&str>() {
            format!("Thread panicked: {msg}")
        } else if let Some(msg) = e.downcast_ref::<String>() {
            format!("\nThread panicked: {msg}")
        } else {
            "\nThread panicked with an unknown error".to_string()
        }
    })?;
    println!("Ledger shut down cleanly.");
    Ok(())
}