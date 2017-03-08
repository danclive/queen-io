# soio
Soio is a I/O library for Rust.

[![crates.io](http://meritbadge.herokuapp.com/soio)](https://crates.io/crates/soio)
[![Build Status](https://travis-ci.org/mcorce/soio.svg?branch=master)](https://travis-ci.org/mcorce/soio)

**Document**

* [master](https://docs.rs/soio)

## Usage

First, add this to your `Cargo.toml`:

```toml
[dependencies]
soio = "0.1"
```

Then, add this to your crate root:

```rust
extern crate soio:
```

Example:

```rust
use soio::{Events, Poll, Ready, PollOpt, Token};
use soio::tcp::TcpStream;

use std::net::{TcpListener, SocketAddr};

// Bind a server socket to connect to.
let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
let server = TcpListener::bind(&addr).unwrap();

// Construct a new `Poll` handle as well as the `Events` we'll store into
let poll = Poll::new().unwrap();
let mut events = Events::with_capacity(1024);

// Connect the stream
let stream = TcpStream::connect(&server.local_addr().unwrap()).unwrap();

// Register the stream with `Poll`
poll.register(&stream, Token(0), Ready::readable() | Ready::writable(), PollOpt::edge()).unwrap();

// Wait for the socket to become ready. This has to happens in a loop to
// handle spurious wakeups.
loop {
    poll.poll(&mut events, None).unwrap();

    for event in &events {
        if event.token() == Token(0) && event.readiness().is_writable() {
            // The socket connected (probably, it could still be a spurious
            // wakeup)
            return;
        }
    }
}
```

## Feature

* Bakced by epoll kqueue
* Non-blocking TCP, UDP sockets
* Thread safe message channel for cross thread communication

## Platforms

* Linux
* OS X
* NetBSD
* Android
* iOS
