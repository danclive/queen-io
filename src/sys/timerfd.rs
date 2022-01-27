use std::os::unix::io::{RawFd, AsRawFd, FromRawFd, IntoRawFd};
use std::time::Duration;
use std::mem;
use std::io::{self, Read};
use std::convert::TryInto;
use std::fmt;

use crate::epoll::{Epoll, Token, Ready, EpollOpt, Source};

use super::fd::FileDesc;

#[derive(Clone, Copy)]
#[repr(i32)]
pub enum Clock {
    Realtime = libc::CLOCK_REALTIME,
    Monotonic = libc::CLOCK_MONOTONIC,
    Boottime = libc::CLOCK_BOOTTIME,
    RealtimeAlarm = libc::CLOCK_REALTIME_ALARM,
    BoottimeAlarm = libc::CLOCK_BOOTTIME_ALARM
}

impl Clock {
    pub fn clock_name(&self) -> &'static str {
        match self {
            Clock::Realtime       => "CLOCK_REALTIME",
            Clock::RealtimeAlarm  => "CLOCK_REALTIME_ALARM",
            Clock::Monotonic      => "CLOCK_MONOTONIC",
            Clock::Boottime       => "CLOCK_BOOTTIME",
            Clock::BoottimeAlarm  => "CLOCK_BOOTTIME_ALARM",
        }
    }
}

impl fmt::Display for Clock {
    fn fmt (&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.clock_name())
    }
}

impl fmt::Debug for Clock {
    fn fmt (&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} ({})", self.clone() as i32, self.clock_name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetTimeFlags {
    /// Flags to `timerfd_settime(2)`.
    ///
    /// The default is zero, i. e. all bits unset.
    Default,

    /// Interpret new_value.it_value as an absolute value on the timer's clock. The timer will
    /// expire when the value of the timer's clock reaches the value specified in
    /// new_value.it_value.
    Abstime,

    /// If this flag is specified along with TFD_TIMER_ABSTIME and the clock for this timer is
    /// CLOCK_REALTIME or CLOCK_REALTIME_ALARM, then mark this timer as cancelable if the
    /// real-time clock undergoes a discontinuous change (settimeofday(2), clock_settime(2),
    /// or similar). When such changes occur, a current or future read(2) from the file
    /// descriptor will fail with the error ECANCELED.
    ///
    /// `TFD_TIMER_CANCEL_ON_SET` is useless without `TFD_TIMER_ABSTIME` set, cf. `fs/timerfd.c`.
    /// Thus `TimerCancelOnSet`` implies `Abstime`.
    TimerCancelOnSet,
}

pub const TFD_CLOEXEC: i32 = libc::TFD_CLOEXEC;
pub const TFD_NONBLOCK: i32 = libc::TFD_NONBLOCK;

const TFD_TIMER_ABSTIME: i32 = libc::TFD_TIMER_ABSTIME;
const TFD_TIMER_CANCEL_ON_SET: i32 = 0o0000002;

#[derive(Debug)]
pub struct TimerFd {
    inner: FileDesc
}

#[derive(Debug, Clone)]
pub struct TimerSpec {
    pub interval: Duration,
    pub value: Duration
}

impl TimerFd {
    /// Create a timerfd with clickid: CLOCK_REALTIME and flags: TFD_CLOEXEC | TFD_NONBLOCK
    /// view: `<http://man7.org/linux/man-pages/man2/timerfd_create.2.html>`
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
        let timerfd = syscall!(timerfd_create(clock as i32, flags))?;
        Ok(TimerFd {
            inner: unsafe { FileDesc::new(timerfd) }
        })
    }

    /// Set time to timerfd
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    /// use queen_io::sys::timerfd::{TimerFd, TimerSpec, SetTimeFlags};
    ///
    /// let timerfd = TimerFd::new().unwrap();
    ///
    /// let timerspec = TimerSpec {
    ///     interval: Duration::new(0, 0),
    ///     value: Duration::new(10, 0)
    /// };
    ///
    /// let old_value = timerfd.settime(timerspec, SetTimeFlags::Default);
    /// ```
    pub fn settime(&self, value: TimerSpec, flags: SetTimeFlags) -> io::Result<TimerSpec> {
        let new_value = libc::itimerspec {
            it_interval: duration_to_timespec(value.interval),
            it_value: duration_to_timespec(value.value)
        };

        let mut old_value: libc::itimerspec = unsafe { mem::zeroed() };

        let flags = match flags {
            SetTimeFlags::Default => 0,
            SetTimeFlags::Abstime => TFD_TIMER_ABSTIME,
            SetTimeFlags::TimerCancelOnSet => TFD_TIMER_ABSTIME | TFD_TIMER_CANCEL_ON_SET,
        };

        syscall!(timerfd_settime(
            self.inner.as_raw_fd(),
            flags,
            &new_value,
            &mut old_value
        ))?;

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
    /// use queen_io::sys::timerfd::{TimerFd, TimerSpec, SetTimeFlags};
    ///
    /// let timerfd = TimerFd::new().unwrap();
    ///
    /// let timerspec = TimerSpec {
    ///     interval: Duration::new(0, 0),
    ///     value: Duration::new(10, 0)
    /// };
    ///
    /// let old_value = timerfd.settime(timerspec, SetTimeFlags::Default);
    ///
    /// let value = timerfd.gettime();
    /// ```
    pub fn gettime(&self) -> io::Result<TimerSpec> {
        let mut itimerspec: libc::itimerspec = unsafe { mem::zeroed() };

        syscall!(timerfd_gettime(
            self.inner.as_raw_fd(),
            &mut itimerspec
        ))?;

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
        tv_sec: duration.as_secs().try_into().unwrap(),
        tv_nsec: duration.subsec_nanos().try_into().unwrap()
    }
}

fn timespec_to_duration(timespec: libc::timespec) -> Duration {
    Duration::new(timespec.tv_sec as u64, timespec.tv_nsec as u32)
}

impl FromRawFd for TimerFd {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        TimerFd {
            inner: FileDesc::new(fd)
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

impl Source for TimerFd {
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
