use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::thread::JoinHandle;

type Job = Box<dyn FnOnce() + Send + 'static>;

#[derive(Debug)]
pub struct ThreadPool {
    workers: Vec<Worker>,
    tx: Option<mpsc::Sender<Job>>,
}

impl ThreadPool {
    pub fn new(size: usize) -> Result<ThreadPool, String> {
        if size < 1 {
            return Err("size must be greater than 0".into());
        }
        let (tx, rx) = mpsc::channel();
        //thread safety Multiple Ownership
        let arx = Arc::new(Mutex::new(rx));
        let mut workers = Vec::with_capacity(size);
        for i in 0..size {
            workers.push(Worker::new(i, Arc::clone(&arx)));
        }
        Ok(ThreadPool { workers, tx: Some(tx) })
    }
    pub fn execute<T>(&self, task: T) -> Result<(), mpsc::SendError<Job>>
    where
        T: FnOnce() + Send + 'static,
    {
        let task = Box::new(task);
        self.tx.as_ref().unwrap().send(task)?;
        Ok(())
    }
}


impl Drop for ThreadPool {
    /// invoke when value was freed.
    fn drop(&mut self) {
        drop(self.tx.take());
        for worker in self.workers.drain(..) {
            println!("Shutting down worker {}", worker.id);
            //join need JoinHandler owns, need move worker to here,
            // or change handler type to Option, take can move owns and set worker.handler to be None
            //   just like self.tx
            worker.handler.join().unwrap();
        }
    }
}


#[derive(Debug)]
pub struct Worker {
    id: usize,
    handler: JoinHandle<()>,
}

impl Worker {
    fn new(id: usize, arx: Arc<Mutex<mpsc::Receiver<Job>>>) -> Worker {
        let handler = thread::spawn(move || {
            loop {
                let res = arx.lock().unwrap().recv();
                if let Some(err) = res.as_ref().err() {
                    println!("Worker[{id}] got an error: {err}");
                    break;
                }else{
                    let job = res.unwrap();
                    println!("Worker[{id}] got a job; executing.");
                    job();
                }
            }
        });
        Worker { id, handler }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{mpsc};
    use std::thread;
    use std::time::Duration;
    use crate::thread_pool::ThreadPool;

    #[test]
    fn test_thread_pool() {
        let pool = ThreadPool::new(2).unwrap();
        let (tx, rx) = mpsc::channel();
        
        let tx1 = tx.clone();
        pool.execute(move || {
            thread::sleep(Duration::from_secs(5));
            println!("task 1");
            tx1.send(1).unwrap();
        }).unwrap();

        let tx2 = tx.clone();
        pool.execute(move || {
            println!("task 2");
            tx2.send(2).unwrap();
        }).unwrap();

        let tx3 = tx.clone();
        pool.execute(move || {
            thread::sleep(Duration::from_secs(3));
            println!("task 3");
            tx3.send(3).unwrap();
        }).unwrap();

        pool.execute(move || {
            println!("task 4");
            tx.send(4).unwrap();
        }).unwrap();
        let mut task_id_order = vec![2,3,4,1];
        for task_id in rx {
            println!("log: task[{}] invoked", task_id);
            assert_eq!(task_id, task_id_order.remove(0));
        }
    }

    #[test]
    fn test_size() {
        let r = ThreadPool::new(0);
        assert!(r.is_err());
        assert_eq!(r.unwrap_err(), "size must be greater than 0");
    }
}
