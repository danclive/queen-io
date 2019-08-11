use std::os::unix::io::{RawFd, AsRawFd, FromRawFd, IntoRawFd};
use std::time::Duration;
use std::mem;
use super::io::{self, Io, Read};
use super::cvt;
use crate::{Epoll, Token, Ready, EpollOpt, Evented};

#[repr(i32)]
pub enum Clock {
    Realtime = libc::CLOCK_REALTIME,
    Monotonic = libc::CLOCK_MONOTONIC,
    Boottime = libc::CLOCK_BOOTTIME,
    RealtimeAlarm = libc::CLOCK_REALTIME_ALARM,
    BoottimeAlarm = libc::CLOCK_BOOTTIME_ALARM
}

pub const TFD_CLOEXEC: i32 = libc::TFD_CLOEXEC;
pub const TFD_NONBLOCK: i32 = libc::TFD_NONBLOCK;

#[derive(Debug)]
pub struct TimerFd {
    inner: Io
}

#[derive(Debug)]
pub struct TimerSpec {
    pub interval: Duration,
    pub value: Duration
}

impl TimerFd {
    /// Create a timerfd with clickid: CLOCK_REALTIME and flags: TFD_CLOEXEC | TFD_NONBLOCK
    /// http://man7.org/linux/man-pages/man2/timerfd_create.2.html
    ///
    /// # Example
    ///
    /// ```
    /// use queen_io::sys::timerfd::TimerFd;
    ///
    /// let timerfd = TimerFd::new();
    /// ```
    pub fn new() -> io::Result<TimerFd> {
        let clock = Clock::Realtime;
        let flags = TFD_CLOEXEC | TFD_NONBLOCK;
        TimerFd::create(clock, flags)
    }

    /// Create a timerfd with clock and flags
    ///
    /// # Example
    ///
    /// ```
    /// use queen_io::sys::timerfd::{ TimerFd, Clock, TFD_CLOEXEC, TFD_NONBLOCK };
    ///
    /// let clock = Clock::Monotonic;
    /// let flags = TFD_CLOEXEC | TFD_NONBLOCK;
    /// let timerfd = TimerFd::create(clock, flags);
    /// ```
    pub fn create(clock: Clock, flags: i32) -> io::Result<TimerFd> {
        let timerfd = unsafe { cvt(libc::timerfd_create(clock as i32, flags))? };
        Ok(TimerFd {
            inner: unsafe { Io::from_raw_fd(timerfd) }
        })
    }

    // https://www.cnblogs.com/mickole/p/3261879.html
    /// Set time to timerfd
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    /// use queen_io::sys::timerfd::{TimerFd, TimerSpec};
    ///
    /// let timerfd = TimerFd::new().unwrap();
    ///
    /// let timerspec = TimerSpec {
    ///     interval: Duration::new(0, 0),
    ///     value: Duration::new(10, 0)
    /// };
    ///
    /// let old_value = timerfd.settime(false, timerspec);
    /// ```
    pub fn settime(&self, abstime: bool, value: TimerSpec) -> io::Result<TimerSpec> {
        let new_value = libc::itimerspec {
            it_interval: duration_to_timespec(value.interval),
            it_value: duration_to_timespec(value.value)
        };

        let mut old_value: libc::itimerspec = unsafe { mem::zeroed() };

        let flags: i32 = if abstime { 1 } else { 0 };

        unsafe {
            cvt(libc::timerfd_settime(
                self.inner.as_raw_fd(),
                flags,
                &new_value as *const libc::itimerspec,
                &mut old_value as *mut libc::itimerspec
            ))?;
        }

        Ok(TimerSpec {
            interval: timespec_to_duration(old_value.it_interval),
            value: timespec_to_duration(old_value.it_value)
        })
    }

    /// Get time
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    /// use queen_io::sys::timerfd::{TimerFd, TimerSpec};
    ///
    /// let timerfd = TimerFd::new().unwrap();
    ///
    /// let timerspec = TimerSpec {
    ///     interval: Duration::new(0, 0),
    ///     value: Duration::new(10, 0)
    /// };
    ///
    /// let old_value = timerfd.settime(false, timerspec);
    ///
    /// let value = timerfd.gettime();
    /// ```
    pub fn gettime(&self) -> io::Result<TimerSpec> {
        let mut itimerspec: libc::itimerspec = unsafe { mem::zeroed() };
        unsafe {
            cvt(libc::timerfd_gettime(
                self.inner.as_raw_fd(),
                &mut itimerspec as *mut libc::itimerspec
            ))?;
        }

        Ok(TimerSpec {
            interval: timespec_to_duration(itimerspec.it_interval),
            value: timespec_to_duration(itimerspec.it_value)
        })
    }

    /// read(2) If the timer has already expired one or more times since
    /// its settings were last modified using timerfd_settime(), or since
    /// the last successful read(2), then the buffer given to read(2) returns
    /// an unsigned 8-byte integer (uint64_t) containing the number of
    /// expirations that have occurred. (The returned value is in host byte
    /// order, i.e., the native byte order for integers on the host machine.)
    pub fn read(&self) -> io::Result<u64> {
        let mut buf = [0u8; 8];
        (&self.inner).read_exact(&mut buf)?;
        let temp: u64 = unsafe { mem::transmute(buf) };
        Ok(temp)
    }
}

fn duration_to_timespec(duration: Duration) -> libc::timespec {
    libc::timespec {
        tv_sec: duration.as_secs() as i64,
        tv_nsec: i64::from(duration.subsec_nanos())
    }
}

fn timespec_to_duration(timespec: libc::timespec) -> Duration {
    Duration::new(timespec.tv_sec as u64, timespec.tv_nsec as u32)
}

impl FromRawFd for TimerFd {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        TimerFd {
            inner: Io::from_raw_fd(fd)
        }
    }
}

impl IntoRawFd for TimerFd {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}

impl AsRawFd for TimerFd {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl Evented for TimerFd {
    fn add(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        epoll.add(&self.as_raw_fd(), token, interest, opts)
    }

    fn modify(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        epoll.modify(&self.as_raw_fd(), token, interest, opts)
    }

    fn delete(&self, epoll: &Epoll) -> io::Result<()> {
        epoll.delete(&self.as_raw_fd())
    }
}
