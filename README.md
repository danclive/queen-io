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
use soio::tcp::{TcpListener, TcpStream};

// Setup some tokens to allow us to identify which event is
// for which socket.
const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

let addr = "127.0.0.1:13265".parse().unwrap();

// Setup the server socket
let server = TcpListener::bind(&addr).unwrap();

// Create an poll instance
let poll = Poll::new().unwrap();

// Start listening for incoming connections
poll.register(&server, SERVER, Ready::readable(),
              PollOpt::edge()).unwrap();

// Setup the client socket
let sock = TcpStream::connect(&addr).unwrap();

// Register the socket
poll.register(&sock, CLIENT, Ready::readable(),
              PollOpt::edge()).unwrap();

// Create storage for events
let mut events = Events::with_capacity(1024);

loop {
    poll.poll(&mut events, None).unwrap();

    for event in events.iter() {
        match event.token() {
            SERVER => {
                // Accept and drop the socket immediately, this will close
                // the socket and notify the client of the EOF.
                let _ = server.accept();
            }
            CLIENT => {
                // The server just shuts down the socket, let's just exit
                // from our event loop.
                return;
            }
            _ => unreachable!(),
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
