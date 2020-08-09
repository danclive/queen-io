use std::path::Path;
use std::io::{Read, Write};
use std::time::Duration;
use std::net::Shutdown;
use std::os::unix::net::{self, SocketAddr};
use std::os::unix::io::{RawFd, FromRawFd, IntoRawFd, AsRawFd};
use std::io;

use crate::epoll::{SelectorId, Ready, Source, Epoll, Token, EpollOpt};

#[derive(Debug)]
pub struct UnixStream {
    inner: net::UnixStream,
    selector_id: SelectorId
}

#[derive(Debug)]
pub struct UnixListener {
    inner: net::UnixListener,
    selector_id: SelectorId
}

impl UnixStream {
    pub fn connect<P: AsRef<Path>>(path: P) -> io::Result<UnixStream> {
        let stream = net::UnixStream::connect(path)?;

        Ok(UnixStream::new(stream)?)
    }

    pub fn new(stream: net::UnixStream) -> io::Result<UnixStream> {
        stream.set_nonblocking(true)?;

        Ok(UnixStream {
            inner: stream,
            selector_id: SelectorId::new()
        })
    }

    pub fn pair() -> io::Result<(UnixStream, UnixStream)> {
        let (stream1, stream2) = net::UnixStream::pair()?;

        Ok((UnixStream::new(stream1)?, UnixStream::new(stream2)?))
    }

    pub fn try_clone(&self) -> io::Result<UnixStream> {
        self.inner.try_clone().map(|s| {
            UnixStream {
                inner: s,
                selector_id: self.selector_id.clone()
            }
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.inner.set_read_timeout(timeout)
    }

    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.inner.set_write_timeout(timeout)
    }

    pub fn read_timeout(&self) -> io::Result<Option<Duration>> {
        self.inner.read_timeout()
    }

    pub fn write_timeout(&self) -> io::Result<Option<Duration>> {
        self.inner.write_timeout()
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.inner.set_nonblocking(nonblocking)
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.inner.take_error()
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.inner.shutdown(how)
    }
}

impl Read for UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<'a> Read for &'a UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.inner).read(buf)
    }
}

impl Write for UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<'a> Write for &'a UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.inner).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.inner).flush()
    }
}

impl Source for UnixStream {
    fn add(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        self.selector_id.associate_selector(epoll)?;
        epoll.add(&self.as_raw_fd(), token, interest, opts)
    }

    fn modify(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        epoll.modify(&self.as_raw_fd(), token, interest, opts)
    }

    fn delete(&self, epoll: &Epoll) -> io::Result<()> {
        epoll.delete(&self.as_raw_fd())
    }
}

impl FromRawFd for UnixStream {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixStream {
        UnixStream {
            inner: net::UnixStream::from_raw_fd(fd),
            selector_id: SelectorId::new(),
        }
    }
}

impl IntoRawFd for UnixStream {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}

impl AsRawFd for UnixStream {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl UnixListener {
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<UnixListener> {
        let listener = net::UnixListener::bind(path)?;

        Ok(UnixListener::new(listener)?)
    }

    pub fn new(sock: net::UnixListener) -> io::Result<UnixListener> {
        sock.set_nonblocking(true)?;

        Ok(UnixListener {
            inner: sock,
            selector_id: SelectorId::new()
        })
    }

    pub fn accept(&self) -> io::Result<(UnixStream, SocketAddr)> {
        self.inner.accept().and_then(|(s, a)| {
            Ok((UnixStream::new(s)?, a))
        })
    }

    pub fn try_clone(&self) -> io::Result<UnixListener> {
        self.inner.try_clone().map(|s| {
            UnixListener {
                inner: s,
                selector_id: self.selector_id.clone()
            }
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.inner.set_nonblocking(nonblocking)
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.inner.take_error()
    }
}

impl Source for UnixListener {
    fn add(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        self.selector_id.associate_selector(epoll)?;
        epoll.add(&self.as_raw_fd(), token, interest, opts)
    }

    fn modify(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        epoll.modify(&self.as_raw_fd(), token, interest, opts)
    }

    fn delete(&self, epoll: &Epoll) -> io::Result<()> {
        epoll.delete(&self.as_raw_fd())
    }
}

impl FromRawFd for UnixListener {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixListener {
        UnixListener {
            inner: net::UnixListener::from_raw_fd(fd),
            selector_id: SelectorId::new()
        }
    }
}

impl IntoRawFd for UnixListener {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}

impl AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
