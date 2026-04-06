use std::time::Instant;
use multi_threaded_ledger::{Ledger, validator::VerificationTask};
use ed25519_dalek::{SigningKey, Signer};
use crossbeam::channel::bounded;

fn main() {
    let num_tx = 100_000;
    let mut ledger = Ledger::new("stress.log").unwrap();
    
    // Setup keys and accounts
    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let sender_bytes = signing_key.verifying_key().to_bytes();
    ledger.set_test_account("alice".to_string(), sender_bytes, 2_000_000);
    ledger.set_test_account("bob".to_string(), [0u8; 32], 0);

    // 1. Pre-generate tasks (Measure ONLY the Ledger/Disk efficiency)
    println!("Pre-generating {} transactions...", num_tx);
    let mut tasks = Vec::with_capacity(num_tx);
    let (resp_tx, _resp_rx) = bounded(num_tx);

    for i in 1..=num_tx {
        let mut msg = Vec::new();
        msg.extend_from_slice(b"alice");
        msg.extend_from_slice(b"bob");
        msg.extend_from_slice(&10u64.to_le_bytes()); // amount
        msg.extend_from_slice(&123456789i64.to_le_bytes()); // ts
        msg.extend_from_slice(&(i as u64).to_le_bytes()); // sequence
        
        let sig = signing_key.sign(&msg);
        tasks.push(VerificationTask {
            sender_name: "alice".to_string(),
            receiver_name: "bob".to_string(),
            amount: 10,
            timestamp: 123456789,
            signature: sig,
            sequence: i as u64,
            sender_pubkey: sender_bytes,
            respond_to: resp_tx.clone(),
        });
    }

    // 2. The REAL Test
    println!("Starting Stress Test...");
    let start = Instant::now();

    for task in tasks {
        // This will block once the 10,000-buffer WAL channel is full!
        // That is when the disk speed starts pushing back on the CPU.
        ledger.add(task).unwrap();
    }

    // 3. The "Finish Line"
    // We don't stop the timer until the WAL thread is 100% done.
    // Note: In your current code, you'd need a way to wait for the WAL thread.
    // For this test, we drop the ledger to trigger the WAL's emergency flush.
    drop(ledger); 

    let duration = start.elapsed();
    let tps = num_tx as f64 / duration.as_secs_f64();

    println!("--- RESULTS ---");
    println!("Total Time: {:?}", duration);
    println!("Real Hardware TPS: {:.2} tx/s", tps);
}