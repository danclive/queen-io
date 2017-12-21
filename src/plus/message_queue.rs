use std::collections::VecDeque;
use std::sync::{Arc, Mutex, Condvar};

use sys::io;
use {Registration, Ready, Evented, Poll, Token, PollOpt};

pub struct MessagesQueue<T> where T: Send {
    queue: Mutex<VecDeque<T>>,
    condvar: Condvar,
    registration: Registration
}

impl<T> MessagesQueue<T> where T: Send {
    pub fn with_capacity(capacity: usize) -> io::Result<Arc<MessagesQueue<T>>> {
        Ok(Arc::new(MessagesQueue {
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
            condvar: Condvar::new(),
            registration: Registration::new()?
        }))
    }

    pub fn push(&self, value: T) -> io::Result<()> {
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(value);

        self.condvar.notify_one();

        if queue.len() == 1 {
            self.registration.set_readiness(Ready::readable())?;
        }

        Ok(())
    }

    pub fn pop(&self) -> T {
        let mut queue = self.queue.lock().unwrap();

        loop {
            if let Some(elem) = queue.pop_front() {
                return elem;
            }

            queue = self.condvar.wait(queue).unwrap();
        }
    }

    pub fn try_pop(&self) -> io::Result<Option<T>> {
        let mut queue = self.queue.lock().unwrap();

        if queue.len() <= 1 {
            self.registration.set_readiness(Ready::empty())?;
        } else {
            self.registration.set_readiness(Ready::readable())?;
        }

        Ok(queue.pop_front())
    }
}

impl<T> Evented for MessagesQueue<T> where T: Send {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.registration.register(poll, token, interest, opts)?;

        let queue = self.queue.lock().unwrap();
        if queue.len() > 0 {
            self.registration.set_readiness(Ready::readable())?;
        }

        Ok(())
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.registration.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.registration.deregister(poll)
    }
}