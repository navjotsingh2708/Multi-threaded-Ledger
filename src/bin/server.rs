use std::{io::{ErrorKind, Read, Write}, net::{TcpListener, TcpStream}};
use multi_threaded_ledger::{ClientRequest, LedgerConfirm, LedgerRequest, ServerResponse, threadpool::ThreadPool, validator::VerificationTask};
use std::{thread};
use multi_threaded_ledger::{Ledger, TransactionError};
use crossbeam::channel::{unbounded, bounded,Sender};
use multi_threaded_ledger::validator::{WorkerPool};

fn main() {
    let listener = TcpListener::bind("0.0.0.0:7878").unwrap();
    let thread_pool = ThreadPool::new(4);
    let worker_pool = WorkerPool::new(8);
    let (verified_tx, verified_rx) = bounded(1024);
    let (tx, rx) = unbounded::<multi_threaded_ledger::LedgerRequest>();
    let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
    let path = "ledger.log";
    let ledger = Ledger::new(path, shutdown_tx.clone()).expect("Failed to open Ledger WAL file");
    let _handle = thread::spawn(move || {
        ledger.run(rx, verified_rx);
    });
    listener.set_nonblocking(true).expect("Cannot set non-blocking");
    loop {
        // Check if handle_connection sent the shutdown signal
        if let Ok(_) = shutdown_rx.try_recv() {
            break; // Exit the listener loop!
        }

        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).expect("Failed to set stream to blocking");
                // stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).expect("Failed to set read timeout");
                let pool_sender = worker_pool.sender.clone();
                let ledger_tx = verified_tx.clone();
                let ledger_req_tx = tx.clone();
                let s_tx = shutdown_tx.clone(); // Pass the signal sender
                
                thread_pool.execute(move || {
                    handle_connection(stream, pool_sender, ledger_tx, ledger_req_tx, s_tx);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No connection yet, sleep a tiny bit to save CPU
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
            Err(e) => eprintln!("Connection error: {e}"),
        }
    }
    println!("Shutting down.");
}

fn send_response(stream: &mut TcpStream, resp: &ServerResponse) {
    let bytes = bincode::serialize(resp).expect("Failed to serialize response");
    let len = (bytes.len() as u32).to_be_bytes();
    let _ = stream.write_all(&len);
    let _ = stream.write_all(&bytes);
}

fn handle_connection(mut stream: TcpStream, pool_sender: Sender<VerificationTask>, 
    ledger_tx: Sender<Result<VerificationTask, TransactionError>>, ledger_req_tx: Sender<LedgerRequest>, shutdown_tx: Sender<()>) {
        loop {
            
            let mut header_buffer = [0u8; 4];
            if let Err(e) = stream.read_exact(&mut header_buffer) {
                match e.kind() {
                    ErrorKind::TimedOut => println!("Header timeout: Closing idle connection."),
                    ErrorKind::UnexpectedEof => println!("Client disconnected normally."),
                    _ => eprintln!("Header read error: {e}"),
                }
                return;
            }
        
            let len = u32::from_be_bytes(header_buffer) as usize;

            if len > 1024 * 1024 { // 1MB limit for example
                eprintln!("Maliciously large body size: {len} bytes. Dropping client.");
                return;
            }

            let mut body_buffer = vec![0u8; len];
            if let Err(e) = stream.read_exact(&mut body_buffer) {
                match e.kind() {
                    ErrorKind::TimedOut => println!("Body timeout: Closing idle connection."),
                    ErrorKind::UnexpectedEof => println!("Client disconnected normally."),
                    _ => eprintln!("Body read error: {e}"),
                }
                return;
            }
        
            let req = match bincode::deserialize(&body_buffer) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Failed to deserialize bytes to ClientRequest: {e}");
                    send_response(&mut stream, &ServerResponse::Error(e.to_string()));
                    let _ = shutdown_tx.send(());
                    return;
                } 
            };
        
            match req {
                ClientRequest::Transfer(mut task) => {
                    let (resp_tx, resp_rx) = crossbeam::channel::bounded(1);
                    task.client_respond_to = Some(resp_tx);
                    task.respond_to = Some(ledger_tx.clone());
                    let sender_pubkey = task.sender_pubkey.clone();
                
                    pool_sender.send(task).expect("WorkerPool is down.");
                    let resp = match resp_rx.recv() {
                    Ok(Ok(LedgerConfirm::Committed)) => ServerResponse::Success,
                    Ok(Ok(LedgerConfirm::Queued)) => ServerResponse::Queued,
                    Ok(Err(e)) => ServerResponse::Error(e.to_string()),
                    Err(e) => ServerResponse::Error(e.to_string()),
                };
                send_response(&mut stream, &resp);
                println!("Received task from: {:?}", sender_pubkey);
                }
                ClientRequest::GetBalance { name } => {
                    let (resp_tx, resp_rx) = crossbeam::channel::bounded(1);
        
                    let ledger_req = LedgerRequest::GetBalance { name, respond_to: resp_tx };
                    ledger_req_tx.send(ledger_req).expect("Main thread(Ledger) is down.");
        
                    let resp = match resp_rx.recv() {
                        Ok(Ok(b))  => ServerResponse::Balance(b),
                        Ok(Err(e)) => ServerResponse::Error(e.to_string()),
                        Err(e)     => ServerResponse::Error(e.to_string()),
                    };
                    send_response(&mut stream, &resp);
                }
                ClientRequest::CreateProfile { name, balance, key } => {
                    let (resp_tx, resp_rx) = crossbeam::channel::bounded(1);
                    let ledger_req = LedgerRequest::Profile { name, key, balance, respond_to: resp_tx };
                    ledger_req_tx.send(ledger_req).expect("Main thread(Ledger) is down.");
        
                    let resp = match resp_rx.recv() {
                        Ok(Ok(_))  => ServerResponse::Success,
                        Ok(Err(e)) => ServerResponse::Error(e.to_string()),
                        Err(e)     => ServerResponse::Error(e.to_string()),
                    };
                    send_response(&mut stream, &resp);
                }
                ClientRequest::ShutDown => {
                    println!("!!! REMOTE SHUTDOWN INITIATED !!!");
                    send_response(&mut stream, &ServerResponse::Success);
                    let _ = shutdown_tx.send(());
                    return;
                }
            }
        }

}