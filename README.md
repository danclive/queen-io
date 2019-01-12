# queen-io

Queen-io is a I/O library for Rust, it originated in [carllerche/mio](https://github.com/carllerche/mio). Unlike mio, queen-io only supports Linux because it use [eventfd](http://www.man7.org/linux/man-pages/man2/eventfd.2.html) instead of pipe which reduces the creation of a file descriptor and is easier to create user-defined events.

[![crates.io](https://meritbadge.herokuapp.com/queen-io)](https://crates.io/crates/queen-io)
[![Build Status](https://travis-ci.org/danclive/queen-io.svg?branch=master)](https://travis-ci.org/danclive/queen-io)
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)

## Document

* [master](https://docs.rs/queen-io)

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
queen-io = "0.2"
```

Then, add this to your crate root:

```rust
extern crate queen_io;
```
