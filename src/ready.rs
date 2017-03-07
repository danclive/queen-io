use std::{fmt, ops};
/// A set of readiness events
///
/// `Ready` is a set of operation descriptors indicating that an operation is
/// ready to be performed. For example, `Ready::readable()` indicates that the
/// associated `Evented` handle is ready to perform a `read` operation.
///
/// **Note that only readable and writable readiness is guaranteed to be
/// supported on all platforms**. This means that `error` and `hup` readiness
/// should be treated as hints. For more details, see [readiness] in the poll
/// documentation.
///
/// `Ready` values can be combined together using the various bitwise operators.
///
/// For high level documentation on polling and readiness, see [`Poll`].
///
/// # Examples
///
/// ```
/// use soio::Ready;
///
/// let ready = Ready::readable() | Ready::writable();
///
/// assert!(ready.is_readable());
/// assert!(ready.is_writable());
/// ```
///
/// [`Poll`]: struct.Poll.html
/// [`readable`]: #method.readable
/// [`writable`]: #method.writable
/// [readiness]: struct.Poll.html#readiness-operations
#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct Ready(usize);

const READABLE: usize = 0b0001;
const WRITABLE: usize = 0b0010;
const ERROR: usize    = 0b0100;
const HUP: usize      = 0b1000;
const READY_ALL: usize = READABLE | WRITABLE | ERROR | HUP;

impl Ready {
    /// Returns the empty `Ready` set.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let ready = Ready::empty();
    ///
    /// assert!(!ready.is_readable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    pub fn empty() -> Ready {
        Ready(0)
    }

    /// Returns a `Ready` representing readable readiness.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let ready = Ready::readable();
    ///
    /// assert!(ready.is_readable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn readable() -> Ready {
        Ready(READABLE)
    }

    /// Returns a `Ready` representing writable readiness.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let ready = Ready::writable();
    ///
    /// assert!(ready.is_writable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn writable() -> Ready {
        Ready(WRITABLE)
    }

    /// Returns a `Ready` representing error readiness.
    ///
    /// **Note that only readable and writable readiness is guaranteed to be
    /// supported on all platforms**. This means that `error` readiness
    /// should be treated as a hint. For more details, see [readiness] in the
    /// poll documentation.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let ready = Ready::error();
    ///
    /// assert!(ready.is_error());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    /// [readiness]: struct.Poll.html#readiness-operations
    #[inline]
    pub fn error() -> Ready {
        Ready(ERROR)
    }

    /// Returns a `Ready` representing HUP readiness.
    ///
    /// A HUP (or hang-up) signifies that a stream socket **peer** closed the
    /// connection, or shut down the writing half of the connection.
    ///
    /// **Note that only readable and writable readiness is guaranteed to be
    /// supported on all platforms**. This means that `hup` readiness
    /// should be treated as a hint. For more details, see [readiness] in the
    /// poll documentation.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let ready = Ready::hup();
    ///
    /// assert!(ready.is_hup());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    /// [readiness]: struct.Poll.html#readiness-operations
    #[inline]
    pub fn hup() -> Ready {
        Ready(HUP)
    }

    /// Returns true if `Ready` is the empty set
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let ready = Ready::empty();
    /// assert!(ready.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        *self == Ready::empty()
    }

    /// Returns true if the value includes readable readiness
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let ready = Ready::readable();
    ///
    /// assert!(ready.is_readable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_readable(&self) -> bool {
        self.contains(Ready::readable())
    }

    /// Returns true if the value includes writable readiness
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let ready = Ready::writable();
    ///
    /// assert!(ready.is_writable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_writable(&self) -> bool {
        self.contains(Ready::writable())
    }

    /// Returns true if the value includes error readiness
    ///
    /// **Note that only readable and writable readiness is guaranteed to be
    /// supported on all platforms**. This means that `error` readiness should
    /// be treated as a hint. For more details, see [readiness] in the poll
    /// documentation.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let ready = Ready::error();
    ///
    /// assert!(ready.is_error());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_error(&self) -> bool {
        self.contains(Ready(ERROR))
    }

    /// Returns true if the value includes HUP readiness
    ///
    /// A HUP (or hang-up) signifies that a stream socket **peer** closed the
    /// connection, or shut down the writing half of the connection.
    ///
    /// **Note that only readable and writable readiness is guaranteed to be
    /// supported on all platforms**. This means that `hup` readiness
    /// should be treated as a hint. For more details, see [readiness] in the
    /// poll documentation.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let ready = Ready::hup();
    ///
    /// assert!(ready.is_hup());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_hup(&self) -> bool {
        self.contains(Ready(HUP))
    }

    /// Adds all readiness represented by `other` into `self`.
    ///
    /// This is equivalent to `*self = *self | other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let mut readiness = Ready::empty();
    /// readiness.insert(Ready::readable());
    ///
    /// assert!(readiness.is_readable());
    /// ```
    #[inline]
    pub fn insert(&mut self, other: Ready) {
        self.0 |= other.0;
    }

    /// Removes all options represented by `other` from `self`.
    ///
    /// This is equivalent to `*self = *self & !other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let mut readiness = Ready::readable();
    /// readiness.remove(Ready::readable());
    ///
    /// assert!(!readiness.is_readable());
    /// ```
    #[inline]
    pub fn remove(&mut self, other: Ready) {
        self.0 &= !other.0;
    }

    /// Returns true if `self` is a superset of `other`.
    ///
    /// `other` may represent more than one readiness operations, in which case
    /// the function only returns true if `self` contains all readiness
    /// specified in `other`.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let readiness = Ready::readable();
    ///
    /// assert!(readiness.contains(Ready::readable()));
    /// assert!(!readiness.contains(Ready::writable()));
    /// ```
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let readiness = Ready::readable() | Ready::writable();
    ///
    /// assert!(readiness.contains(Ready::readable()));
    /// assert!(readiness.contains(Ready::writable()));
    /// ```
    ///
    /// ```
    /// use soio::Ready;
    ///
    /// let readiness = Ready::readable() | Ready::writable();
    ///
    /// assert!(!Ready::readable().contains(readiness));
    /// assert!(readiness.contains(readiness));
    /// assert!((readiness | Ready::hup()).contains(readiness));
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn contains(&self, other: Ready) -> bool {
        (*self & other) == other
    }

    pub fn as_usize(&self) -> usize {
        self.0
    }
}

impl ops::BitOr for Ready {
    type Output = Ready;

    #[inline]
    fn bitor(self, other: Ready) -> Ready {
        Ready(self.0 | other.0)
    }
}

impl ops::BitXor for Ready {
    type Output = Ready;

    #[inline]
    fn bitxor(self, other: Ready) -> Ready {
        Ready(self.0 ^ other.0)
    }
}

impl ops::BitAnd for Ready {
    type Output = Ready;

    #[inline]
    fn bitand(self, other: Ready) -> Ready {
        Ready(self.0 & other.0)
    }
}

impl ops::Sub for Ready {
    type Output = Ready;

    #[inline]
    fn sub(self, other: Ready) -> Ready {
        Ready(self.0 & !other.0)
    }
}

impl ops::Not for Ready {
    type Output = Ready;

    #[inline]
    fn not(self) -> Ready {
        Ready(!self.0 & READY_ALL)
    }
}

impl From<usize> for Ready {
    fn from(event: usize) -> Ready {
        Ready(event)
    }
}

impl fmt::Debug for Ready {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (Ready::readable(), "Readable"),
            (Ready::writable(), "Writable"),
            (Ready(ERROR), "Error"),
            (Ready(HUP), "Hup")];

        try!(write!(fmt, "Ready {{"));

        for &(flag, msg) in &flags {
            if self.contains(flag) {
                if one { try!(write!(fmt, " | ")) }
                try!(write!(fmt, "{}", msg));

                one = true
            }
        }

        try!(write!(fmt, "}}"));

        Ok(())
    }
}
