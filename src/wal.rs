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