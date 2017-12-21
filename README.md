# queen-io

Queen-io is a I/O library for Rust, it originated in [carllerche/mio](https://github.com/carllerche/mio). Unlike mio, queen-io only supports Linux because it use [eventfd](http://www.man7.org/linux/man-pages/man2/eventfd.2.html) instead of pipe-- which reduces the creation of a file descriptor and is easier to create user-defined events.

