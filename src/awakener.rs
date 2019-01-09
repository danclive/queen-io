use std::sync::Arc;

use crate::sys::eventfd::EventFd;
use crate::sys::io;
use crate::{Poll, Token, Ready, PollOpt, Evented};

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

impl Evented for Awakener {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.inner.register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.inner.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.inner.deregister(poll)
    }
}
