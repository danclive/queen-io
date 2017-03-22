use std::{ops, mem};

use sys;

/// A specialized byte slice type for performing vector reads and writes.
///
/// # Examples
///
/// ```
/// use soio::IoVec;
///
/// let mut data = vec![];
/// data.extend_from_slice(b"hello");
///
/// let iovec: &IoVec = data.as_slice().into();
///
/// assert_eq!(iovec.as_bytes(), &b"hello"[..]);
/// ```
pub struct IoVec {
    sys: sys::IoVec,
}

impl ops::Deref for IoVec {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.sys.as_ref()
    }
}

impl ops::DerefMut for IoVec {
    fn deref_mut(&mut self) -> &mut [u8] {
        self.sys.as_mut()
    }
}

impl<'a> From<&'a [u8]> for &'a IoVec {
    fn from(bytes: &'a [u8]) -> &'a IoVec {
        unsafe {
            mem::transmute(<&sys::IoVec>::from(bytes))
        }
    }
}

impl<'a> From<&'a mut [u8]> for &'a mut IoVec {
    fn from(bytes: &'a mut [u8]) -> &'a mut IoVec {
        unsafe {
            mem::transmute(<&mut sys::IoVec>::from(bytes))
        }
    }
}

impl IoVec {
    /// Converts an `self` to a bytes slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::IoVec;
    ///
    /// let mut data = vec![];
    /// data.extend_from_slice(b"hello");
    ///
    /// let iovec: &IoVec = data.as_slice().into();
    ///
    /// assert_eq!(iovec.as_bytes(), &b"hello"[..]);
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        //self.data.as_bytes()
        &**self
    }

    /// Converts an `self` to a mutable bytes slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use soio::IoVec;
    ///
    /// let mut data = vec![];
    /// data.extend_from_slice(b"hello");
    ///
    /// let iovec: &mut IoVec = data.as_mut_slice().into();
    ///
    /// iovec.as_mut_bytes()[0] = b'j';
    ///
    /// assert_eq!(iovec.as_bytes(), &b"jello"[..]);
    /// ```
    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        //self.data.as_mut_bytes()
        &mut **self
    }
}

impl<'a> Default for &'a IoVec {
    fn default() -> Self {
        let b: &[u8] = Default::default();
        b.into()
    }
}

impl<'a> Default for &'a mut IoVec {
    fn default() -> Self {
        let b: &mut [u8] = Default::default();
        b.into()
    }
}
