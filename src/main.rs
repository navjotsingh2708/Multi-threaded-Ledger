use std::{io::{self, Write}, sync::mpsc, thread};
use ed25519_dalek::ed25519::signature::SignerMut;
use multi_threaded_ledger::Ledger;
mod crypto;

fn read_cli(command: &str) -> String {
    print!("{command}");
    io::stdout().flush().unwrap();
    let mut c = String::new();
    io::stdin().read_line(&mut c).unwrap();
    let c = c.trim().to_string();
    c
}

fn main() {
    let (tx, rx) = mpsc::channel();
    let path = "ledger.log";
    let ledger = Ledger::new(path).expect("Failed to open Ledger WAL file");
    let handle = thread::spawn(move || {
        ledger.run(rx);
    });
    loop {
        let input = read_cli("> ");
        match input.as_str() {
            "1" | "add" => {
                let sender = read_cli("Sender - ");
                let amount = read_cli("Amount - ");
                let receiver = read_cli("Receiver - ");
                let timestamp = chrono::Utc::now().timestamp();
                let amount: u64 = match amount.parse::<u64>() {
                    Ok(v) => v,
                    Err(_) => {
                        println!("Invalid amount");
                        continue;
                    }
                };

                let mut message = Vec::new();
                message.extend_from_slice(sender.as_bytes());
                message.extend_from_slice(receiver.as_bytes());
                message.extend_from_slice(&amount.to_le_bytes());
                message.extend_from_slice(&timestamp.to_le_bytes());



                let mut key = crypto::load_key(&sender).expect("Failed to load wallet");

                let signature = key.sign(&message);
                
                let (resp_tx, resp_rx) = mpsc::channel();

                tx.send(multi_threaded_ledger::LedgerRequest::AddTransaction { sender, receiver, amount, timestamp, signature: signature, respond_to: resp_tx }).unwrap();

                match resp_rx.recv().unwrap() {
                    Ok(_) => {
                        println!("Transaction added.");
                },
                    Err(e) => println!("Error: {:?}", e),
                }
                
            }

            "2" | "list" => {
                let (resp_tx, resp_rx) = mpsc::channel();
                tx.send(multi_threaded_ledger::LedgerRequest::ListTransaction { respond_to: resp_tx }).unwrap();
                match resp_rx.recv() {
                   Ok(l) => println!("{:?}", l),
                   Err(e) => println!("{e}"), 
                }
            }

            "3" | "profile" => {
                let name = read_cli("Name - ");
                let balance = read_cli("balance - ");
                let balance:u64 = match balance.parse::<u64>() {
                    Ok(b) => b,
                    Err(_) => {
                        println!("Invalid balance");
                        continue;
                    }
                };

                let key = crypto::setup(&name).expect("Setup failed");

                let (resp_tx, resp_rx) = mpsc::channel();

                tx.send(multi_threaded_ledger::LedgerRequest::Profile { name, balance, key, respond_to: resp_tx }).unwrap();

                match resp_rx.recv().unwrap() {
                    Ok(_) => println!("Account created."),
                    Err(e) => println!("Error: {:?}", e),
                }
            }

            "4" | "balance" => {
                let name = read_cli("Name - ");
                let (resp_tx, resp_rx) = mpsc::channel();
                tx.send(multi_threaded_ledger::LedgerRequest::GetBalance { name, respond_to: resp_tx }).unwrap();
                match resp_rx.recv().unwrap() {
                    Ok(b) => println!("Balance - {b}"),
                    Err(e) => println!("Error: {:?}", e),
                }
            }

            "5" | "shutdown" => {
                tx.send(multi_threaded_ledger::LedgerRequest::ShutDown).unwrap();
                break;

            }
            _ => println!("Unknown command.")
        }
    }
    handle.join().unwrap();
    println!("Ledger shut down cleanly.");
}