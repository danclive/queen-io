use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::{cmp, i32, u64};
use std::io;

use libc::{self, c_int};
use libc::{EPOLLERR, EPOLLHUP};
use libc::{EPOLLET, EPOLLOUT, EPOLLIN, EPOLLPRI};
use libc::{EPOLLRDHUP, EPOLLONESHOT};

use crate::epoll::{Token, Ready, EpollOpt, Event};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

pub struct Epoll {
    id: usize,
    epfd: RawFd
}

impl Epoll {
    pub fn new() -> io::Result<Epoll> {
        let epfd = syscall!(epoll_create1(libc::EPOLL_CLOEXEC))?;

        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;

        Ok(Epoll {
            id,
            epfd
        })
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn wait(&self, evts: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        let timeout = timeout
            .map(|to| cmp::min(to.as_millis(), libc::c_int::max_value() as u128) as libc::c_int)
            .unwrap_or(-1);

        let cnt = syscall!(epoll_wait(
            self.epfd,
            evts.events.as_mut_ptr(),
            evts.events.capacity() as i32,
            timeout
        ))?;

        unsafe { evts.events.set_len(cnt as usize) };

        Ok(())
    }

    pub fn add(&self, fd: RawFd, token: Token, interests: Ready, opts: EpollOpt) -> io::Result<()> {
        let mut info = libc::epoll_event {
            events: ioevent_to_epoll(interests, opts),
            u64: usize::from(token) as u64
        };

        syscall!(epoll_ctl(self.epfd, libc::EPOLL_CTL_ADD, fd, &mut info))?;

        Ok(())
    }

    pub fn modify(&self, fd: RawFd, token: Token, interests: Ready, opts: EpollOpt) -> io::Result<()> {
        let mut info = libc::epoll_event {
            events: ioevent_to_epoll(interests, opts),
            u64: usize::from(token) as u64
        };
       
        syscall!(epoll_ctl(self.epfd, libc::EPOLL_CTL_MOD, fd, &mut info))?;
            
        Ok(())
    }

    pub fn delete(&self, fd: RawFd) -> io::Result<()> {
        let mut info = libc::epoll_event {
            events: 0,
            u64: 0,
        };

        syscall!(epoll_ctl(self.epfd, libc::EPOLL_CTL_DEL, fd, &mut info))?;

        Ok(())
    }
}

fn ioevent_to_epoll(interest: Ready, opts: EpollOpt) -> u32 {
    let mut kind = 0;

    if interest.is_readable() {
        kind |= EPOLLIN;
    }

    if interest.is_writable() {
        kind |= EPOLLOUT;
    }

    if interest.is_hup() {
        kind |= EPOLLRDHUP;
    }

    if opts.is_edge() {
        kind |= EPOLLET;
    }

    if opts.is_oneshot() {
        kind |= EPOLLONESHOT;
    }

    if opts.is_level() {
        kind &= !EPOLLET;
    }

    kind as u32
}

impl AsRawFd for Epoll {
    fn as_raw_fd(&self) -> RawFd {
        self.epfd
    }
}

impl Drop for Epoll {
    fn drop(&mut self) {
        unsafe {
            let _ = libc::close(self.epfd);
        }
    }
}

pub struct Events {
    events: Vec<libc::epoll_event>,
}

impl Events {
    pub fn with_capacity(u: usize) -> Events {
        Events {
            events: Vec::with_capacity(u)
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.events.capacity()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    #[inline]
    pub fn get(&self, idx: usize) -> Option<Event> {
        self.events.get(idx).map(|event| {
            let epoll = event.events as c_int;
            let mut kind = Ready::empty();

            if (epoll & EPOLLIN) != 0 || (epoll & EPOLLPRI) != 0 {
                kind = kind | Ready::readable();
            }

            if (epoll & EPOLLOUT) != 0 {
                kind = kind | Ready::writable();
            }

            // EPOLLHUP - Usually means a socket error happened
            if (epoll & EPOLLERR) != 0 {
                kind = kind | Ready::error();
            }

            if (epoll & EPOLLRDHUP) != 0 || (epoll & EPOLLHUP) != 0 {
                kind = kind | Ready::hup();
            }

            let token = self.events[idx].u64;

            Event::new(kind, Token(token as usize))
        })
    }
}
