pub mod tcp;

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::sys::io;
use crate::Poll;

#[derive(Debug)]
struct SelectorId {
    id: AtomicUsize,
}

impl SelectorId {
    fn new() -> SelectorId {
        SelectorId {
            id: AtomicUsize::new(0),
        }
    }

    fn associate_selector(&self, poll: &Poll) -> io::Result<()> {
        let selector_id = self.id.load(Ordering::SeqCst);

        if selector_id != 0 && selector_id != poll.0.id() {
            Err(io::Error::new(io::ErrorKind::Other, "socket already registered"))
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


