use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Relaxed, Acquire, AcqRel};
use std::os::unix::io::{AsRawFd, RawFd};
use std::io;

use crate::plus::spsc_queue;
use crate::Awakener;
use crate::epoll::{Ready, Evented, Epoll, Token, EpollOpt};

pub struct Queue<T: Send> {
    inner: Arc<Inner<T>>
}

struct Inner<T> {
    queue: spsc_queue::Queue<T>,
    pending: AtomicUsize,
    awakener: Awakener
}

impl <T: Send> Queue<T> {
    pub fn with_cache(bound: usize) -> io::Result<Queue<T>> {
        Ok(Queue {
            inner: Arc::new(Inner {
                queue: unsafe { spsc_queue::Queue::with_additions(bound, (), ()) },
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
        self.inner.queue.pop().and_then(|res| {let _ = self.dec(); Some(res)})
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
        self.inner.awakener.as_raw_fd()
    }
}

impl<T: Send> Evented for Queue<T> {
    fn add(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        self.inner.awakener.add(epoll, token, interest, opts)?;

        if self.inner.pending.load(Relaxed) > 0 {
            self.inner.awakener.set_readiness(Ready::readable())?;
        }

        Ok(())
    }

    fn modify(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        self.inner.awakener.modify(epoll, token, interest, opts)
    }

    fn delete(&self, epoll: &Epoll) -> io::Result<()> {
        self.inner.awakener.delete(epoll)
    }
}
