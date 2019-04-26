use std::os::unix::io::RawFd;

use crate::sys::io;
use crate::{Poll, Token, Ready, PollOpt};

pub trait Evented {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()>;

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()>;

    fn deregister(&self, poll: &Poll) -> io::Result<()>;
}

impl Evented for RawFd {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        poll.0.register(*self, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        poll.0.reregister(*self, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        poll.0.deregister(*self)
    }
}
