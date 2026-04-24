use std::io::{BufWriter, Error, Write};
use std::thread;
use crossbeam::channel::{Sender, bounded, tick};
use crossbeam::select;

pub struct Wal {
    // writer: BufWriter<File>,
    tx: Sender<Message>
}

enum Message {
    Writer {
        data: Vec<u8>,
        confirm: Sender<Result<(), String>>,
    }
}

impl Wal {
    pub fn new(path: &str) -> std::io::Result<Self> {
        // Ok(Self { writer: BufWriter::new(file) })
        let (tx, rx) = bounded::<>(10000);
        let file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;

        thread::spawn(move || {
            let mut writer = BufWriter::new(file);
            let ticker = tick(std::time::Duration::from_millis(10));
            let mut data_buffer: Vec<Vec<u8>> = Vec::with_capacity(100);
            let mut confirm_buffer = Vec::with_capacity(100);

            loop {
                let mut should_flush = false;
                select! {
                    recv(rx) -> msg => {
                        match msg {
                            Ok(Message::Writer { data, confirm }) => {
                                data_buffer.push(data);
                                confirm_buffer.push(confirm);
                                if data_buffer.len() > 100 {should_flush = true}
                            },
                            Err(_) => {
                                for entry in &data_buffer {
                                    writer.write_all(entry).expect("Disk write failed");
                                }
                                writer.flush().expect("Flush failed");
                                writer.get_mut().sync_all().expect("Sync failed");
                                data_buffer.clear();
                                break;
                            },
                        };
                    }

                    recv(ticker) -> _ => {
                        if !data_buffer.is_empty() {
                            should_flush = true;
                        }
                    }
                }
                if should_flush == true {
                    let result: Result<(), Error> = (|| {
                        for entry in &data_buffer {
                            writer.write_all(entry)?;
                        }
                        writer.flush()?;
                        writer.get_mut().sync_all()?;
                        Ok(())
                    })();

                    match result {
                        Ok(_) => {
                            for tx in confirm_buffer.drain(..) {
                                let _ = tx.send(Ok(()));
                            }
                        },
                        Err(e) => {
                            for tx in confirm_buffer.drain(..) {
                                let _ = tx.send(Err(e.to_string()));
                            }
                        }
                    }
                    data_buffer.clear();
                }
            }
        });
        Ok(Self { tx })
    }

    pub fn append(&mut self, data: Vec<u8>) -> Result<(), std::io::Error> {
        let (tx, rx) = bounded(1);
        self.tx.send(Message::Writer { data, confirm: tx }).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "Wal thread crashed")
        })?;

        let _ = rx.recv().map_err(|_| Error::new(std::io::ErrorKind::BrokenPipe, "WAL down"))?;
        Ok(())
    }
}