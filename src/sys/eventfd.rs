use std::os::unix::io::{RawFd, AsRawFd, FromRawFd, IntoRawFd};
use std::mem;
use libc;

use super::io::{self, Io, Read, Write};
use super::cvt;
use crate::{Epoll, Token, Ready, EpollOpt, Evented};

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
        (&self.inner).read_exact(&mut buf)?;
        let temp: u64 = unsafe { mem::transmute(buf) };
        Ok(temp)
    }

    pub fn write(&self, val: u64) -> io::Result<()> {
        let buf: [u8; 8] = unsafe { mem::transmute(val) };
        (&self.inner).write_all(&buf)?;
        Ok(())
    }
}

impl FromRawFd for EventFd {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        EventFd {
            inner: Io::from_raw_fd(fd)
        }
    }
}

impl IntoRawFd for EventFd {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}

impl AsRawFd for EventFd {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl Evented for EventFd {
    fn add(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        epoll.add(&self.as_raw_fd(), token, interest, opts)
    }

    fn modify(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        epoll.modify(&self.as_raw_fd(), token, interest, opts)
    }

    fn delete(&self, epoll: &Epoll) -> io::Result<()> {
        epoll.delete(&self.as_raw_fd())
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
