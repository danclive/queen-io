use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Relaxed, Acquire, AcqRel};
use std::os::unix::io::{AsRawFd, RawFd};

use crate::plus::mpsc_queue;
use crate::sys::io;
use crate::{Awakener, Ready, Evented, Poll, Token, PollOpt};

pub struct Queue<T: Send> {
    inner: Arc<Inner<T>>
}

struct Inner<T> {
    queue: mpsc_queue::Queue<T>,
    pending: AtomicUsize,
    awakener: Awakener
}

impl <T: Send> Queue<T> {
    pub fn new() -> io::Result<Queue<T>> {
        Ok(Queue {
            inner: Arc::new(Inner {
                queue: mpsc_queue::Queue::new(),
                pending: AtomicUsize::new(0),
                awakener: Awakener::new()?
            })
        })
    }

    fn inc(&self) -> io::Result<()> {
        let cnt = self.inner.pending.fetch_add(1, Acquire);

        if 0 == cnt {
            self.inner.awakener.set_readiness(Ready::readable())?;
        }
        Ok(())
    }

    fn dec(&self) -> io::Result<()> {
        let first = self.inner.pending.load(Acquire);

        if first == 1 {
            self.inner.awakener.set_readiness(Ready::empty())?;
        }

        let second = self.inner.pending.fetch_sub(1, AcqRel);

        if first == 1 && second > 1 {
            self.inner.awakener.set_readiness(Ready::readable())?;
        }

        Ok(())
    }

    pub fn push(&self, value: T) {
        self.inner.queue.push(value);
        let _ = self.inc();
    }

    pub fn pop(&self) -> Option<T> {
        if let mpsc_queue::PopResult::Data(ret) = self.inner.queue.pop() {
            let _ = self.dec();
            return Some(ret)
        }

        None
    }
}

impl<T: Send> Clone for Queue<T> {
    fn clone(&self) -> Queue<T> {
        Queue {
            inner: self.inner.clone()
        }
    }
}

impl<T: Send> AsRawFd for Queue<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.awakener.as_raw_fd()
    }
}

impl<T: Send> Evented for Queue<T> {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.inner.awakener.register(poll, token, interest, opts)?;

        if self.inner.pending.load(Relaxed) > 0 {
            self.inner.awakener.set_readiness(Ready::readable())?;
        }

        Ok(())
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.inner.awakener.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.inner.awakener.deregister(poll)
    }
}
