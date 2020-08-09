use std::os::unix::io::RawFd;
use std::io;

use super::{Epoll, Token, Ready, EpollOpt};

pub trait Source {
    fn add(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()>;

    fn modify(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()>;

    fn delete(&self, epoll: &Epoll) -> io::Result<()>;
}

impl Source for RawFd {
    fn add(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        epoll.0.add(*self, token, interest, opts)
    }

    fn modify(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        epoll.0.modify(*self, token, interest, opts)
    }

    fn delete(&self, epoll: &Epoll) -> io::Result<()> {
        epoll.0.delete(*self)
    }
}
