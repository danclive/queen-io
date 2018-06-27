use std::sync::Arc;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;

use sys::io;
use sys::Awakener;

use {Poll, Token, Ready, PollOpt, Evented};

#[derive(Clone, Debug)]
pub struct Registration {
    pub awakener: Arc<Awakener>
}

impl Registration {
    pub fn new() -> io::Result<Registration>{
        let awakener = Arc::new(Awakener::new()?);

        let registration = Registration {
                awakener
        };

        Ok(registration)
    }

    pub fn set_readiness(&self, ready: Ready) -> io::Result<()> {
        if ready == Ready::readable() || ready == Ready::writable() {
            self.awakener.wakeup()?;
        }

        if ready == Ready::empty() {
            self.awakener.cleanup()
        }

        Ok(())
    }

    pub fn finish(&self) {
        self.awakener.cleanup()
    }
}

impl AsRawFd for Registration {
    fn as_raw_fd(&self) -> RawFd {
        self.awakener.as_raw_fd()
    }
}

impl Evented for Registration {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.awakener.register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.awakener.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.awakener.deregister(poll)
    }
}
