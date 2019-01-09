use std::collections::VecDeque;
use std::sync::{Arc, Mutex, Condvar};

#[derive(Clone, Debug)]
pub struct BlockQueue<T> where T: Send {
    inner: Arc<BlockQueueInner<T>>
}

#[derive(Debug)]
struct BlockQueueInner<T> {
    queue: Mutex<VecDeque<T>>,
    condvar: Condvar
}

impl<T> BlockQueue<T> where T: Send {
    pub fn with_capacity(capacity: usize) -> BlockQueue<T> {
        BlockQueue {
            inner: Arc::new(BlockQueueInner {
                queue: Mutex::new(VecDeque::with_capacity(capacity)),
                condvar: Condvar::new()
            })
        }
    }

    pub fn push(&self, value: T) {
        let mut queue = self.inner.queue.lock().unwrap();
        queue.push_back(value);

        self.inner.condvar.notify_one();
    }

    pub fn pop(&self) -> T {
        let mut queue = self.inner.queue.lock().unwrap();

        loop {
            if let Some(elem) = queue.pop_front() {
                return elem;
            }

            queue = self.inner.condvar.wait(queue).unwrap();
        }
    }

    pub fn try_pop(&self) -> Option<T> {
        let mut queue = self.inner.queue.lock().unwrap();
        queue.pop_front()
    }
}
