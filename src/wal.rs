use std::io::Write;

use crate::Transaction;

pub struct Wal {
    file: std::fs::File,
}

impl Wal {
    pub fn new(path: &str) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self { file })
    }

    pub fn append(&mut self, tx: &Transaction) -> std::io::Result<()> {
        let serialize = serde_json::to_string(tx)?;
        writeln!(self.file, "{}", serialize)?;
        self.file.flush()?;
        Ok(())
    }
}