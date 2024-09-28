use std::fmt;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::sys;

pub use epoll_opt::EpollOpt;
pub use event::{Event, Events, IntoIter, Iter};
pub use ready::Ready;
pub use source::Source;
pub use token::Token;

mod epoll_opt;
mod event;
mod ready;
mod source;
mod token;

pub struct Epoll(pub(crate) sys::Epoll);

impl Epoll {
    pub fn new() -> io::Result<Epoll> {
        is_send::<Epoll>();
        is_sync::<Epoll>();

        Ok(Epoll(sys::Epoll::new()?))
    }

    pub fn wait(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<usize> {
        self.0.wait(&mut events.inner, timeout)?;
        Ok(events.len())
    }

    pub fn add<S>(
        &self,
        source: &S,
        token: Token,
        interest: Ready,
        opts: EpollOpt,
    ) -> io::Result<()>
    where
        S: Source + ?Sized,
    {
        validate_args(token, interest)?;

        source.add(self, token, interest, opts)?;

        Ok(())
    }

    pub fn modify<S>(
        &self,
        source: &S,
        token: Token,
        interest: Ready,
        opts: EpollOpt,
    ) -> io::Result<()>
    where
        S: Source + ?Sized,
    {
        validate_args(token, interest)?;

        source.modify(self, token, interest, opts)?;

        Ok(())
    }

    pub fn delete<S>(&self, source: &S) -> io::Result<()>
    where
        S: Source + ?Sized,
    {
        source.delete(self)?;

        Ok(())
    }
}

impl AsRawFd for Epoll {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl fmt::Debug for Epoll {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Epoll")
    }
}

fn validate_args(_token: Token, interest: Ready) -> io::Result<()> {
    if !interest.is_readable() && !interest.is_writable() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "interest must include readable or writable",
        ));
    }

    Ok(())
}

fn is_send<T: Send>() {}
fn is_sync<T: Sync>() {}

#[derive(Debug, Default)]
pub struct SelectorId {
    id: AtomicUsize,
}

impl SelectorId {
    pub fn new() -> SelectorId {
        SelectorId {
            id: AtomicUsize::new(0),
        }
    }

    pub fn associate_selector(&self, poll: &Epoll) -> io::Result<()> {
        let selector_id = self.id.load(Ordering::SeqCst);

        if selector_id != 0 && selector_id != poll.0.id() {
            Err(io::Error::new(io::ErrorKind::Other, "socket already added"))
        } else {
            self.id.store(poll.0.id(), Ordering::SeqCst);
            Ok(())
        }
    }
}

impl Clone for SelectorId {
    fn clone(&self) -> SelectorId {
        SelectorId {
            id: AtomicUsize::new(self.id.load(Ordering::SeqCst)),
        }
    }
}
