use std::io;
use std::time::Duration;
use std::cmp;
use std::i32;
use std::convert::TryInto;
use std::os::unix::io::RawFd;

mod ready;
mod event;

pub use ready::Ready;
pub use event::{Event, Events};
use event::{ioevent_to_poll, poll_to_ioevent};

pub fn poll(evts: &mut Events, timeout: Option<Duration>) -> io::Result<i32> {
    let timeout = timeout
        .map(|to| cmp::min(to.as_millis(), libc::c_int::max_value() as u128) as libc::c_int)
        .unwrap_or(-1);

    let ret = unsafe { libc::poll(evts.events.as_mut_ptr(), evts.len().try_into().unwrap(), timeout) };
    if ret < 0 {
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::Interrupted {
            return Err(err);
        }
    }

    Ok(ret)
}

pub fn wait(fd: RawFd, readiness: Ready, timeout: Option<Duration>) -> io::Result<Ready> {
    let timeout = timeout
        .map(|to| cmp::min(to.as_millis(), libc::c_int::max_value() as u128) as libc::c_int)
        .unwrap_or(-1);

    let mut pollfd = libc::pollfd { fd, events: ioevent_to_poll(readiness), revents: 0 };

    let ret = unsafe { libc::poll(&mut pollfd, 1, timeout) };
    if ret < 0 {
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::Interrupted {
            return Err(err);
        }
    }

    Ok(poll_to_ioevent(pollfd.revents))
}
