use std::io::{Read, Write};
use std::net::TcpStream;
use std::{error::Error};
use ed25519_dalek::{ed25519::signature::SignerMut};
use multi_threaded_ledger::{ClientRequest, ServerResponse};
use multi_threaded_ledger::crypto::{load_private_key, load_public_key, setup};
use multi_threaded_ledger::validator::{VerificationTask};

fn read_cli(prompt: &str) -> Result<String, Box<dyn Error>> {
    loop {
        print!("{prompt}");
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        
        let trimmed = input.trim().to_owned();
        if !trimmed.is_empty() {
            return Ok(trimmed); 
        }
    }
}

fn read_response(stream: &mut TcpStream) -> Result<ServerResponse, Box<dyn Error>> {
    let mut header = [0u8; 4];
    stream.read_exact(&mut header)?;
    let len = u32::from_be_bytes(header) as usize;
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body)?;
    Ok(bincode::deserialize(&body)?)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect("127.0.0.1:7878")?;
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
                    respond_to: None,
                    client_respond_to: None,
                };
                let req = ClientRequest::Transfer(task);
                let bytes = bincode::serialize(&req)?;
                let len = (bytes.len() as u32).to_be_bytes();
                stream.write_all(&len)?;
                stream.write_all(&bytes)?;
                println!("Transaction sent...");
                match read_response(&mut stream)? {
                    ServerResponse::Success      => println!("Success"),
                    ServerResponse::Queued       => println!("Transaction queued"),
                    // ServerResponse::Balance(b)   => println!("Balance - {b}"),
                    ServerResponse::Error(e)     => println!("Error - {e}"),
                    _ => println!("Unexpected Response"),
                }
        }

            "2" | "profile" => {
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

                let req = ClientRequest::CreateProfile { name, balance, key: key.to_bytes() };

                let bytes = bincode::serialize(&req)?;
                let len = (bytes.len() as u32).to_be_bytes();
                stream.write_all(&len)?;
                stream.write_all(&bytes)?;
                
                match read_response(&mut stream)? {
                    ServerResponse::Success => println!("Account created succesfully"),
                    ServerResponse::Error(e) => println!("Error - {e}"),
                    _ => println!("Unexpected response"),
                }
            }

            "3" | "balance" => {
                let name = read_cli("Name - ")?;
                let req = ClientRequest::GetBalance { name };

                let bytes = bincode::serialize(&req)?;
                let len = (bytes.len() as u32).to_be_bytes();
                stream.write_all(&len)?;
                stream.write_all(&bytes)?;

                match read_response(&mut stream)? {
                    ServerResponse::Balance(b) => println!("Balance - {b}"),
                    ServerResponse::Error(e) => println!("Error - {e}"),
                    _ => println!("Unexpected response"),
                }              
            }
            "5" | "quit" => {
                let req = ClientRequest::ShutDown;
                let bytes = bincode::serialize(&req)?;
                let len = (bytes.len() as u32).to_be_bytes();
                stream.write_all(&len)?;
                stream.write_all(&bytes)?;
                println!("Shutdown command sent. Exiting client.");
                break; // Exit the client loop
            }

            _ => {
                println!("Unexpected input! (press 5 or quit to exit)");
            }
        }
    }
    Ok(())
}