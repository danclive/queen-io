use std::io;
use {Poll, Token, Ready, PollOpt};
/// A value that may be registered with `Poll`
///
/// Values that implement `Evented` can be registered with `Poll`. Users of soio
/// should not use the `Evented` trait functions directly. Instead, the
/// equivalent functions on `Poll` should be used.
///
/// See [`Poll`] for more details.
///
/// # Implementing `Evented`
///
/// There are two types of `Evented` values.
///
/// * **System** handles, which are backed by sockets or other system handles.
/// These `Evented` handles will be monitored by the system selector. In this
/// case, an implementation of `Evented` delegates to a lower level handle.
///
/// * **User** handles, which are driven entirely in user space using
/// [`Registration`] and [`SetReadiness`]. In this case, the implementer takes
/// responsibility for driving the readiness state changes.
///
/// [`Poll`]: struct.Poll.html
/// [`Registration`]: struct.Registration.html
/// [`SetReadiness`]: struct.SetReadiness.html
///
/// # Examples
///
/// Implementing `Evented` on a struct containing a socket:
///
/// ```
/// use soio::{Ready, Poll, PollOpt, Token};
/// use soio::Evented;
/// use soio::tcp::TcpStream;
///
/// use std::io;
///
/// pub struct MyEvented {
///     socket: TcpStream,
/// }
///
/// impl Evented for MyEvented {
///     fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt)
///         -> io::Result<()>
///     {
///         // Delegate the `register` call to `socket`
///         self.socket.register(poll, token, interest, opts)
///     }
///
///     fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt)
///         -> io::Result<()>
///     {
///         // Delegate the `reregister` call to `socket`
///         self.socket.reregister(poll, token, interest, opts)
///     }
///
///     fn deregister(&self, poll: &Poll) -> io::Result<()> {
///         // Delegate the `deregister` call to `socket`
///         self.socket.deregister(poll)
///     }
/// }
/// ```
///
/// Implement `Evented` using [`Registration`] and [`SetReadiness`].
///
/// ```
/// use soio::{Ready, Registration, Poll, PollOpt, Token};
/// use soio::Evented;
///
/// use std::io;
/// use std::time::Instant;
/// use std::thread;
///
/// pub struct Deadline {
///     when: Instant,
///     registration: Registration,
/// }
///
/// impl Deadline {
///     pub fn new(when: Instant) -> Deadline {
///         let (registration, set_readiness) = Registration::new();
///
///         thread::spawn(move || {
///             let now = Instant::now();
///
///             if now < when {
///                 thread::sleep(when - now);
///             }
///
///             set_readiness.set_readiness(Ready::readable());
///         });
///
///         Deadline {
///             when: when,
///             registration: registration,
///         }
///     }
///
///     pub fn is_elapsed(&self) -> bool {
///         Instant::now() >= self.when
///     }
/// }
///
/// impl Evented for Deadline {
///     fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt)
///         -> io::Result<()>
///     {
///         self.registration.register(poll, token, interest, opts)
///     }
///
///     fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt)
///         -> io::Result<()>
///     {
///         self.registration.reregister(poll, token, interest, opts)
///     }
///
///     fn deregister(&self, poll: &Poll) -> io::Result<()> {
///         self.registration.deregister(poll)
///     }
/// }
/// ```
pub trait Evented {
    /// Register `self` with the given `Poll` instance.
    ///
    /// This function should not be called directly. Use [`Poll::register`]
    /// instead. Implementors should handle registration by either delegating
    /// the call to another `Evented` type or calling [`Registration::register`].
    ///
    /// See [struct] documentation for more details.
    ///
    /// [`Poll::register`]: struct.Poll.html#method.register
    /// [`Registration::register`]: struct.Registration.html#method.register
    /// [struct]: #
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()>;

    /// Re-register `self` with the given `Poll` instance.
    ///
    /// This function should not be called directly. Use [`Poll::reregister`]
    /// instead. Implementors should handle re-registration by either delegating
    /// the call to another `Evented` type or calling [`Registration::reregister`].
    ///
    /// See [struct] documentation for more details.
    ///
    /// [`Poll::reregister`]: struct.Poll.html#method.register
    /// [`Registration::reregister`]: struct.Registration.html#method.reregister
    /// [struct]: #
    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()>;

    /// Deregister `self` from the given `Poll` instance
    ///
    /// This function should not be called directly. Use [`Poll::deregister`]
    /// instead. Implementors shuld handle deregistration by either delegating
    /// the call to another `Evented` type or by dropping the [`Registration`]
    /// associated with `self`.
    ///
    /// See [struct] documentation for more details.
    ///
    /// [`Poll::deregister`]: struct.Poll.html#method.deregister
    /// [`Registration`]: struct.Registration.html
    /// [struct]: #
    fn deregister(&self, poll: &Poll) -> io::Result<()>;
}
