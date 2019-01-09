use std::os::unix::io::{RawFd, AsRawFd, FromRawFd};
use std::mem;
use libc;

use super::io::{self, Io, Read, Write};
use super::cvt;
use crate::{Poll, Token, Ready, PollOpt, Evented};

pub const EFD_CLOEXEC: i32 = libc::EFD_CLOEXEC;
pub const EFD_NONBLOCK: i32 = libc::EFD_NONBLOCK;
pub const EFD_SEMAPHORE: i32 = libc::EFD_SEMAPHORE;

#[derive(Debug)]
pub struct EventFd {
	inner: Io
}

impl EventFd {
	/// Create an eventfd with initval: 0 and flags: EFD_CLOEXEC | EFD_NONBLOCK
	/// view: http://man7.org/linux/man-pages/man2/eventfd.2.html
	pub fn new() -> io::Result<EventFd> {
		let flags = EFD_CLOEXEC | EFD_NONBLOCK;
		EventFd::with_options(0, flags)
	}

	pub fn with_options(initval: u32, flags: i32) -> io::Result<EventFd> {
		let eventfd = unsafe { cvt(libc::eventfd(initval, flags))? };
		Ok(EventFd {
			inner: unsafe { Io::from_raw_fd(eventfd) }
		})
	}

	pub fn read(&self) -> io::Result<u64> {
		let mut buf = [0u8; 8];
		(&self.inner).read(&mut buf)?;
		let temp: u64 = unsafe { mem::transmute(buf) };
		return Ok(temp);
	}

	pub fn write(&self, val: u64) -> io::Result<()> {
		let buf: [u8; 8] = unsafe { mem::transmute(val) };
		(&self.inner).write(&buf)?;
		Ok(())
	}
}

impl AsRawFd for EventFd {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl Evented for EventFd {
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


#[cfg(test)]
mod test {
	use super::EventFd;

	#[test]
	fn write_and_read() {
		let eventfd = EventFd::new().unwrap();
		eventfd.write(123).unwrap();
		let count = eventfd.read().unwrap();
		assert_eq!(123, count);
	}

	#[test]
	fn write_block() {
		let eventfd = EventFd::new().unwrap();

		assert!(eventfd.write(0xfffffffffffffffe).is_ok());
		assert!(eventfd.write(0xfffffffffffffffe).is_err()); // Err(Os { code: 11, kind: WouldBlock, message: "Resource temporarily unavailable" })
	}
}
