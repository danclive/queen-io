use std::io;
use std::time::Duration;
use std::cmp;
use std::i32;
use std::convert::TryInto;

use libc::c_int;

mod ready;
mod event;

pub use ready::Ready;
pub use event::{Event, Events};

pub fn poll(evts: &mut Events, timeout: Option<Duration>) -> io::Result<i32> {
    let timeout_ms = if let Some(timeout) = &timeout {
        if timeout.as_secs() == 0 && timeout.subsec_nanos() == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot set a 0 duration timeout",
            ));
        }

        let timeout_ms = timeout.as_millis() as u64;
        let mut timeout_ms = cmp::min(timeout_ms, c_int::max_value() as u64) as c_int;

        if timeout_ms == 0 {
            timeout_ms = 1;
        }

        timeout_ms
    } else {
        -1
    };

    let ret = unsafe { libc::poll(evts.events.as_mut_ptr(), evts.len().try_into().unwrap(), timeout_ms) };
    if ret < 0 {
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::Interrupted {
            return Err(err);
        }
    }

    Ok(ret)
}
