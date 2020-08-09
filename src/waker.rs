use std::sync::Arc;
use std::os::unix::io::{RawFd, AsRawFd, FromRawFd};
use std::io;

use crate::sys::eventfd::EventFd;
use crate::epoll::{Ready, Source, Epoll, Token, EpollOpt};

#[derive(Debug, Clone)]
pub struct Waker {
    inner: Arc<EventFd>
}

impl Waker {
    pub fn new() -> io::Result<Waker> {
        let eventfd = EventFd::new()?;

        Ok(Waker {
            inner: Arc::new(eventfd)
        })
    }

    pub fn wakeup(&self) -> io::Result<()> {
        match self.inner.write(1) {
            Ok(_) => Ok(()),
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }

    pub fn finish(&self) -> io::Result<()> {
        match self.inner.read() {
            Ok(_) => Ok(()),
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }

    pub fn set_readiness(&self, ready: Ready) -> io::Result<()> {
        if ready == Ready::readable() || ready == Ready::writable() {
            self.wakeup()?;
        }

        if ready == Ready::empty() {
            self.finish()?;
        }

        Ok(())
    }
}

impl FromRawFd for Waker {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Waker {
            inner: Arc::new(EventFd::from_raw_fd(fd))
        }
    }
}

impl AsRawFd for Waker {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl Source for Waker {
    fn add(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        self.inner.add(epoll, token, interest, opts)
    }

    fn modify(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        self.inner.modify(epoll, token, interest, opts)
    }

    fn delete(&self, epoll: &Epoll) -> io::Result<()> {
        self.inner.delete(epoll)
    }
}
