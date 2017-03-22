use std::{fmt, io, usize};
use std::sync::{Mutex, Condvar};
use std::sync::atomic::{AtomicUsize};
use std::sync::atomic::Ordering::{Release, SeqCst};
use std::time::{Duration, Instant};

use {sys, Token, Ready, PollOpt, Evented, Events};
use registration::ReadinessQueue;

// Poll is backed by two readiness queues. The first is a system readiness queue
// represented by `sys::Selector`. The system readiness queue handles events
// provided by the system, such as TCP and UDP. The second readiness queue is
// implemented in user space by `ReadinessQueue`. It provides a way to implement
// purely user space `Evented` types.
//
// `ReadinessQueue` is is backed by a MPSC queue that supports reuse of linked
// list nodes. This significantly reduces the number of required allocations.
// Each `Registration` / `SetReadiness` pair allocates a single readiness node
// that is used for the lifetime of the registration.
//
// The readiness node also includes a single atomic variable, `state` that
// tracks most of the state associated with the registration. This includes the
// current readiness, interest, poll options, and internal state. When the node
// state is mutated, it is queued in the MPSC channel. A call to
// `ReadinessQueue::poll` will dequeue and process nodes. The node state can
// still be mutated while it is queued in the channel for processing.
// Intermediate state values do not matter as long as the final state is
// included in the call to `poll`. This is the eventually consistent nature of
// the readiness queue.
//
// The readiness node is ref counted using the `ref_count` field. On creation,
// the ref_count is initialized to 3: one `Registration` handle, one
// `SetReadiness` handle, and one for the readiness queue. Since the readiness queue
// doesn't *always* hold a handle to the node, we don't use the Arc type for
// managing ref counts (this is to avoid constantly incrementing and
// decrementing the ref count when pushing & popping from the queue). When the
// `Registration` handle is dropped, the `dropped` flag is set on the node, then
// the node is pushed into the registration queue. When Poll::poll pops the
// node, it sees the drop flag is set, and decrements it's ref count.
//
// The MPSC queue is a modified version of the intrusive MPSC node based queue
// described by 1024cores [1].
//
// The first modification is that two markers are used instead of a single
// `stub`. The second marker is a `sleep_marker` which is used to signal to
// producers that the consumer is going to sleep. This sleep_marker is only used
// when the queue is empty, implying that the only node in the queue is
// `end_marker`.
//
// The second modification is an `until` argument passed to the dequeue
// function. When `poll` encounters a level-triggered node, the node will be
// immediately pushed back into the queue. In order to avoid an infinite loop,
// `poll` before pushing the node, the pointer is saved off and then passed
// again as the `until` argument. If the next node to pop is `until`, then
// `Dequeue::Empty` is returned.
//
// [1] http://www.1024cores.net/home/lock-free-algorithms/queues/intrusive-mpsc-node-based-queue


/// Polls for readiness events on all registered values.
///
/// `Poll` allows a program to monitor a large number of `Evented` types,
/// waiting until one or more become "ready" for some class of operations; e.g.
/// reading and writing. An `Evented` type is considered ready if it is possible
/// to immediately perform a corresponding operation; e.g. [`read`] or
/// [`write`].
///
/// To use `Poll`, an `Evented` type must first be registered with the `Poll`
/// instance using the [`register`] method, supplying readiness interest. The
/// readiness interest tells `Poll` which specific operations on the handle to
/// monitor for readiness. A `Token` is also passed to the [`register`]
/// function. When `Poll` returns a readiness event, it will include this token.
/// This associates the event with the `Evented` handle that generated the
/// event.
///
/// [`read`]: tcp/struct.TcpStream.html#method.read
/// [`write`]: tcp/struct.TcpStream.html#method.write
/// [`register`]: #method.register
///
/// # Examples
///
/// A basic example -- establishing a `TcpStream` connection.
///
/// ```
/// use soio::{Events, Poll, Ready, PollOpt, Token};
/// use soio::tcp::TcpStream;
///
/// use std::net::{TcpListener, SocketAddr};
///
/// // Bind a server socket to connect to.
/// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
/// let server = TcpListener::bind(&addr).unwrap();
///
/// // Construct a new `Poll` handle as well as the `Events` we'll store into
/// let poll = Poll::new().unwrap();
/// let mut events = Events::with_capacity(1024);
///
/// // Connect the stream
/// let stream = TcpStream::connect(&server.local_addr().unwrap()).unwrap();
///
/// // Register the stream with `Poll`
/// poll.register(&stream, Token(0), Ready::readable() | Ready::writable(), PollOpt::edge()).unwrap();
///
/// // Wait for the socket to become ready. This has to happens in a loop to
/// // handle spurious wakeups.
/// loop {
///     poll.poll(&mut events, None).unwrap();
///
///     for event in &events {
///         if event.token() == Token(0) && event.readiness().is_writable() {
///             // The socket connected (probably, it could still be a spurious
///             // wakeup)
///             return;
///         }
///     }
/// }
/// ```
///
/// # Edge-triggered and level-triggered
///
/// An [`Evented`] registration may request edge-triggered events or
/// level-triggered events. This is done by setting `register`'s
/// [`PollOpt`] argument to either [`edge`] or [`level`].
///
/// The difference between the two can be described as follows. Supposed that
/// this scenario happens:
///
/// 1. A [`TcpStream`] is registered with `Poll`.
/// 2. The socket receives 2kb of data.
/// 3. A call to [`Poll::poll`] returns the token associated with the socket
///    indicating readable readiness.
/// 4. 1kb is read from the socket.
/// 5. Another call to [`Poll::poll`] is made.
///
/// If when the socket was registered with `Poll`, edge triggered events were
/// requested, then the call to [`Poll::poll`] done in step **5** will
/// (probably) hang despite there being another 1kb still present in the socket
/// read buffer. The reason for this is that edge-triggered mode delivers events
/// only when changes occur on the monitored [`Evented`]. So, in step *5* the
/// caller might end up waiting for some data that is already present inside the
/// socket buffer.
///
/// With edge-triggered events, operations **must** be performed on the
/// `Evented` type until [`WouldBlock`] is returned. In other words, after
/// receiving an event indicating readiness for a certain operation, one should
/// assume that [`Poll::poll`] may never return another event for the same token
/// and readiness until the operation returns [`WouldBlock`].
///
/// By contrast, when level-triggered notfications was requested, each call to
/// [`Poll::poll`] will return an event for the socket as long as data remains
/// in the socket buffer. Generally, level-triggered events should be avoided if
/// high performance is a concern.
///
/// Since even with edge-triggered events, multiple events can be generated upon
/// receipt of multiple chunks of data, the caller has the option to set the
/// [`oneshot`] flag. This tells `Poll` to disable the associated [`Evented`]
/// after the event is returned from [`Poll::poll`]. The subsequent calls to
/// [`Poll::poll`] will no longer include events for [`Evented`] handles that
/// are disabled even if the readiness state changes. The handle can be
/// re-enabled by calling [`reregister`]. When handles are disabled, internal
/// resources used to monitor the handle are maintained until the handle is
/// dropped or deregistered. This makes re-registering the handle a fast
/// operation.
///
/// For example, in the following scenario:
///
/// 1. A [`TcpStream`] is registered with `Poll`.
/// 2. The socket receives 2kb of data.
/// 3. A call to [`Poll::poll`] returns the token associated with the socket
///    indicating readable readiness.
/// 4. 2kb is read from the socket.
/// 5. Another call to read is issued and [`WouldBlock`] is returned
/// 6. The socket receives another 2kb of data.
/// 7. Another call to [`Poll::poll`] is made.
///
/// Assuming the socket was registered with `Poll` with the [`edge`] and
/// [`oneshot`] options, then the call to [`Poll::poll`] in step 7 would block. This
/// is because, [`oneshot`] tells `Poll` to disable events for the socket after
/// returning an event.
///
/// In order to receive the event for the data received in step 6, the socket
/// would need to be reregistered using [`reregister`].
///
/// [`PollOpt`]: struct.PollOpt.html
/// [`edge`]: struct.PollOpt.html#method.edge
/// [`level`]: struct.PollOpt.html#method.level
/// [`Poll::poll`]: struct.Poll.html#method.poll
/// [`WouldBlock`]: https://doc.rust-lang.org/std/io/enum.ErrorKind.html#variant.WouldBlock
/// [`Evented`]: event/trait.Evented.html
/// [`TcpStream`]: tcp/struct.TcpStream.html
/// [`reregister`]: #method.reregister
/// [`oneshot`]: struct.PollOpt.html#method.oneshot
///
/// # Portability
///
/// Using `Poll` provides a portable interface across supported platforms as
/// long as the caller takes the following into consideration:
///
/// ### Spurious events
///
/// [`Poll::poll`] may return readiness events even if the associated
/// [`Evented`] handle is not actually ready. Given the same code, this may
/// happen more on some platforms than others. It is important to never assume
/// that, just because a readiness notification was received, that the
/// associated operation will as well.
///
/// If operation fails with [`WouldBlock`], then the caller should not treat
/// this as an error and wait until another readiness event is received.
///
/// ### Draining readiness
///
/// When using edge-triggered mode, once a readiness event is received, the
/// corresponding operation must be performed repeatedly until it returns
/// [`WouldBlock`]. Unless this is done, there is no guarantee that another
/// readiness event will be delivered, even if further data is received for the
/// [`Evented`] handle.
///
/// For example, in the first scenario described above, after step 5, even if
/// the socket receives more data there is no guarantee that another readiness
/// event will be delivered.
///
/// ### Readiness operations
///
/// The only readiness operations that are guaranteed to be present on all
/// supported platforms are [`readable`] and [`writable`]. All other readiness
/// operations may have false negatives and as such should be considered
/// **hints**. This means that if a socket is registered with [`readable`],
/// [`error`], and [`hup`] interest, and either an error or hup is received, a
/// readiness event will be generated for the socket, but it **may** only
/// include `readable` readiness. Also note that, given the potential for
/// spurious events, receiving a readiness event with `hup` or `error` doesn't
/// actually mean that a `read` on the socket will return a result matching the
/// readiness event.
///
/// In other words, portable programs that explicitly check for [`hup`] or
/// [`error`] readiness should be doing so as an **optimization** and always be
/// able to handle an error or HUP situation when performing the actual read
/// operation.
///
/// [`readable`]: struct.Ready.html#method.readable
/// [`writable`]: struct.Ready.html#method.writable
/// [`error`]: struct.Ready.html#method.error
/// [`hup`]: struct.Ready.html#method.hup
///
/// ### Registering handles
///
/// Unless otherwise noted, it should be assumed that types implementing
/// [`Evented`] will never be become ready unless they are registered with `Poll`.
///
/// For example:
///
/// ```
/// use soio::{Poll, Ready, PollOpt, Token};
/// use soio::tcp::TcpStream;
/// use std::time::Duration;
/// use std::thread;
///
/// let sock = TcpStream::connect(&"216.58.193.100:80".parse().unwrap()).unwrap();
///
/// thread::sleep(Duration::from_secs(1));
///
/// let poll = Poll::new().unwrap();
///
/// // The connect is not guaranteed to have started until it is registered at
/// // this point
/// poll.register(&sock, Token(0), Ready::readable() | Ready::writable(), PollOpt::edge()).unwrap();
/// ```
///
/// # Implementation notes
///
/// `Poll` is backed by the selector provided by the operating system.
///
/// |      OS    |  Selector |
/// |------------|-----------|
/// | Linux      | [epoll]   |
/// | OS X, iOS  | [kqueue]  |
/// | FreeBSD    | [kqueue]  |
/// | Android    | [epoll]   |
///
/// On all supported platforms, socket operations are handled by using the
/// system selector. Platform specific extensions (e.g. [`EventedFd`]) allow
/// accessing other features provided by individual system selectors. For
/// example, Linux's [`signalfd`] feature can be used by registering the FD with
/// `Poll` via [`EventedFd`].
///
/// On all platforms except windows, a call to [`Poll::poll`] is mostly just a
/// direct call to the system selector. However, [IOCP] uses a completion model
/// instead of a readiness model. In this case, `Poll` must adapt the completion
/// model soio's API. While non-trivial, the bridge layer is still quite
/// efficient. The most expensive part being calls to `read` and `write` require
/// data to be copied into an intermediate buffer before it is passed to the
/// kernel.
///
/// Notifications generated by [`SetReadiness`] are handled by an internal
/// readiness queue. A single call to [`Poll::poll`] will collect events from
/// both from the system selector and the internal readiness queue.
///
/// [epoll]: http://man7.org/linux/man-pages/man7/epoll.7.html
/// [kqueue]: https://www.freebsd.org/cgi/man.cgi?query=kqueue&sektion=2
/// [IOCP]: https://msdn.microsoft.com/en-us/library/windows/desktop/aa365198(v=vs.85).aspx
/// [`signalfd`]: http://man7.org/linux/man-pages/man2/signalfd.2.html
/// [`EventedFd`]: struct.EventedFd.html
/// [`SetReadiness`]: struct.SetReadiness.html
/// [`Poll::poll`]: struct.Poll.html#method.poll
pub struct Poll {
    // Platform specific IO selector
    selector: sys::Selector,

    // Custom readiness queue
    pub readiness_queue: ReadinessQueue,

    // Use an atomic to first check if a full lock will be required. This is a
    // fast-path check for single threaded cases avoiding the extra syscall
    lock_state: AtomicUsize,

    // Sequences concurrent calls to `Poll::poll`
    lock: Mutex<()>,

    // Wakeup the next waiter
    condvar: Condvar,
}

const AWAKEN: Token = Token(usize::MAX);

/*
 *
 * ===== Poll =====
 *
 */

impl Poll {
    /// Return a new `Poll` handle.
    ///
    /// This function will make a syscall to the operating system to create the
    /// system selector. If this syscall fails, `Poll::new` will return with the
    /// error.
    ///
    /// See [struct] level docs for more details.
    ///
    /// [struct]: struct.Poll.html
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::{Poll, Events};
    /// use std::time::Duration;
    ///
    /// let poll = match Poll::new() {
    ///     Ok(poll) => poll,
    ///     Err(e) => panic!("failed to create Poll instance; err={:?}", e),
    /// };
    ///
    /// // Create a structure to receive polled events
    /// let mut events = Events::with_capacity(1024);
    ///
    /// // Wait for events, but none will be received because no `Evented`
    /// // handles have been registered with this `Poll` instance.
    /// let n = poll.poll(&mut events, Some(Duration::from_millis(500))).unwrap();
    /// assert_eq!(n, 0);
    /// ```
    pub fn new() -> io::Result<Poll> {
        is_send::<Poll>();
        is_sync::<Poll>();

        let poll = Poll {
            selector: try!(sys::Selector::new()),
            readiness_queue: try!(ReadinessQueue::new()),
            lock_state: AtomicUsize::new(0),
            lock: Mutex::new(()),
            condvar: Condvar::new(),
        };

        // Register the notification wakeup FD with the IO poller
        try!(poll.readiness_queue.inner.awakener.register(&poll, AWAKEN, Ready::readable(), PollOpt::edge()));

        Ok(poll)
    }

    /// Register an `Evented` handle with the `Poll` instance.
    ///
    /// Once registerd, the `Poll` instance will monitor the `Evented` handle
    /// for readiness state changes. When it notices a state change, it will
    /// return a readiness event for the handle the next time [`poll`] is
    /// called.
    ///
    /// See the [`struct`] docs for a high level overview.
    ///
    /// # Arguments
    ///
    /// `handle: &E: Evented`: This is the handle that the `Poll` instance
    /// should monitor for readiness state changes.
    ///
    /// `token: Token`: The caller picks a token to associate with the socket.
    /// When [`poll`] returns an event for the handle, this token is included.
    /// This allows the caller to map the event to its handle. The token
    /// associated with the `Evented` handle can be changed at any time by
    /// calling [`reregister`].
    ///
    /// `token` cannot be `Token(usize::MAX)` as it is reserved for internal
    /// usage.
    ///
    /// See documentation on [`Token`] for an example showing how to pick
    /// [`Token`] values.
    ///
    /// `interest: Ready`: Specifies which operations `Poll` should monitor for
    /// readiness. `Poll` will only return readiness events for operations
    /// specified by this argument.
    ///
    /// If a socket is registered with [`readable`] interest and the socket
    /// becomes writable, no event will be returned from [`poll`].
    ///
    /// The readiness interest for an `Evented` handle can be changed at any
    /// time by calling [`reregister`].
    ///
    /// `opts: PollOpt`: Specifies the registration options. The most common
    /// options being [`level`] for level-triggered events, [`edge`] for
    /// edge-triggered events, and [`oneshot`].
    ///
    /// The registration options for an `Evented` handle can be changed at any
    /// time by calling [`reregister`].
    ///
    /// # Notes
    ///
    /// Unless otherwise specified, the caller should assume that once an
    /// `Evented` handle is registered with a `Poll` instance, it is bound to
    /// that `Poll` instance for the lifetime of the `Evented` handle. This
    /// remains true even if the `Evented` handle is deregistered from the poll
    /// instance using [`deregister`].
    ///
    /// This function is **thread safe**. It can be called concurrently from
    /// multiple threads.
    ///
    /// [`struct`]: #
    /// [`reregister`]: #method.reregister
    /// [`deregister`]: #method.deregister
    /// [`poll`]: #method.poll
    /// [`level`]: struct.PollOpt.html#method.level
    /// [`edge`]: struct.PollOpt.html#method.edge
    /// [`oneshot`]: struct.PollOpt.html#method.oneshot
    /// [`Token`]: struct.Token.html
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::{Events, Poll, Ready, PollOpt, Token};
    /// use soio::tcp::TcpStream;
    /// use std::time::{Duration, Instant};
    ///
    /// let poll = Poll::new().unwrap();
    /// let socket = TcpStream::connect(&"216.58.193.100:80".parse().unwrap()).unwrap();
    ///
    /// // Register the socket with `poll`
    /// poll.register(&socket, Token(0), Ready::readable() | Ready::writable(), PollOpt::edge()).unwrap();
    ///
    /// let mut events = Events::with_capacity(1024);
    /// let start = Instant::now();
    /// let timeout = Duration::from_millis(500);
    ///
    /// loop {
    ///     let elapsed = start.elapsed();
    ///
    ///     if elapsed >= timeout {
    ///         // Connection timed out
    ///         return;
    ///     }
    ///
    ///     let remaining = timeout - elapsed;
    ///     poll.poll(&mut events, Some(remaining)).unwrap();
    ///
    ///     for event in &events {
    ///         if event.token() == Token(0) {
    ///             // Something (probably) happened on the socket.
    ///             return;
    ///         }
    ///     }
    /// }
    /// ```
    pub fn register<E: ?Sized>(&self, handle: &E, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()>
        where E: Evented
    {
        try!(validate_args(token, interest));

        /*
         * Undefined behavior:
         * - Reusing a token with a different `Evented` without deregistering
         * (or closing) the original `Evented`.
         */
        trace!("registering with poller");

        // Register interests for this socket
        try!(handle.register(self, token, interest, opts));

        Ok(())
    }

    /// Re-register an `Evented` handle with the `Poll` instance.
    ///
    /// Re-registering an `Evented` handle allows changing the details of the
    /// registration. Specifically, it allows updating the associated `token`,
    /// interest`, and `opts` specified in previous `register` and `reregister`
    /// calls.
    ///
    /// The `reregister` arguments fully override the previous values. In other
    /// words, if a socket is registered with [`readable`] interest and the call
    /// to `reregister` specifies [`writable`], then read interest is no longer
    /// requested for the handle.
    ///
    /// The `Evented` handle must have previously been registered with this
    /// instance of `Poll` otherwise the call to `reregister` will return with
    /// an error.
    ///
    /// `token` cannot be `Token(usize::MAX)` as it is reserved for internal
    /// usage.
    ///
    /// See the [`register`] documentation for details about the function
    /// arguments and see the [`struct`] docs for a high level overview of
    /// polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::{Poll, Ready, PollOpt, Token};
    /// use soio::tcp::TcpStream;
    ///
    /// let poll = Poll::new().unwrap();
    /// let socket = TcpStream::connect(&"216.58.193.100:80".parse().unwrap()).unwrap();
    ///
    /// // Register the socket with `poll`, requesting readable
    /// poll.register(&socket, Token(0), Ready::readable(), PollOpt::edge()).unwrap();
    ///
    /// // Reregister the socket specifying a different token and write interest
    /// // instead. `PollOpt::edge()` must be specified even though that value
    /// // is not being changed.
    /// poll.reregister(&socket, Token(2), Ready::writable(), PollOpt::edge()).unwrap();
    /// ```
    ///
    /// [`struct`]: #
    /// [`register`]: #method.register
    /// [`readable`]: struct.Ready.html#method.readable
    /// [`writable`]: struct.Ready.html#method.writable
    pub fn reregister<E: ?Sized>(&self, handle: &E, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()>
        where E: Evented
    {
        try!(validate_args(token, interest));

        trace!("registering with poller");

        // Register interests for this socket
        try!(handle.reregister(self, token, interest, opts));

        Ok(())
    }

    /// Deregister an `Evented` handle with the `Poll` instance.
    ///
    /// When an `Evented` handle is deregistered, the `Poll` instance will
    /// no longer monitor it for readiness state changes. Unlike disabiling
    /// handles with [`oneshot`], deregistering clears up any internal resources
    /// needed to track the handle.
    ///
    /// A handle can be passed back to `register` after it has been
    /// deregistered; however, it must be passed back to the **same** `Poll`
    /// instance.
    ///
    /// `Evented` handles are automatically deregistered when they are dropped.
    /// It is common to never need to explicitly call `deregister`.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::{Events, Poll, Ready, PollOpt, Token};
    /// use soio::tcp::TcpStream;
    /// use std::time::Duration;
    ///
    /// let poll = Poll::new().unwrap();
    /// let socket = TcpStream::connect(&"216.58.193.100:80".parse().unwrap()).unwrap();
    ///
    /// // Register the socket with `poll`
    /// poll.register(&socket, Token(0), Ready::readable(), PollOpt::edge()).unwrap();
    ///
    /// poll.deregister(&socket).unwrap();
    ///
    /// let mut events = Events::with_capacity(1024);
    ///
    /// // Set a timeout because this poll should never receive any events.
    /// let n = poll.poll(&mut events, Some(Duration::from_secs(1))).unwrap();
    /// assert_eq!(0, n);
    /// ```
    pub fn deregister<E: ?Sized>(&self, handle: &E) -> io::Result<()>
        where E: Evented
    {
        trace!("deregistering handle with poller");

        // Deregister interests for this socket
        try!(handle.deregister(self));

        Ok(())
    }

    /// Wait for readiness events
    ///
    /// Blocks the current thread and waits for readiness events for any of the
    /// `Evented` handles that have been registered with this `Poll` instance.
    /// The function will block until either at least one readiness event has
    /// been received or `timeout` has elapsed. A `timeout` of `None` means that
    /// `poll` will block until a readiness event has been received.
    ///
    /// The supplied `events` will be cleared and newly received readinss events
    /// will be pushed onto the end. At most `events.capacity()` events will be
    /// returned. If there are further pending readiness events, they will be
    /// returned on the next call to `poll`.
    ///
    /// A single call to `poll` may result in multiple readiness events being
    /// returned for a single `Evented` handle. For example, if a TCP socket
    /// becomes both readable and writable, it may be possible for a single
    /// readiness event to be returned with both [`readable`] and [`writable`]
    /// readiness **OR** two separate events may be returned, one with
    /// [`readable`] set and one with [`writable`] set.
    ///
    /// Note that the `timeout` will be rounded up to the system clock
    /// granularity (usually 1ms), and kernel scheduling delays mean that
    /// the blocking interval may be overrun by a small amount.
    ///
    /// `poll` returns the number of readiness events that have been pushed into
    /// `events` or `Err` when an error has been encountered with the system
    /// selector.
    ///
    /// See the [struct] level documentation for a higher level discussion of
    /// polling.
    ///
    /// [`readable`]: struct.Ready.html#method.readable
    /// [`writable`]: struct.Ready.html#method.writable
    /// [struct]: #
    ///
    /// # Examples
    ///
    /// A basic example -- establishing a `TcpStream` connection.
    ///
    /// ```
    /// use soio::{Events, Poll, Ready, PollOpt, Token};
    /// use soio::tcp::TcpStream;
    ///
    /// use std::net::{TcpListener, SocketAddr};
    /// use std::thread;
    ///
    /// // Bind a server socket to connect to.
    /// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /// let server = TcpListener::bind(&addr).unwrap();
    /// let addr = server.local_addr().unwrap().clone();
    ///
    /// // Spawn a thread to accept the socket
    /// thread::spawn(move || {
    ///     let _ = server.accept();
    /// });
    ///
    /// // Construct a new `Poll` handle as well as the `Events` we'll store into
    /// let poll = Poll::new().unwrap();
    /// let mut events = Events::with_capacity(1024);
    ///
    /// // Connect the stream
    /// let stream = TcpStream::connect(&addr).unwrap();
    ///
    /// // Register the stream with `Poll`
    /// poll.register(&stream, Token(0), Ready::readable() | Ready::writable(), PollOpt::edge()).unwrap();
    ///
    /// // Wait for the socket to become ready. This has to happens in a loop to
    /// // handle spurious wakeups.
    /// loop {
    ///     poll.poll(&mut events, None).unwrap();
    ///
    ///     for event in &events {
    ///         if event.token() == Token(0) && event.readiness().is_writable() {
    ///             // The socket connected (probably, it could still be a spurious
    ///             // wakeup)
    ///             return;
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// [struct]: #
    pub fn poll(&self, events: &mut Events, mut timeout: Option<Duration>) -> io::Result<usize> {
        let zero = Some(Duration::from_millis(0));

        // At a high level, the synchronization strategy is to acquire access to
        // the critical section by transitioning the atomic from unlocked ->
        // locked. If the attempt fails, the thread will wait on the condition
        // variable.
        //
        // # Some more detail
        //
        // The `lock_state` atomic usize combines:
        //
        // - locked flag, stored in the least significant bit
        // - number of waiting threads, stored in the rest of the bits.
        //
        // When a thread transitions the locked flag from 0 -> 1, it has
        // obtained access to the critical section.
        //
        // When entering `poll`, a compare-and-swap from 0 -> 1 is attempted.
        // This is a fast path for the case when there are no concurrent calls
        // to poll, which is very common.
        //
        // On failure, the mutex is locked, and the thread attempts to increment
        // the number of waiting threads component of `lock_state`. If this is
        // successfully done while the locked flag is set, then the thread can
        // wait on the condition variable.
        //
        // When a thread exits the critical section, it unsets the locked flag.
        // If there are any waiters, which is atomically determined while
        // unsetting the locked flag, then the condvar is notified.

        let mut curr = self.lock_state.compare_and_swap(0, 1, SeqCst);

        if 0 != curr {
            // Enter slower path
            let mut lock = self.lock.lock().unwrap();
            let mut inc = false;

            loop {
                if curr & 1 == 0 {
                    // The lock is currently free, attempt to grab it
                    let mut next = curr | 1;

                    if inc {
                        // The waiter count has previously been incremented, so
                        // decrement it here
                        next -= 2;
                    }

                    let actual = self.lock_state.compare_and_swap(curr, next, SeqCst);

                    if actual != curr {
                        curr = actual;
                        continue;
                    }

                    // Lock acquired, break from the loop
                    break;
                }

                if timeout == zero {
                    if inc {
                        self.lock_state.fetch_sub(2, SeqCst);
                    }

                    return Ok(0);
                }

                // The lock is currently held, so wait for it to become
                // free. If the waiter count hasn't been incremented yet, do
                // so now
                if !inc {
                    let next = curr.checked_add(2).expect("overflow");
                    let actual = self.lock_state.compare_and_swap(curr, next, SeqCst);

                    if actual != curr {
                        curr = actual;
                        continue;
                    }

                    // Track that the waiter count has been incremented for
                    // this thread and fall through to the condvar waiting
                    inc = true;
                }

                lock = match timeout {
                    Some(to) => {
                        let now = Instant::now();

                        // Wait to be notified
                        let (l, _) = self.condvar.wait_timeout(lock, to).unwrap();

                        // See how much time was elapsed in the wait
                        let elapsed = now.elapsed();

                        // Update `timeout` to reflect how much time is left to
                        // wait.
                        if elapsed >= to {
                            timeout = zero;
                        } else {
                            // Update the timeout
                            timeout = Some(to - elapsed);
                        }

                        l
                    }
                    None => {
                        self.condvar.wait(lock).unwrap()
                    }
                };

                // Reload the state
                curr = self.lock_state.load(SeqCst);

                // Try to lock again...
            }
        }

        let ret = self.poll2(events, timeout);

        // Release the lock
        if 1 != self.lock_state.fetch_and(!1, Release) {
            // Acquire the mutex
            let _lock = self.lock.lock().unwrap();

            // There is at least one waiting thread, so notify one
            self.condvar.notify_one();
        }

        ret
    }

    #[inline]
    fn poll2(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<usize> {
        // Compute the timeout value passed to the system selector. If the
        // readiness queue has pending nodes, we still want to poll the system
        // selector for new events, but we don't want to block the thread to
        // wait for new events.
        let timeout = if timeout == Some(Duration::from_millis(0)) {
            // If blocking is not requested, then there is no need to prepare
            // the queue for sleep
            timeout
        } else if self.readiness_queue.prepare_for_sleep() {
            // The readiness queue is empty. The call to `prepare_for_sleep`
            // inserts `sleep_marker` into the queue. This signals to any
            // threads setting readiness that the `Poll::poll` is going to
            // sleep, so the awakener should be used.
            timeout
        } else {
            // The readiness queue is not empty, so do not block the thread.
            Some(Duration::from_millis(0))
        };

        // First get selector events
        let res = self.selector.select(&mut events.inner, AWAKEN, timeout);

        if try!(res) {
            // Some awakeners require reading from a FD.
            self.readiness_queue.inner.awakener.cleanup();
        }

        // Poll custom event queue
        self.readiness_queue.poll(&mut events.inner);

        // Return number of polled events
        Ok(events.len())
    }
}

fn validate_args(token: Token, interest: Ready) -> io::Result<()> {
    if token == AWAKEN {
        return Err(io::Error::new(io::ErrorKind::Other, "invalid token"));
    }

    if !interest.is_readable() && !interest.is_writable() {
        return Err(io::Error::new(io::ErrorKind::Other, "interest must include readable or writable"));
    }

    Ok(())
}

impl fmt::Debug for Poll {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Poll")
    }
}

// ===== Accessors for internal usage =====

pub fn selector(poll: &Poll) -> &sys::Selector {
    &poll.selector
}

fn is_send<T: Send>() {}
fn is_sync<T: Sync>() {}