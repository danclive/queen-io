use std::io;

use {Poll, Events, Event, Evented, Token, Ready, PollOpt};

pub struct EventLoop {
    run: bool,
    poll: Poll,
    events: Events,
}

impl EventLoop {
    pub fn new() -> io::Result<EventLoop> {
        Ok(EventLoop {
            run: true,
            poll: try!(Poll::new()),
            events: Events::with_capacity(1024),
        })
    }

    pub fn register<E: ?Sized>(&mut self, handle: &E, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()>
        where E: Evented
    {
        self.poll.register(handle, token, interest, opts)
    }

    pub fn reregister<E: ?Sized>(&mut self, handle: &E, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()>
        where E: Evented
    {
        self.poll.reregister(handle, token, interest, opts)
    }

    pub fn deregister<E: ?Sized>(&mut self, handle: &E) -> io::Result<()>
        where E: Evented
    {
        self.poll.deregister(handle)
    }

    pub fn shutdown(&mut self) {
        self.run = false;
    }

    pub fn event<H>(&mut self, handler: &mut H, event: Event)
        where H: Handler
    {
        handler.event(self, event.token(), event.readiness());
    }

    pub fn run_once<H>(&mut self, handler: &mut H) -> io::Result<()>
        where H: Handler
    {
        let size = try!(self.poll.poll(&mut self.events, None));

        for i in 0..size {
            let event = self.events.get(i).unwrap();
            self.event(handler, event)
        }

        Ok(())
    }

    pub fn run<H>(&mut self, handler: &mut H) -> io::Result<()>
        where H: Handler
    {
        self.run = true;

        while self.run {
            try!(self.run_once(handler))
        }

        Ok(())
    }
}

pub trait Handler: Sized {
    fn event(&mut self, evevt_loop: &mut EventLoop, token: Token, interest: Ready);
}
