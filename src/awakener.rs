use std::sync::Arc;
use std::os::unix::io::{RawFd, AsRawFd, FromRawFd};

use crate::sys::eventfd::EventFd;
use crate::sys::io;
use crate::{Epoll, Token, Ready, EpollOpt, Evented};

#[derive(Debug, Clone)]
pub struct Awakener {
    inner: Arc<EventFd>
}

impl Awakener {
	pub fn new() -> io::Result<Awakener> {
		let eventfd = EventFd::new()?;

		Ok(Awakener {
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

impl FromRawFd for Awakener {
	unsafe fn from_raw_fd(fd: RawFd) -> Self {
		Awakener {
			inner: Arc::new(EventFd::from_raw_fd(fd))
		}
	}
}

impl AsRawFd for Awakener {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl Evented for Awakener {
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
