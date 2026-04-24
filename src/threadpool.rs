use std::{thread};
use crossbeam::channel::{bounded, Sender, Receiver};

pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<Sender<Job>>,
}

struct Worker {
    _id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

type Job = Box<dyn FnOnce() + Send + 'static>;

impl ThreadPool {
    pub fn new(size: usize) -> ThreadPool {
        assert!(size > 0);

        let (sender, receiver) = bounded(size);
        let mut workers = Vec::with_capacity(size);

        for id in 0..size {
            workers.push(Worker::new(id, receiver.clone()));
        }
        ThreadPool { workers, sender: Some(sender) }


    }

    pub fn execute<F>(&self, f: F) where F: FnOnce() + Send + 'static, {
        let job = Box::new(f);
        if let Some(ref s) = self.sender {
            s.send(job).expect("Channel closed");
        }
    }

}

impl Worker {
    fn new(id: usize, receiver: Receiver<Job>) -> Worker {
        let thread = thread::spawn(move || {

            while let Ok(job) = receiver.recv() {
                println!("Worker {id} got work.");
                job()
            }
            println!("Worker {id} shutting down.");

        });
        
        Worker { _id: id, thread: Some(thread) }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        drop(self.sender.take());

        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                thread.join().expect("Thread failed to join");
            }
        }
    }
}
