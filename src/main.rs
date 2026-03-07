use std::{io::{self, Write}, sync::mpsc, thread};
use multi_threaded_ledger::Ledger;

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
                
                let (resp_tx, resp_rx) = mpsc::channel();

                tx.send(multi_threaded_ledger::LedgerRequest::AddTransaction { sender, receiver, amount, timestamp, respond_to: resp_tx }).unwrap();

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
                let (resp_tx, resp_rx) = mpsc::channel();
                tx.send(multi_threaded_ledger::LedgerRequest::Profile { name, balance, respond_to: resp_tx }).unwrap();
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