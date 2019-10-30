use std::io::{self, IoSlice, IoSliceMut, Error, ErrorKind};
use std::mem;
use std::time::{Duration, Instant};
use std::cmp;
use std::net::{Shutdown, SocketAddr, SocketAddrV4, SocketAddrV6};

use libc::{self, c_int, c_void, sockaddr, socklen_t, MSG_PEEK, SOCK_CLOEXEC};

use super::fd::FileDesc;
use super::commom::{AsInner, FromInner, IntoInner};
use super::cvt;

pub fn setsockopt<T>(sock: &Socket, opt: c_int, val: c_int,
                     payload: T) -> io::Result<()> {

    let payload = &payload as *const T as *const c_void;
    syscall!(setsockopt(*sock.as_inner(), opt, val, payload,
                          mem::size_of::<T>() as libc::socklen_t))?;
    Ok(())
}

pub fn getsockopt<T: Copy>(sock: &Socket, opt: c_int,
                       val: c_int) -> io::Result<T> {
    let mut slot: T = unsafe { mem::zeroed() };
    let mut len = mem::size_of::<T>() as libc::socklen_t;
    syscall!(getsockopt(*sock.as_inner(), opt, val,
                    &mut slot as *mut _ as *mut _,
                    &mut len))?;
    assert_eq!(len as usize, mem::size_of::<T>());
    Ok(slot)
}

pub fn sockname<F>(f: F) -> io::Result<SocketAddr>
    where F: FnOnce(*mut libc::sockaddr, *mut libc::socklen_t) -> c_int
{
    unsafe {
        let mut storage: libc::sockaddr_storage = mem::zeroed();
        let mut len = mem::size_of_val(&storage) as libc::socklen_t;
        cvt(f(&mut storage as *mut _ as *mut _, &mut len))?;
        sockaddr_to_addr(&storage, len as usize)
    }
}

struct _SocketAddrV4 {
    pub inner: libc::sockaddr_in
}

struct _SocketAddrV6 {
    pub inner: libc::sockaddr_in6
}

impl FromInner<libc::sockaddr_in> for SocketAddrV4 {
    fn from_inner(addr: libc::sockaddr_in) -> SocketAddrV4 {
        unsafe {
            mem::transmute(_SocketAddrV4 { inner: addr })
        }
    }
}

impl FromInner<libc::sockaddr_in6> for SocketAddrV6 {
    fn from_inner(addr: libc::sockaddr_in6) -> SocketAddrV6 {
        unsafe {
            mem::transmute(_SocketAddrV6 { inner: addr })
        }
    }
}

impl<'a> IntoInner<(*const libc::sockaddr, libc::socklen_t)> for &'a SocketAddr {
    fn into_inner(self) -> (*const libc::sockaddr, libc::socklen_t) {
        match *self {
            SocketAddr::V4(ref a) => {
                (a as *const _ as *const _, mem::size_of_val(a) as libc::socklen_t)
            }
            SocketAddr::V6(ref a) => {
                (a as *const _ as *const _, mem::size_of_val(a) as libc::socklen_t)
            }
        }
    }
}

pub fn sockaddr_to_addr(storage: &libc::sockaddr_storage, len: usize) -> io::Result<SocketAddr> {
    match storage.ss_family as libc::c_int {
        libc::AF_INET => {
            assert!(len as usize >= mem::size_of::<libc::sockaddr_in>());
            Ok(SocketAddr::V4(FromInner::from_inner(unsafe {
                *(storage as *const _ as *const libc::sockaddr_in)
            })))
        }
        libc::AF_INET6 => {
            assert!(len as usize >= mem::size_of::<libc::sockaddr_in6>());
            Ok(SocketAddr::V6(FromInner::from_inner(unsafe {
                *(storage as *const _ as *const libc::sockaddr_in6)
            })))
        }
        _ => {
            Err(Error::new(ErrorKind::InvalidInput, "invalid argument"))
        }
    }
}

pub struct Socket(FileDesc);

impl Socket {
    pub fn new(addr: &SocketAddr, ty: c_int) -> io::Result<Socket> {
        let fam = match *addr {
            SocketAddr::V4(..) => libc::AF_INET,
            SocketAddr::V6(..) => libc::AF_INET6,
        };
        Socket::new_raw(fam, ty)
    }

    pub fn new_raw(fam: c_int, ty: c_int) -> io::Result<Socket> {
        match syscall!(socket(fam, ty | SOCK_CLOEXEC, 0)) {
            Ok(fd) => return Ok(Socket(FileDesc::new(fd))),
            Err(ref e) if e.raw_os_error() == Some(libc::EINVAL) => {}
            Err(e) => return Err(e),
        }

        let fd = syscall!(socket(fam, ty, 0))?;
        let fd = FileDesc::new(fd);
        fd.set_cloexec()?;
        let socket = Socket(fd);

        Ok(socket)
    }

    pub fn new_pair(fam: c_int, ty: c_int) -> io::Result<(Socket, Socket)> {
        let mut fds = [0, 0];

        match syscall!(socketpair(fam, ty | SOCK_CLOEXEC, 0, fds.as_mut_ptr())) {
            Ok(_) => {
                return Ok((Socket(FileDesc::new(fds[0])), Socket(FileDesc::new(fds[1]))));
            }
            Err(ref e) if e.raw_os_error() == Some(libc::EINVAL) => {},
            Err(e) => return Err(e),
        }

        syscall!(socketpair(fam, ty, 0, fds.as_mut_ptr()))?;
        let a = FileDesc::new(fds[0]);
        let b = FileDesc::new(fds[1]);
        a.set_cloexec()?;
        b.set_cloexec()?;

        Ok((Socket(a), Socket(b)))
    }

    pub fn connect_timeout(&self, addr: &SocketAddr, timeout: Duration) -> io::Result<()> {
        self.set_nonblocking(true)?;
        let (addrp, len) = addr.into_inner();
        let r = syscall!(connect(self.0.raw(), addrp, len));
        self.set_nonblocking(false)?;

        match r {
            Ok(_) => return Ok(()),
            // there's no ErrorKind for EINPROGRESS :(
            Err(ref e) if e.raw_os_error() == Some(libc::EINPROGRESS) => {}
            Err(e) => return Err(e),
        }

        let mut pollfd = libc::pollfd {
            fd: self.0.raw(),
            events: libc::POLLOUT,
            revents: 0,
        };

        if timeout.as_secs() == 0 && timeout.subsec_nanos() == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      "cannot set a 0 duration timeout"));
        }

        let start = Instant::now();

        loop {
            let elapsed = start.elapsed();
            if elapsed >= timeout {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "connection timed out"));
            }

            let timeout = timeout - elapsed;
            let mut timeout = timeout.as_secs()
                .saturating_mul(1_000)
                .saturating_add(timeout.subsec_nanos() as u64 / 1_000_000);
            if timeout == 0 {
                timeout = 1;
            }

            let timeout = cmp::min(timeout, c_int::max_value() as u64) as c_int;

            match unsafe { libc::poll(&mut pollfd, 1, timeout) } {
                -1 => {
                    let err = io::Error::last_os_error();
                    if err.kind() != io::ErrorKind::Interrupted {
                        return Err(err);
                    }
                }
                0 => {}
                _ => {
                    // linux returns POLLOUT|POLLERR|POLLHUP for refused connections (!), so look
                    // for POLLHUP rather than read readiness
                    if pollfd.revents & libc::POLLHUP != 0 {
                        let e = self.take_error()?
                            .unwrap_or_else(|| {
                                io::Error::new(io::ErrorKind::Other, "no error set after POLLHUP")
                            });
                        return Err(e);
                    }

                    return Ok(());
                }
            }
        }
    }

    pub fn accept(&self, storage: *mut sockaddr, len: *mut socklen_t)
                  -> io::Result<Socket> {

        let res = loop {
            match syscall!(accept4(self.0.raw(), storage, len, SOCK_CLOEXEC)) {
                Err(ref e) if e.kind() == ErrorKind::Interrupted => (),
                other => break other
            };
        };

        match res {
            Ok(fd) => return Ok(Socket(FileDesc::new(fd))),
            Err(ref e) if e.raw_os_error() == Some(libc::ENOSYS) => {}
            Err(e) => return Err(e),
        }

        let fd = loop {
            match syscall!(accept(self.0.raw(), storage, len)) {
                Err(ref e) if e.kind() == ErrorKind::Interrupted => (),
                other => break other
            };
        }?;

        let fd = FileDesc::new(fd);
        fd.set_cloexec()?;
        Ok(Socket(fd))
    }

    pub fn duplicate(&self) -> io::Result<Socket> {
        self.0.duplicate().map(Socket)
    }

    fn recv_with_flags(&self, buf: &mut [u8], flags: c_int) -> io::Result<usize> {
        let ret = syscall!(recv(self.0.raw(),
                       buf.as_mut_ptr() as *mut c_void,
                       buf.len(),
                       flags)
        )?;
        Ok(ret as usize)
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.recv_with_flags(buf, 0)
    }

    pub fn peek(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.recv_with_flags(buf, MSG_PEEK)
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        self.0.read_vectored(bufs)
    }

    fn recv_from_with_flags(&self, buf: &mut [u8], flags: c_int)
                            -> io::Result<(usize, SocketAddr)> {
        let mut storage: libc::sockaddr_storage = unsafe { mem::zeroed() };
        let mut addrlen = mem::size_of_val(&storage) as libc::socklen_t;

        let n = syscall!(recvfrom(self.0.raw(),
                        buf.as_mut_ptr() as *mut c_void,
                        buf.len(),
                        flags,
                        &mut storage as *mut _ as *mut _,
                        &mut addrlen)
        )?;
        Ok((n as usize, sockaddr_to_addr(&storage, addrlen as usize)?))
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.recv_from_with_flags(buf, 0)
    }

    pub fn peek_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.recv_from_with_flags(buf, MSG_PEEK)
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        self.0.write_vectored(bufs)
    }

    pub fn set_timeout(&self, dur: Option<Duration>, kind: libc::c_int) -> io::Result<()> {
        let timeout = match dur {
            Some(dur) => {
                if dur.as_secs() == 0 && dur.subsec_nanos() == 0 {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                              "cannot set a 0 duration timeout"));
                }

                let secs = if dur.as_secs() > libc::time_t::max_value() as u64 {
                    libc::time_t::max_value()
                } else {
                    dur.as_secs() as libc::time_t
                };
                let mut timeout = libc::timeval {
                    tv_sec: secs,
                    tv_usec: (dur.subsec_nanos() / 1000) as libc::suseconds_t,
                };
                if timeout.tv_sec == 0 && timeout.tv_usec == 0 {
                    timeout.tv_usec = 1;
                }
                timeout
            }
            None => {
                libc::timeval {
                    tv_sec: 0,
                    tv_usec: 0,
                }
            }
        };
        setsockopt(self, libc::SOL_SOCKET, kind, timeout)
    }

    pub fn timeout(&self, kind: libc::c_int) -> io::Result<Option<Duration>> {
        let raw: libc::timeval = getsockopt(self, libc::SOL_SOCKET, kind)?;
        if raw.tv_sec == 0 && raw.tv_usec == 0 {
            Ok(None)
        } else {
            let sec = raw.tv_sec as u64;
            let nsec = (raw.tv_usec as u32) * 1000;
            Ok(Some(Duration::new(sec, nsec)))
        }
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let how = match how {
            Shutdown::Write => libc::SHUT_WR,
            Shutdown::Read => libc::SHUT_RD,
            Shutdown::Both => libc::SHUT_RDWR,
        };
        syscall!(shutdown(self.0.raw(), how))?;
        Ok(())
    }

    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        setsockopt(self, libc::IPPROTO_TCP, libc::TCP_NODELAY, nodelay as c_int)
    }

    pub fn nodelay(&self) -> io::Result<bool> {
        let raw: c_int = getsockopt(self, libc::IPPROTO_TCP, libc::TCP_NODELAY)?;
        Ok(raw != 0)
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        let mut nonblocking = nonblocking as libc::c_int;
        syscall!(ioctl(*self.as_inner(), libc::FIONBIO, &mut nonblocking)).map(|_| ())
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        let raw: c_int = getsockopt(self, libc::SOL_SOCKET, libc::SO_ERROR)?;
        if raw == 0 {
            Ok(None)
        } else {
            Ok(Some(io::Error::from_raw_os_error(raw as i32)))
        }
    }
}

impl AsInner<c_int> for Socket {
    fn as_inner(&self) -> &c_int { self.0.as_inner() }
}

impl FromInner<c_int> for Socket {
    fn from_inner(fd: c_int) -> Socket { Socket(FileDesc::new(fd)) }
}

impl IntoInner<c_int> for Socket {
    fn into_inner(self) -> c_int { self.0.into_raw() }
}
