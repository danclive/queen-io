use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed};
use std::sync::Arc;

use crate::epoll::{Epoll, EpollOpt, Ready, Source, Token};
use crate::waker::Waker;

pub use concurrent_queue::{ConcurrentQueue, PopError, PushError};

pub struct Queue<T> {
    inner: Arc<QueueInner<T>>,
}

struct QueueInner<T> {
    queue: ConcurrentQueue<T>,
    pending: AtomicUsize,
    waker: Waker,
}

impl<T: Send> Queue<T> {
    pub fn bounded(cap: usize) -> io::Result<Queue<T>> {
        Ok(Queue {
            inner: Arc::new(QueueInner {
                queue: ConcurrentQueue::bounded(cap),
                pending: AtomicUsize::new(0),
                waker: Waker::new()?,
            }),
        })
    }

    pub fn unbounded() -> io::Result<Queue<T>> {
        Ok(Queue {
            inner: Arc::new(QueueInner {
                queue: ConcurrentQueue::unbounded(),
                pending: AtomicUsize::new(0),
                waker: Waker::new()?,
            }),
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

    pub fn push(&self, value: T) -> Result<(), PushError<T>> {
        self.inner.queue.push(value).map(|_| {
            let _ = self.inc();
        })
    }

    pub fn pop(&self) -> Result<T, PopError> {
        self.inner.queue.pop().inspect(|_res| {
            let _ = self.dec();
        })
    }

    pub fn pending(&self) -> usize {
        self.inner.pending.load(Relaxed)
    }

    pub fn is_empty(&self) -> bool {
        self.inner.queue.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.inner.queue.is_full()
    }

    pub fn len(&self) -> usize {
        self.inner.queue.len()
    }

    pub fn capacity(&self) -> Option<usize> {
        self.inner.queue.capacity()
    }

    pub fn close(&self) -> bool {
        self.inner.queue.close()
    }

    pub fn is_closed(&self) -> bool {
        self.inner.queue.is_closed()
    }

    pub fn wake(&self) -> io::Result<()> {
        self.inner.waker.set_readiness(Ready::readable())
    }
}

impl<T: Send> Clone for Queue<T> {
    fn clone(&self) -> Queue<T> {
        Queue {
            inner: self.inner.clone(),
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

    fn modify(
        &self,
        epoll: &Epoll,
        token: Token,
        interest: Ready,
        opts: EpollOpt,
    ) -> io::Result<()> {
        self.inner.waker.modify(epoll, token, interest, opts)
    }

    fn delete(&self, epoll: &Epoll) -> io::Result<()> {
        self.inner.waker.delete(epoll)
    }
}
