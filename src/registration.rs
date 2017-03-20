use std::{fmt, io, ptr, usize};
use std::cell::UnsafeCell;
use std::{mem, ops, isize};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicPtr, AtomicBool};
use std::sync::atomic::Ordering::{self, Acquire, Release, AcqRel, Relaxed};
use {sys, Poll, Token, Ready, PollOpt, Evented, Event};
/// Handle to a user space `Poll` registration.
///
/// `Registration` allows implementing [`Evented`] for types that cannot work
/// with the [system selector]. A `Registration` is always paired with a
/// `SetReadiness`, which allows updating the registration's readiness state.
/// When [`set_readiness`] is called and the `Registration` is associated with a
/// [`Poll`] instance, a readiness event will be created and eventually returned
/// by [`poll`].
///
/// A `Registration` / `SetReadiness` pair is created by calling
/// [`Registration::new2`]. At this point, the registration is not being
/// monitored by a [`Poll`] instance, so calls to `set_readiness` will not
/// result in any readiness notifications.
///
/// `Registration` implements [`Evented`], so it can be used with [`Poll`] using
/// the same [`register`], [`reregister`], and [`deregister`] functions used
/// with TCP, UDP, etc... types. Once registered with [`Poll`], readiness state
/// changes result in readiness events being dispatched to the [`Poll`] instance
/// with which `Registration` is registered.
///
/// **Note**, before using `Registration` be sure to read the
/// [`set_readiness`] documentation and the [portability] notes. The
/// guarantees offered by `Registration` may be weaker than expected.
///
/// For high level documentation, see [`Poll`].
///
/// # Examples
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
///
/// [system selector]: struct.Poll.html#implementation-notes
/// [`Poll`]: struct.Poll.html
/// [`Registration::new2`]: struct.Registration.html#method.new2
/// [`Evented`]: event/trait.Evented.html
pub struct Registration {
    inner: RegistrationInner,
}

unsafe impl Send for Registration {}
unsafe impl Sync for Registration {}

/// Updates the readiness state of the associated [`Registration`].
///
/// See [`Registration`] for more documentation on using `SetReadiness` and
/// [`Poll`] for high level polling documentation.
///
/// [`Registration`]
#[derive(Clone)]
pub struct SetReadiness {
    inner: RegistrationInner,
}

unsafe impl Send for SetReadiness {}
unsafe impl Sync for SetReadiness {}

struct RegistrationInner {
    // Unsafe pointer to the registration's node. The node is ref counted. This
    // cannot "simply" be tracked by an Arc because `Poll::poll` has an implicit
    // handle though it isn't stored anywhere. In other words, `Poll::poll`
    // needs to decrement the ref count before the node is freed.
    node: *mut ReadinessNode,
}

#[derive(Clone)]
pub struct ReadinessQueue {
    pub inner: Arc<ReadinessQueueInner>,
}

unsafe impl Send for ReadinessQueue {}
unsafe impl Sync for ReadinessQueue {}

pub struct ReadinessQueueInner {
    // Used to wake up `Poll` when readiness is set in another thread.
    pub awakener: sys::Awakener,

    // Head of the MPSC queue used to signal readiness to `Poll::poll`.
    head_readiness: AtomicPtr<ReadinessNode>,

    // Tail of the readiness queue.
    //
    // Only accessed by Poll::poll. Coordination will be handled by the poll fn
    tail_readiness: UnsafeCell<*mut ReadinessNode>,

    // Fake readiness node used to punctuate the end of the readiness queue.
    // Before attempting to read from the queue, this node is inserted in order
    // to partition the queue between nodes that are "owned" by the dequeue end
    // and nodes that will be pushed on by producers.
    end_marker: Box<ReadinessNode>,

    // Similar to `end_marker`, but this node signals to producers that `Poll`
    // has gone to sleep and must be woken up.
    sleep_marker: Box<ReadinessNode>,
}

/// Node shared by a `Registration` / `SetReadiness` pair as well as the node
/// queued into the MPSC channel.
struct ReadinessNode {
    // Node state, see struct docs for `ReadinessState`
    //
    // This variable is the primary point of coordination between all the
    // various threads concurrently accessing the node.
    state: AtomicState,

    // The registration token cannot fit into the `state` variable, so it is
    // broken out here. In order to atomically update both the state and token
    // we have to jump through a few hoops.
    //
    // First, `state` includes `token_read_pos` and `token_write_pos`. These can
    // either be 0, 1, or 2 which represent a token slot. `token_write_pos` is
    // the token slot that contains the most up to date registration token.
    // `token_read_pos` is the token slot that `poll` is currently reading from.
    //
    // When a call to `update` includes a different token than the one currently
    // associated with the registration (token_write_pos), first an unused token
    // slot is found. The unused slot is the one not represented by
    // `token_read_pos` OR `token_write_pos`. The new token is written to this
    // slot, then `state` is updated with the new `token_write_pos` value. This
    // requires that there is only a *single* concurrent call to `update`.
    //
    // When `poll` reads a node state, it checks that `token_read_pos` matches
    // `token_write_pos`. If they do not match, then it atomically updates
    // `state` such that `token_read_pos` is set to `token_write_pos`. It will
    // then read the token at the newly updated `token_read_pos`.
    token_0: UnsafeCell<Token>,
    token_1: UnsafeCell<Token>,
    token_2: UnsafeCell<Token>,

    // Used when the node is queued in the readiness linked list. Accessing
    // this field requires winning the "queue" lock
    next_readiness: AtomicPtr<ReadinessNode>,

    // Ensures that there is only one concurrent call to `update`.
    //
    // Each call to `update` will attempt to swap `update_lock` from `false` to
    // `true`. If the CAS succeeds, the thread has obtained the update lock. If
    // the CAS fails, then the `update` call returns immediately and the update
    // is discarded.
    update_lock: AtomicBool,

    // Pointer to Arc<ReadinessQueueInner>
    readiness_queue: AtomicPtr<()>,

    // Tracks the number of `ReadyRef` pointers
    ref_count: AtomicUsize,
}

/// Stores the ReadinessNode state in an AtomicUsize. This wrapper around the
/// atomic variable handles encoding / decoding `ReadinessState` values.
struct AtomicState {
    inner: AtomicUsize,
}

const MASK_2: usize = 4 - 1;
const MASK_4: usize = 16 - 1;
const QUEUED_MASK: usize = 1 << QUEUED_SHIFT;
const DROPPED_MASK: usize = 1 << DROPPED_SHIFT;

const READINESS_SHIFT: usize = 0;
const INTEREST_SHIFT: usize = 4;
const POLL_OPT_SHIFT: usize = 8;
const TOKEN_RD_SHIFT: usize = 12;
const TOKEN_WR_SHIFT: usize = 14;
const QUEUED_SHIFT: usize = 16;
const DROPPED_SHIFT: usize = 17;

/// Tracks all state for a single `ReadinessNode`. The state is packed into a
/// `usize` variable from low to high bit as follows:
///
/// 4 bits: Registration current readiness
/// 4 bits: Registration interest
/// 4 bits: Poll options
/// 2 bits: Token position currently being read from by `poll`
/// 2 bits: Token position last written to by `update`
/// 1 bit:  Queued flag, set when node is being pushed into MPSC queue.
/// 1 bit:  Dropped flag, set when all `Registration` handles have been dropped.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct ReadinessState(usize);

/// Returned by `dequeue_node`. Represents the different states as described by
/// the queue documentation on 1024cores.net.
enum Dequeue {
    Data(*mut ReadinessNode),
    Empty,
    Inconsistent,
}

const MAX_REFCOUNT: usize = (isize::MAX) as usize;

/*
 *
 * ===== Registration =====
 *
 */

impl Registration {
    /// Create and return a new `Registration` and the associated
    /// `SetReadiness`.
    ///
    /// See [struct] documentation for more detail and [`Poll`]
    /// for high level documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::{Events, Ready, Registration, Poll, PollOpt, Token};
    /// use std::thread;
    ///
    /// let (registration, set_readiness) = Registration::new();
    ///
    /// thread::spawn(move || {
    ///     use std::time::Duration;
    ///     thread::sleep(Duration::from_millis(500));
    ///
    ///     set_readiness.set_readiness(Ready::readable());
    /// });
    ///
    /// let poll = Poll::new().unwrap();
    /// poll.register(&registration, Token(0), Ready::readable() | Ready::writable(), PollOpt::edge()).unwrap();
    ///
    /// let mut events = Events::with_capacity(256);
    ///
    /// loop {
    ///     poll.poll(&mut events, None);
    ///
    ///     for event in &events {
    ///         if event.token() == Token(0) && event.readiness().is_readable() {
    ///             return;
    ///         }
    ///     }
    /// }
    ///
    /// ```
    /// [struct]: #
    /// [`Poll`]: struct.Poll.html
    pub fn new() -> (Registration, SetReadiness) {
        // Allocate the registration node. The new node will have `ref_count`
        // set to 2: one SetReadiness, one Registration.
        let node = Box::into_raw(Box::new(ReadinessNode::new(
                    ptr::null_mut(), Token(0), Ready::empty(), PollOpt::empty(), 2)));

        let registration = Registration {
            inner: RegistrationInner {
                node: node,
            },
        };

        let set_readiness = SetReadiness {
            inner: RegistrationInner {
                node: node,
            },
        };

        (registration, set_readiness)
    }

    /// Create and return a new `Registration` and the associated
    /// `SetReadiness`.
    ///
    /// See [struct] documentation for more detail and [`Poll`]
    /// for high level documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::{Events, Ready, Registration, Poll, PollOpt, Token};
    /// use std::thread;
    ///
    /// let poll = Poll::new().unwrap();
    ///
    /// let (registration, set_readiness) = Registration::new_prev(&poll, Token(123), Ready::readable() | Ready::writable(), PollOpt::edge());
    ///
    /// thread::spawn(move || {
    ///     use std::time::Duration;
    ///     thread::sleep(Duration::from_millis(500));
    ///
    ///     set_readiness.set_readiness(Ready::readable()).unwrap();
    /// });
    ///
    /// let mut events = Events::with_capacity(256);
    ///
    /// loop {
    ///     poll.poll(&mut events, None).unwrap();
    ///
    ///     for event in &events {
    ///         if event.token() == Token(123) && event.readiness().is_readable() {
    ///             return;
    ///         }
    ///     }
    /// }
    ///
    /// ```
    /// [struct]: #
    /// [`Poll`]: struct.Poll.html
    pub fn new_prev(poll: &Poll, token: Token, interest: Ready, opt: PollOpt)
        -> (Registration, SetReadiness)
    {
        // Clone handle to the readiness queue, this bumps the ref count
        let queue = poll.readiness_queue.clone();

        // Convert to a *mut () pointer
        let queue: *mut () = unsafe { mem::transmute(queue) };

        // Allocate the registration node. The new node will have `ref_count`
        // set to 3: one SetReadiness, one Registration, and one Poll handle.
        let node = Box::into_raw(Box::new(ReadinessNode::new(
                    queue, token, interest, opt, 3)));

        let registration = Registration {
            inner: RegistrationInner {
                node: node,
            },
        };

        let set_readiness = SetReadiness {
            inner: RegistrationInner {
                node: node,
            },
        };

        (registration, set_readiness)
    }
}

impl Evented for Registration {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.inner.update(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.inner.update(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.inner.update(poll, Token(0), Ready::empty(), PollOpt::empty())
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        // `flag_as_dropped` toggles the `dropped` flag and notifies
        // `Poll::poll` to release its handle (which is just decrementing
        // the ref count).
        if self.inner.state.flag_as_dropped() {
            // Can't do anything if the queuing fails
            let _ = self.inner.enqueue_with_wakeup();
        }
    }
}

impl fmt::Debug for Registration {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Registration")
            .finish()
    }
}

impl SetReadiness {
    /// Returns the registration's current readiness.
    ///
    /// # Note
    ///
    /// There is no guarantee that `readiness` establishes any sort of memory
    /// ordering. Any concurrent data access must be synchronized using another
    /// strategy.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::{Registration, Ready};
    ///
    /// let (registration, set_readiness) = Registration::new();
    ///
    /// assert!(set_readiness.readiness().is_empty());
    ///
    /// set_readiness.set_readiness(Ready::readable()).unwrap();
    /// assert!(set_readiness.readiness().is_readable());
    /// ```
    pub fn readiness(&self) -> Ready {
        self.inner.readiness()
    }

    /// Set the registration's readiness
    ///
    /// If the associated `Registration` is registered with a [`Poll`] instance
    /// and has requested readiness events that include `ready`, then a call
    /// [`poll`] will receive a readiness event representing the readiness
    /// state change.
    ///
    /// # Note
    ///
    /// There is no guarantee that `readiness` establishes any sort of memory
    /// ordering. Any concurrent data access must be synchronized using another
    /// strategy.
    ///
    /// There is also no guarantee as to when the readiness event will be
    /// delivered to poll. A best attempt will be made to make the delivery in a
    /// "timely" fashion. For example, the following is **not** guaranteed to
    /// work:
    ///
    /// ```
    /// use soio::{Events, Registration, Ready, Poll, PollOpt, Token};
    ///
    /// let poll = Poll::new().unwrap();
    /// let (registration, set_readiness) = Registration::new();
    ///
    /// poll.register(&registration,
    ///               Token(0),
    ///               Ready::readable(),
    ///               PollOpt::edge()).unwrap();
    ///
    /// // Set the readiness, then immediately poll to try to get the readiness
    /// // event
    /// set_readiness.set_readiness(Ready::readable()).unwrap();
    ///
    /// let mut events = Events::with_capacity(1024);
    /// poll.poll(&mut events, None).unwrap();
    ///
    /// // There is NO guarantee that the following will work. It is possible
    /// // that the readiness event will be delivered at a later time.
    /// let event = events.get(0).unwrap();
    /// assert_eq!(event.token(), Token(0));
    /// assert!(event.readiness().is_readable());
    /// ```
    ///
    /// # Examples
    ///
    /// A simple example, for a more elaborate example, see the [`Evented`]
    /// documentation.
    ///
    /// ```
    /// use soio::{Registration, Ready};
    ///
    /// let (registration, set_readiness) = Registration::new();
    ///
    /// assert!(set_readiness.readiness().is_empty());
    ///
    /// set_readiness.set_readiness(Ready::readable()).unwrap();
    /// assert!(set_readiness.readiness().is_readable());
    /// ```
    ///
    /// [`Registration`]: struct.Registration.html
    /// [`Poll`]: struct.Poll.html
    /// [`poll`]: struct.Poll.html#method.poll
    pub fn set_readiness(&self, ready: Ready) -> io::Result<()> {
        self.inner.set_readiness(ready)
    }
}

impl fmt::Debug for SetReadiness {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "SetReadiness")
    }
}

impl RegistrationInner {
    /// Get the registration's readiness.
    fn readiness(&self) -> Ready {
        self.state.load(Relaxed).readiness()
    }

    /// Set the registration's readiness.
    ///
    /// This function can be called concurrently by an arbitrary number of
    /// SetReadiness handles.
    fn set_readiness(&self, ready: Ready) -> io::Result<()> {
        // Load the current atomic state.
        let mut state = self.state.load(Acquire);
        let mut next;

        loop {
            next = state;

            if state.is_dropped() {
                // Node is dropped, no more notifications
                return Ok(());
            }

            // Update the readiness
            next.set_readiness(ready);

            // If the readiness is not blank, try to obtain permission to
            // push the node into the readiness queue.
            if !next.effective_readiness().is_empty() {
                next.set_queued();
            }

            let actual = self.state.compare_and_swap(state, next, AcqRel);

            if state == actual {
                break;
            }

            state = actual;
        }

        if !state.is_queued() && next.is_queued() {
            // We toggled the queued flag, making us responsible for queuing the
            // node in the MPSC readiness queue.
            try!(self.enqueue_with_wakeup());
        }

        Ok(())
    }

    /// Update the registration details associated with the node
    fn update(&self, poll: &Poll, token: Token, interest: Ready, opt: PollOpt) -> io::Result<()> {
        // First, ensure poll instances match
        //
        // Load the queue pointer, `Relaxed` is sufficient here as only the
        // pointer is being operated on. The actual memory is guaranteed to be
        // visible the `poll: &Poll` ref passed as an argument to the function.
        let mut queue = self.readiness_queue.load(Relaxed);
        let other: &*mut () = unsafe { mem::transmute(&poll.readiness_queue) };
        let other = *other;

        debug_assert!(mem::size_of::<ReadinessQueue>() == mem::size_of::<*mut ()>());

        if queue.is_null() {
            // Attempt to set the queue pointer. `Release` ordering synchronizes
            // with `Acquire` in `ensure_with_wakeup`.
            let actual = self.readiness_queue.compare_and_swap(
                queue, other, Release);

            if actual.is_null() {
                // The CAS succeeded, this means that the node's ref count
                // should be incremented to reflect that the `poll` function
                // effectively owns the node as well.
                //
                // `Relaxed` ordering used for the same reason as in
                // RegistrationInner::clone
                self.ref_count.fetch_add(1, Relaxed);

                // Note that the `queue` reference stored in our
                // `readiness_queue` field is intended to be a strong reference,
                // so now that we've successfully claimed the reference we bump
                // the refcount here.
                //
                // Down below in `release_node` when we deallocate this
                // `RegistrationInner` is where we'll transmute this back to an
                // arc and decrement the reference count.
                mem::forget(poll.readiness_queue.clone());
            } else {
                // The CAS failed, another thread set the queue pointer, so ensure
                // that the pointer and `other` match
                if actual != other {
                    return Err(io::Error::new(io::ErrorKind::Other, "registration handle associated with another `Poll` instance"));
                }
            }

            queue = other;
        } else if queue != other {
            return Err(io::Error::new(io::ErrorKind::Other, "registration handle associated with another `Poll` instance"));
        }

        unsafe {
            let actual = &poll.readiness_queue.inner as *const _ as *const usize;
            debug_assert_eq!(queue as usize, *actual);
        }

        // The `update_lock` atomic is used as a flag ensuring only a single
        // thread concurrently enters the `update` critical section. Any
        // concurrent calls to update are discarded. If coordinated updates are
        // required, the soio user is responsible for handling that.
        //
        // Acquire / Release ordering is used on `update_lock` to ensure that
        // data access to the `token_*` variables are scoped to the critical
        // section.

        // Acquire the update lock.
        if self.update_lock.compare_and_swap(false, true, Acquire) {
            // The lock is already held. Discard the update
            return Ok(());
        }

        // Relaxed ordering is acceptable here as the only memory that needs to
        // be visible as part of the update are the `token_*` variables, and
        // ordering has already been handled by the `update_lock` access.
        let mut state = self.state.load(Relaxed);
        let mut next;

        // Read the current token, again this memory has been ordered by the
        // acquire on `update_lock`.
        let curr_token_pos = state.token_write_pos();
        let curr_token = unsafe { self::token(self, curr_token_pos) };

        let mut next_token_pos = curr_token_pos;

        // If the `update` call is changing the token, then compute the next
        // available token slot and write the token there.
        //
        // Note that this computation is happening *outside* of the
        // compare-and-swap loop. The update lock ensures that only a single
        // thread could be mutating the write_token_position, so the
        // `next_token_pos` will never need to be recomputed even if
        // `token_read_pos` concurrently changes. This is because
        // `token_read_pos` can ONLY concurrently change to the current value of
        // `token_write_pos`, so `next_token_pos` will always remain valid.
        if token != curr_token {
            next_token_pos = state.next_token_pos();

            // Update the token
            match next_token_pos {
                0 => unsafe { *self.token_0.get() = token },
                1 => unsafe { *self.token_1.get() = token },
                2 => unsafe { *self.token_2.get() = token },
                _ => unreachable!(),
            }
        }

        // Now enter the compare-and-swap loop
        loop {
            next = state;

            // The node is only dropped once all `Registration` handles are
            // dropped. Only `Registration` can call `update`.
            debug_assert!(!state.is_dropped());

            // Update the write token position, this will also release the token
            // to Poll::poll.
            next.set_token_write_pos(next_token_pos);

            // Update readiness and poll opts
            next.set_interest(interest);
            next.set_poll_opt(opt);

            // If there is effective readiness, the node will need to be queued
            // for processing. This exact behavior is still TBD, so we are
            // conservative for now and always fire.
            //
            // See https://github.com/carllerche/soio/issues/535.
            if !next.effective_readiness().is_empty() {
                next.set_queued();
            }

            // compare-and-swap the state values. Only `Release` is needed here.
            // The `Release` ensures that `Poll::poll` will see the token
            // update and the update function doesn't care about any other
            // memory visibility.
            let actual = self.state.compare_and_swap(state, next, Release);

            if actual == state {
                break;
            }

            // CAS failed, but `curr_token_pos` should not have changed given
            // that we still hold the update lock.
            debug_assert_eq!(curr_token_pos, actual.token_write_pos());

            state = actual;
        }

        // Release the lock
        self.update_lock.store(false, Release);

        if !state.is_queued() && next.is_queued() {
            // We are responsible for enqueing the node.
            try!(enqueue_with_wakeup(queue, self));
        }

        Ok(())
    }
}

impl ops::Deref for RegistrationInner {
    type Target = ReadinessNode;

    fn deref(&self) -> &ReadinessNode {
        unsafe { &*self.node }
    }
}

impl Clone for RegistrationInner {
    fn clone(&self) -> RegistrationInner {
        // Using a relaxed ordering is alright here, as knowledge of the
        // original reference prevents other threads from erroneously deleting
        // the object.
        //
        // As explained in the [Boost documentation][1], Increasing the
        // reference counter can always be done with memory_order_relaxed: New
        // references to an object can only be formed from an existing
        // reference, and passing an existing reference from one thread to
        // another must already provide any required synchronization.
        //
        // [1]: (www.boost.org/doc/libs/1_55_0/doc/html/atomic/usage_examples.html)
        let old_size = self.ref_count.fetch_add(1, Relaxed);

        // However we need to guard against massive refcounts in case someone
        // is `mem::forget`ing Arcs. If we don't do this the count can overflow
        // and users will use-after free. We racily saturate to `isize::MAX` on
        // the assumption that there aren't ~2 billion threads incrementing
        // the reference count at once. This branch will never be taken in
        // any realistic program.
        //
        // We abort because such a program is incredibly degenerate, and we
        // don't care to support it.
        if old_size & !MAX_REFCOUNT != 0 {
            // TODO: This should really abort the process
            panic!();
        }

        RegistrationInner {
            node: self.node.clone(),
        }
    }
}

impl Drop for RegistrationInner {
    fn drop(&mut self) {
        // Only handles releasing from `Registration` and `SetReadiness`
        // handles. Poll has to call this itself.
        release_node(self.node);
    }
}

/*
 *
 * ===== ReadinessQueue =====
 *
 */

impl ReadinessQueue {
    /// Create a new `ReadinessQueue`.
    pub fn new() -> io::Result<ReadinessQueue> {

        let end_marker = Box::new(ReadinessNode::marker());
        let sleep_marker = Box::new(ReadinessNode::marker());

        let ptr = &*end_marker as *const _ as *mut _;

        Ok(ReadinessQueue {
            inner: Arc::new(ReadinessQueueInner {
                awakener: try!(sys::Awakener::new()),
                head_readiness: AtomicPtr::new(ptr),
                tail_readiness: UnsafeCell::new(ptr),
                end_marker: end_marker,
                sleep_marker: sleep_marker,
            })
        })
    }

    /// Poll the queue for new events
    pub fn poll(&self, dst: &mut sys::Events) {
        // `until` is set with the first node that gets re-enqueued due to being
        // set to have level-triggered notifications. This prevents an infinite
        // loop where `Poll::poll` will keep dequeuing nodes it enqueues.
        let mut until = ptr::null_mut();

        'outer:
        while dst.len() < dst.capacity() {
            // Dequeue a node. If the queue is in an inconsistent state, then
            // stop polling. `Poll::poll` will be called again shortly and enter
            // a syscall, which should be enough to enable the other thread to
            // finish the queuing process.
            let ptr = match unsafe { self.inner.dequeue_node(until) } {
                Dequeue::Empty | Dequeue::Inconsistent => break,
                Dequeue::Data(ptr) => ptr,
            };

            let node = unsafe { &*ptr };

            // Read the node state with Acquire ordering. This allows reading
            // the token variables.
            let mut state = node.state.load(Acquire);
            let mut next;
            let mut readiness;
            let mut opt;

            loop {
                // Build up any changes to the readiness node's state and
                // attempt the CAS at the end
                next = state;

                // Given that the node was just read from the queue, the
                // `queued` flag should still be set.
                debug_assert!(state.is_queued());

                // The dropped flag means we need to release the node and
                // perform no further processing on it.
                if state.is_dropped() {
                    // Release the node and continue
                    release_node(ptr);
                    continue 'outer;
                }

                // Process the node
                readiness = state.effective_readiness();
                opt = state.poll_opt();

                if opt.is_edge() {
                    // Mark the node as dequeued
                    next.set_dequeued();

                    if opt.is_oneshot() && !readiness.is_empty() {
                        next.disarm();
                    }
                } else if readiness.is_empty() {
                    next.set_dequeued();
                }

                // Ensure `token_read_pos` is set to `token_write_pos` so that
                // we read the most up to date token value.
                next.update_token_read_pos();

                if state == next {
                    break;
                }

                let actual = node.state.compare_and_swap(state, next, AcqRel);

                if actual == state {
                    break;
                }

                state = actual;
            }

            // If the queued flag is still set, then the node must be requeued.
            // This typically happens when using level-triggered notifications.
            if next.is_queued() {
                if until.is_null() {
                    // We never want to see the node again
                    until = ptr;
                }

                // Requeue the node
                self.inner.enqueue_node(node);
            }

            if !readiness.is_empty() {
                // Get the token
                let token = unsafe { token(node, next.token_read_pos()) };

                // Push the event
                dst.push_event(Event::new(readiness, token));
            }
        }
    }

    fn wakeup(&self) -> io::Result<()> {
        self.inner.awakener.wakeup()
    }

    /// Prepend the given node to the head of the readiness queue. This is done
    /// with relaxed ordering. Returns true if `Poll` needs to be woken up.
    fn enqueue_node_with_wakeup(&self, node: &ReadinessNode) -> io::Result<()> {
        if self.inner.enqueue_node(node) {
            try!(self.wakeup());
        }

        Ok(())
    }

    /// Prepare the queue for the `Poll::poll` thread to block in the system
    /// selector. This involves changing `head_readiness` to `sleep_marker`.
    /// Returns true if successfull and `poll` can block.
    pub fn prepare_for_sleep(&self) -> bool {
        let end_marker = self.inner.end_marker();
        let sleep_marker = self.inner.sleep_marker();

        let tail = unsafe { *self.inner.tail_readiness.get() };

        // If the tail is currently set to the sleep_marker, then check if the
        // head is as well. If it is, then the queue is currently ready to
        // sleep. If it is not, then the queue is not empty and there should be
        // no sleeping.
        if tail == sleep_marker {
            return self.inner.head_readiness.load(Acquire) == sleep_marker;
        }

        // If the tail is not currently set to `end_marker`, then the queue is
        // not empty.
        if tail != end_marker {
            return false;
        }

        self.inner.sleep_marker.next_readiness.store(ptr::null_mut(), Relaxed);

        let actual = self.inner.head_readiness.compare_and_swap(
            end_marker, sleep_marker, AcqRel);

        debug_assert!(actual != sleep_marker);

        if actual != end_marker {
            // The readiness queue is not empty
            return false;
        }

        // The current tail should be pointing to `end_marker`
        debug_assert!(unsafe { *self.inner.tail_readiness.get() == end_marker });
        // The `end_marker` next pointer should be null
        debug_assert!(self.inner.end_marker.next_readiness.load(Relaxed).is_null());

        // Update tail pointer.
        unsafe { *self.inner.tail_readiness.get() = sleep_marker; }
        true
    }
}

impl ReadinessQueueInner {
    /// Push the node into the readiness queue
    fn enqueue_node(&self, node: &ReadinessNode) -> bool {
        // This is the 1024cores.net intrusive MPSC queue [1] "push" function.
        let node_ptr = node as *const _ as *mut _;

        // Relaxed used as the ordering is "released" when swapping
        // `head_readiness`
        node.next_readiness.store(ptr::null_mut(), Relaxed);

        unsafe {
            let prev = self.head_readiness.swap(node_ptr, AcqRel);

            debug_assert!((*prev).next_readiness.load(Relaxed).is_null());

            (*prev).next_readiness.store(node_ptr, Release);

            prev == self.sleep_marker()
        }
    }

    /// Must only be called in `poll` or `drop`
    unsafe fn dequeue_node(&self, until: *mut ReadinessNode) -> Dequeue {
        // This is the 1024cores.net intrusive MPSC queue [1] "pop" function
        // with the modifications mentioned at the top of the file.
        let mut tail = *self.tail_readiness.get();
        let mut next = (*tail).next_readiness.load(Acquire);

        if tail == self.end_marker() || tail == self.sleep_marker() {
            if next.is_null() {
                return Dequeue::Empty;
            }

            *self.tail_readiness.get() = next;
            tail = next;
            next = (*next).next_readiness.load(Acquire);
        }

        // Only need to check `until` at this point. `until` is either null,
        // which will never match tail OR it is a node that was pushed by
        // the current thread. This means that either:
        //
        // 1) The queue is inconsistent, which is handled explicitly
        // 2) We encounter `until` at this point in dequeue
        // 3) we will pop a different node
        if tail == until {
            return Dequeue::Empty;
        }

        if !next.is_null() {
            *self.tail_readiness.get() = next;
            return Dequeue::Data(tail);
        }

        if self.head_readiness.load(Acquire) != tail {
            return Dequeue::Inconsistent;
        }

        // Push the stub node
        self.enqueue_node(&*self.end_marker);

        next = (*tail).next_readiness.load(Acquire);

        if !next.is_null() {
            *self.tail_readiness.get() = next;
            return Dequeue::Data(tail);
        }

        Dequeue::Inconsistent
    }

    fn end_marker(&self) -> *mut ReadinessNode {
        &*self.end_marker as *const ReadinessNode as *mut ReadinessNode
    }

    fn sleep_marker(&self) -> *mut ReadinessNode {
        &*self.sleep_marker as *const ReadinessNode as *mut ReadinessNode
    }
}

impl Drop for ReadinessQueueInner {
    fn drop(&mut self) {
        loop {
            // Free any nodes that happen to be left in the readiness queue
            let ptr = match unsafe { self.dequeue_node(ptr::null_mut()) } {
                Dequeue::Empty => break,
                Dequeue::Inconsistent => {
                    // This really shouldn't be possible as all other handles to
                    // `ReadinessQueueInner` are dropped, but handle this by
                    // spinning I guess?
                    continue;
                }
                Dequeue::Data(ptr) => ptr,
            };

            let node = unsafe { &*ptr };

            let state = node.state.load(Acquire);

            debug_assert!(state.is_queued());
            debug_assert!(state.is_dropped());

            release_node(ptr);
        }
    }
}

impl ReadinessNode {
    /// Return a new `ReadinessNode`, initialized with a ref_count of 3.
    fn new(queue: *mut (),
           token: Token,
           interest: Ready,
           opt: PollOpt,
           ref_count: usize) -> ReadinessNode
    {
        ReadinessNode {
            state: AtomicState::new(interest, opt),
            // Only the first token is set, the others are initialized to 0
            token_0: UnsafeCell::new(token),
            token_1: UnsafeCell::new(Token(0)),
            token_2: UnsafeCell::new(Token(0)),
            next_readiness: AtomicPtr::new(ptr::null_mut()),
            update_lock: AtomicBool::new(false),
            readiness_queue: AtomicPtr::new(queue),
            ref_count: AtomicUsize::new(ref_count),
        }
    }

    fn marker() -> ReadinessNode {
        ReadinessNode {
            state: AtomicState::new(Ready::empty(), PollOpt::empty()),
            token_0: UnsafeCell::new(Token(0)),
            token_1: UnsafeCell::new(Token(0)),
            token_2: UnsafeCell::new(Token(0)),
            next_readiness: AtomicPtr::new(ptr::null_mut()),
            update_lock: AtomicBool::new(false),
            readiness_queue: AtomicPtr::new(ptr::null_mut()),
            ref_count: AtomicUsize::new(0),
        }
    }

    fn enqueue_with_wakeup(&self) -> io::Result<()> {
        let queue = self.readiness_queue.load(Acquire);

        if queue.is_null() {
            // Not associated with a queue, nothing to do
            return Ok(());
        }

        enqueue_with_wakeup(queue, self)
    }
}

fn enqueue_with_wakeup(queue: *mut (), node: &ReadinessNode) -> io::Result<()> {
    debug_assert!(!queue.is_null());
    // This is ugly... but we don't want to bump the ref count.
    let queue: &ReadinessQueue = unsafe { mem::transmute(&queue) };
    queue.enqueue_node_with_wakeup(node)
}

unsafe fn token(node: &ReadinessNode, pos: usize) -> Token {
    match pos {
        0 => *node.token_0.get(),
        1 => *node.token_1.get(),
        2 => *node.token_2.get(),
        _ => unreachable!(),
    }
}

fn release_node(ptr: *mut ReadinessNode) {
    unsafe {
        // `AcqRel` synchronizes with other `release_node` functions and ensures
        // that the drop happens after any reads / writes on other threads.
        if (*ptr).ref_count.fetch_sub(1, AcqRel) != 1 {
            return;
        }

        let node = Box::from_raw(ptr);

        // Decrement the readiness_queue Arc
        let queue = node.readiness_queue.load(Acquire);

        if queue.is_null() {
            return;
        }

        let _: Arc<ReadinessQueueInner> = mem::transmute(queue);
    }
}

impl AtomicState {
    fn new(interest: Ready, opt: PollOpt) -> AtomicState {
        let state = ReadinessState::new(interest, opt);

        AtomicState {
            inner: AtomicUsize::new(state.into()),
        }
    }

    /// Loads the current `ReadinessState`
    fn load(&self, order: Ordering) -> ReadinessState {
        self.inner.load(order).into()
    }

    /// Stores a state if the current state is the same as `current`.
    fn compare_and_swap(&self, current: ReadinessState, new: ReadinessState, order: Ordering) -> ReadinessState {
        self.inner.compare_and_swap(current.into(), new.into(), order).into()
    }

    // Returns `true` if the node should be queued
    fn flag_as_dropped(&self) -> bool {
        let prev: ReadinessState = self.inner.fetch_or(DROPPED_MASK | QUEUED_MASK, Release).into();
        // The flag should not have been previously set
        debug_assert!(!prev.is_dropped());

        !prev.is_queued()
    }
}

impl ReadinessState {
    // Create a `ReadinessState` initialized with the provided arguments
    #[inline]
    fn new(interest: Ready, opt: PollOpt) -> ReadinessState {
        let interest = interest.as_usize();
        let opt = opt.as_usize();

        debug_assert!(interest <= MASK_4);
        debug_assert!(opt <= MASK_4);

        let mut val = interest << INTEREST_SHIFT;
        val |= opt << POLL_OPT_SHIFT;

        ReadinessState(val)
    }

    #[inline]
    fn get(&self, mask: usize, shift: usize) -> usize{
        (self.0 >> shift) & mask
    }

    #[inline]
    fn set(&mut self, val: usize, mask: usize, shift: usize) {
        self.0 = (self.0 & !(mask << shift)) | (val << shift)
    }

    /// Get the readiness
    #[inline]
    fn readiness(&self) -> Ready {
        let v = self.get(MASK_4, READINESS_SHIFT);
        Ready::from(v)
    }

    #[inline]
    fn effective_readiness(&self) -> Ready {
        self.readiness() & self.interest()
    }

    /// Set the readiness
    #[inline]
    fn set_readiness(&mut self, v: Ready) {
        self.set(v.as_usize(), MASK_4, READINESS_SHIFT);
    }

    /// Get the interest
    #[inline]
    fn interest(&self) -> Ready {
        let v = self.get(MASK_4, INTEREST_SHIFT);
        Ready::from(v)
    }

    /// Set the interest
    #[inline]
    fn set_interest(&mut self, v: Ready) {
        self.set(v.as_usize(), MASK_4, INTEREST_SHIFT);
    }

    #[inline]
    fn disarm(&mut self) {
        self.set_interest(Ready::empty());
    }

    /// Get the poll options
    #[inline]
    fn poll_opt(&self) -> PollOpt {
        let v = self.get(MASK_4, POLL_OPT_SHIFT);
        PollOpt::from(v)
    }

    /// Set the poll options
    #[inline]
    fn set_poll_opt(&mut self, v: PollOpt) {
        self.set(v.as_usize(), MASK_4, POLL_OPT_SHIFT);
    }

    #[inline]
    fn is_queued(&self) -> bool {
        self.0 & QUEUED_MASK == QUEUED_MASK
    }

    /// Set the queued flag
    #[inline]
    fn set_queued(&mut self) {
        // Dropped nodes should never be queued
        debug_assert!(!self.is_dropped());
        self.0 |= QUEUED_MASK;
    }

    #[inline]
    fn set_dequeued(&mut self) {
        debug_assert!(self.is_queued());
        self.0 &= !QUEUED_MASK
    }

    #[inline]
    fn is_dropped(&self) -> bool {
        self.0 & DROPPED_MASK == DROPPED_MASK
    }

    #[inline]
    fn token_read_pos(&self) -> usize {
        self.get(MASK_2, TOKEN_RD_SHIFT)
    }

    #[inline]
    fn token_write_pos(&self) -> usize {
        self.get(MASK_2, TOKEN_WR_SHIFT)
    }

    #[inline]
    fn next_token_pos(&self) -> usize {
        let rd = self.token_read_pos();
        let wr = self.token_write_pos();

        match wr {
            0 => {
                match rd {
                    1 => 2,
                    2 => 1,
                    0 => 1,
                    _ => unreachable!(),
                }
            }
            1 => {
                match rd {
                    0 => 2,
                    2 => 0,
                    1 => 2,
                    _ => unreachable!(),
                }
            }
            2 => {
                match rd {
                    0 => 1,
                    1 => 0,
                    2 => 0,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }

    #[inline]
    fn set_token_write_pos(&mut self, val: usize) {
        self.set(val, MASK_2, TOKEN_WR_SHIFT);
    }

    #[inline]
    fn update_token_read_pos(&mut self) {
        let val = self.token_write_pos();
        self.set(val, MASK_2, TOKEN_RD_SHIFT);
    }
}

impl From<ReadinessState> for usize {
    fn from(src: ReadinessState) -> usize {
        src.0
    }
}

impl From<usize> for ReadinessState {
    fn from(src: usize) -> ReadinessState {
        ReadinessState(src)
    }
}
