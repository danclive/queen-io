use std::io;
use std::mem;
use std::os::unix::io::RawFd;

use libc::{self, c_int, c_void};

pub fn setsockopt<T>(fd: RawFd, opt: c_int, val: c_int,
                     payload: T) -> io::Result<()> {

    let payload = &payload as *const T as *const c_void;
    syscall!(setsockopt(fd, opt, val, payload,
                          mem::size_of::<T>() as libc::socklen_t))?;
    Ok(())
}

pub fn getsockopt<T: Copy>(fd: RawFd, opt: c_int,
                       val: c_int) -> io::Result<T> {
    let mut slot: T = unsafe { mem::zeroed() };
    let mut len = mem::size_of::<T>() as libc::socklen_t;
    syscall!(getsockopt(fd, opt, val,
                    &mut slot as *mut _ as *mut _,
                    &mut len))?;
    assert_eq!(len as usize, mem::size_of::<T>());
    Ok(slot)
}
