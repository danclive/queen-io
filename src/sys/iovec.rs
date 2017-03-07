use std::{mem, slice};

use libc;

pub struct IoVec {
    inner: [u8],
}

pub fn as_os_slice<'a>(iov: &'a [&::IoVec]) -> &'a [libc::iovec] {
    unsafe { mem::transmute(iov) }
}

pub fn as_os_slice_mut<'a>(iov: &'a mut [&mut ::IoVec]) -> &'a mut [libc::iovec] {
    unsafe { mem::transmute(iov) }
}

impl IoVec {
    pub fn iovec(&self) -> libc::iovec {
        unsafe { mem::transmute(&self.inner) }
    }

    pub fn as_ref(&self) -> &[u8] {
        unsafe {
            let vec = self.iovec();
            slice::from_raw_parts(vec.iov_base as *const u8, vec.iov_len)
        }
    }

    pub fn as_mut(&self) -> &mut [u8] {
        unsafe {
            let vec = self.iovec();
            slice::from_raw_parts_mut(vec.iov_base as *mut u8, vec.iov_len)
        }
    }
}

impl<'a> From<&'a [u8]> for &'a IoVec {
    fn from(bytes: &'a [u8]) -> &'a IoVec {
        unsafe {
            mem::transmute(libc::iovec {
                iov_base: bytes.as_ptr() as *mut _,
                iov_len: bytes.len(),
            })
        }
    }
}

impl<'a> From<&'a mut [u8]> for &'a mut IoVec {
    fn from(bytes: &'a mut [u8]) -> &'a mut IoVec {
        unsafe {
            mem::transmute(libc::iovec {
                iov_base: bytes.as_ptr() as *mut _,
                iov_len: bytes.len(),
            })
        }
    }
}
