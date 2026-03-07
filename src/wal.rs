use std::io::{Write, BufWriter};
use std::fs::{File, OpenOptions};

use crate::Transaction;

pub struct Wal {
    writer: BufWriter<File>,
}

impl Wal {
    pub fn new(path: &str) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self { writer: BufWriter::new(file) })
    }

    pub fn append(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()?;
        Ok(())
    }
}