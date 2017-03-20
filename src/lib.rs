//! A fast IO library for Rust focusing on non-blocking APIs, event
//! notification, and other useful utilties for building high performance IO
//! apps.
//!
//! ## Usage
//!
//! First, add this to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! soio = "0.1"
//! ```
//!
//! Then, add this to your crate root:
//!
//! ```rust
//! extern crate soio;
//! ```
//!
//! # Example
//!
//! ```
//! use soio::{Events, Poll, Ready, PollOpt, Token};
//! use soio::tcp::{TcpListener, TcpStream};
//!
//! // Setup some tokens to allow us to identify which event is
//! // for which socket.
//! const SERVER: Token = Token(0);
//! const CLIENT: Token = Token(1);
//!
//! let addr = "127.0.0.1:13265".parse().unwrap();
//!
//! // Setup the server socket
//! let server = TcpListener::bind(&addr).unwrap();
//!
//! // Create an poll instance
//! let poll = Poll::new().unwrap();
//!
//! // Start listening for incoming connections
//! poll.register(&server, SERVER, Ready::readable(),
//!               PollOpt::edge()).unwrap();
//!
//! // Setup the client socket
//! let sock = TcpStream::connect(&addr).unwrap();
//!
//! // Register the socket
//! poll.register(&sock, CLIENT, Ready::readable(),
//!               PollOpt::edge()).unwrap();
//!
//! // Create storage for events
//! let mut events = Events::with_capacity(1024);
//!
//! loop {
//!     poll.poll(&mut events, None).unwrap();
//!
//!     for event in events.iter() {
//!         match event.token() {
//!             SERVER => {
//!                 // Accept and drop the socket immediately, this will close
//!                 // the socket and notify the client of the EOF.
//!                 let _ = server.accept();
//!             }
//!             CLIENT => {
//!                 // The server just shuts down the socket, let's just exit
//!                 // from our event loop.
//!                 return;
//!             }
//!             _ => unreachable!(),
//!         }
//!     }
//! }
//!
//! ```

extern crate libc;
extern crate net2;
#[macro_use]
extern crate log;

mod sys;
mod net;
mod io;
mod iovec;
mod event;
mod evented;
mod ready;
mod poll;
mod poll_opt;
mod registration;
mod token;
pub mod channel;

mod evloop;

pub use iovec::IoVec;
pub use net::{
    tcp,
    udp,
};

pub use event::{
    Event,
    Events,
};

pub use evented::Evented;

pub use ready::Ready;

pub use poll::Poll;

pub use poll_opt::PollOpt;

pub use registration::{
    Registration,
    SetReadiness,
};

pub use token::Token;

pub use sys::EventedFd;

pub use evloop::{
    EventLoop,
    Handler,
};
