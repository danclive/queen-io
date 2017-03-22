use std::io::{Read, Write};
use sys;
use {io, Ready, Poll, PollOpt, Token};
use evented::Evented;

/*
 *
 * ===== Awakener =====
 *
 */

pub struct Awakener {
    reader: sys::Io,
    writer: sys::Io,
}

impl Awakener {
    pub fn new() -> io::Result<Awakener> {
        let (rd, wr) = try!(sys::pipe());

        Ok(Awakener {
            reader: rd,
            writer: wr,
        })
    }

    pub fn wakeup(&self) -> io::Result<()> {
        match (&self.writer).write(&[1]) {
            Ok(_) => Ok(()),
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }

    pub fn cleanup(&self) {
        let mut buf = [0; 128];

        loop {
            // Consume data until all bytes are purged
            match (&self.reader).read(&mut buf) {
                Ok(i) if i > 0 => {},
                _ => return,
            }
        }
    }

    fn reader(&self) -> &sys::Io {
        &self.reader
    }
}

impl Evented for Awakener {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.reader().register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.reader().reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.reader().deregister(poll)
    }
}
