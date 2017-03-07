use {sys, Token, Ready};
/// A collection of readiness events.
///
/// `Events` is passed as an argument to [`Poll::poll`] and will be used to
/// receive any new readiness events received since the last call to [`poll`].
/// Usually, a single `Events` instance is created at the same time as the
/// [`Poll`] and the single instance is reused for each call to [`poll`].
///
/// See [`Poll`] for more documentation on polling.
///
/// # Examples
///
/// ```
/// use soio::{Events, Poll};
/// use std::time::Duration;
///
/// let mut events = Events::with_capacity(1024);
/// let poll = Poll::new().unwrap();
///
/// assert_eq!(0, events.len());
///
/// // Register `Evented` handles with `poll`
///
/// poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();
///
/// for event in &events {
///     println!("event={:?}", event);
/// }
/// ```
///
/// [`Poll::poll`]: struct.Poll.html#method.poll
/// [`poll`]: struct.Poll.html#method.poll
/// [`Poll`]: struct.Poll.html
pub struct Events {
    pub inner: sys::Events,
}

/// [`Events`] iterator.
///
/// This struct is created by the [`iter`] method on [`Events`].
///
/// # Examples
///
/// ```
/// use soio::{Events, Poll};
/// use std::time::Duration;
///
/// let mut events = Events::with_capacity(1024);
/// let poll = Poll::new().unwrap();
///
/// // Register handles with `poll`
///
/// poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();
///
/// for event in events.iter() {
///     println!("event={:?}", event);
/// }
/// ```
///
/// [`Events`]: struct.Events.html
/// [`iter`]: struct.Events.html#method.iter
pub struct Iter<'a> {
    inner: &'a Events,
    pos: usize,
}

impl Events {
    /// Return a new `Events` capable of holding up to `capacity` events.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Events;
    ///
    /// let events = Events::with_capacity(1024);
    ///
    /// assert_eq!(1024, events.capacity());
    /// ```
    pub fn with_capacity(capacity: usize) -> Events {
        Events {
            inner: sys::Events::with_capacity(capacity),
        }
    }

    /// Returns the `Event` at the given index, or `None` if the index is out of
    /// bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::{Events, Poll};
    /// use std::time::Duration;
    ///
    /// let mut events = Events::with_capacity(1024);
    /// let poll = Poll::new().unwrap();
    ///
    /// // Register handles with `poll`
    ///
    /// let n = poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();
    ///
    /// for i in 0..n {
    ///     println!("event={:?}", events.get(i).unwrap());
    /// }
    /// ```
    pub fn get(&self, idx: usize) -> Option<Event> {
        self.inner.get(idx)
    }

    /// Returns the number of `Event` values currently in `self`.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Events;
    ///
    /// let events = Events::with_capacity(1024);
    ///
    /// assert_eq!(0, events.len());
    /// ```
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns the number of `Event` values that `self` can hold.
    ///
    /// ```
    /// use soio::Events;
    ///
    /// let events = Events::with_capacity(1024);
    ///
    /// assert_eq!(1024, events.capacity());
    /// ```
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Returns `true` if `self` contains no `Event` values.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Events;
    ///
    /// let events = Events::with_capacity(1024);
    ///
    /// assert!(events.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns an iterator over the `Event` values.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::{Events, Poll};
    /// use std::time::Duration;
    ///
    /// let mut events = Events::with_capacity(1024);
    /// let poll = Poll::new().unwrap();
    ///
    /// // Register handles with `poll`
    ///
    /// poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();
    ///
    /// for event in events.iter() {
    ///     println!("event={:?}", event);
    /// }
    /// ```
    pub fn iter(&self) -> Iter {
        Iter {
            inner: self,
            pos: 0
        }
    }
}

impl<'a> IntoIterator for &'a Events {
    type Item = Event;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = Event;

    fn next(&mut self) -> Option<Event> {
        let ret = self.inner.get(self.pos);
        self.pos += 1;
        ret
    }
}


/// An readiness event returned by [`Poll::poll`].
///
/// `Event` is a [readiness state] paired with a [`Token`]. It is returned by
/// [`Poll::poll`].
///
/// For more documentation on polling and events, see [`Poll`].
///
/// # Examples
///
/// ```
/// use soio::{Event, Ready, Token};
///
/// let event = Event::new(Ready::readable() | Ready::writable(), Token(0));
///
/// assert_eq!(event.readiness(), Ready::readable() | Ready::writable());
/// assert_eq!(event.token(), Token(0));
/// ```
///
/// [`Poll::poll`]: struct.Poll.html#method.poll
/// [`Poll`]: struct.Poll.html
/// [readiness state ]: struct.Ready.html
/// [`Token`]: struct.Token.html
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Event {
    kind: Ready,
    token: Token
}

impl Event {
    /// Creates a new `Event` containing `readiness` and `token`
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::{Event, Ready, Token};
    ///
    /// let event = Event::new(Ready::readable() | Ready::writable(), Token(0));
    ///
    /// assert_eq!(event.readiness(), Ready::readable() | Ready::writable());
    /// assert_eq!(event.token(), Token(0));
    /// ```
    pub fn new(readiness: Ready, token: Token) -> Event {
        Event {
            kind: readiness,
            token: token,
        }
    }

    /// Returns the event's readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::{Event, Ready, Token};
    ///
    /// let event = Event::new(Ready::readable() | Ready::writable(), Token(0));
    ///
    /// assert_eq!(event.readiness(), Ready::readable() | Ready::writable());
    /// ```
    pub fn readiness(&self) -> Ready {
        self.kind
    }

    /// Returns the event's token.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::{Event, Ready, Token};
    ///
    /// let event = Event::new(Ready::readable() | Ready::writable(), Token(0));
    ///
    /// assert_eq!(event.token(), Token(0));
    /// ```
    pub fn token(&self) -> Token {
        self.token
    }
}
