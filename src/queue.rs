use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Relaxed, Acquire, AcqRel};
use std::os::unix::io::{AsRawFd, RawFd};
use std::io;

use concurrent_queue::{ConcurrentQueue, PopError, PushError};

use crate::waker::Waker;
use crate::epoll::{Ready, Source, Epoll, Token, EpollOpt};

pub struct Queue<T> {
    inner: Arc<Inner<T>>
}

struct Inner<T> {
    queue: ConcurrentQueue<T>,
    pending: AtomicUsize,
    waker: Waker
}

impl <T: Send> Queue<T> {
    pub fn bounded(cap: usize) -> io::Result<Queue<T>> {
        Ok(Queue {
            inner: Arc::new(Inner {
                queue: ConcurrentQueue::bounded(cap),
                pending: AtomicUsize::new(0),
                waker: Waker::new()?
            })
        })
    }

    pub fn unbounded() -> io::Result<Queue<T>> {
        Ok(Queue {
            inner: Arc::new(Inner {
                queue: ConcurrentQueue::unbounded(),
                pending: AtomicUsize::new(0),
                waker: Waker::new()?
            })
        })
    }

    fn inc(&self) -> io::Result<()> {
        let cnt = self.inner.pending.fetch_add(1, Acquire);

        if 0 == cnt {
            self.inner.waker.set_readiness(Ready::readable())?;
        }
        Ok(())
    }

    fn dec(&self) -> io::Result<()> {
        let first = self.inner.pending.load(Acquire);

        if first == 1 {
            self.inner.waker.set_readiness(Ready::empty())?;
        }

        let second = self.inner.pending.fetch_sub(1, AcqRel);

        if first == 1 && second > 1 {
            self.inner.waker.set_readiness(Ready::readable())?;
        }

        Ok(())
    }

    pub fn push(&self, value: T) -> Result<(), PushError<T>>{
        self.inner.queue.push(value).map(|_| { let _ = self.inc(); })
    }

    pub fn pop(&self) -> Result<T, PopError> {
        self.inner.queue.pop().map(|res| {let _ = self.dec(); res})
    }

    pub fn pending(&self) -> usize {
        self.inner.pending.load(Relaxed)
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
        self.inner.waker.as_raw_fd()
    }
}

impl<T: Send> Source for Queue<T> {
    fn add(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        self.inner.waker.add(epoll, token, interest, opts)?;

        if self.inner.pending.load(Relaxed) > 0 {
            self.inner.waker.set_readiness(Ready::readable())?;
        }

        Ok(())
    }

    fn modify(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        self.inner.waker.modify(epoll, token, interest, opts)
    }

    fn delete(&self, epoll: &Epoll) -> io::Result<()> {
        self.inner.waker.delete(epoll)
    }
}
