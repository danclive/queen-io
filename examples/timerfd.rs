use std::time::Duration;

use queen_io::sys::timerfd::*;

fn main() {
    let clock = Clock::Realtime;
    let flags = TFD_CLOEXEC;
    let timerfd = TimerFd::create(clock, flags).unwrap();

    let timerspec = TimerSpec {
        interval: Duration::new(1, 0),
        value: Duration::new(10, 0)
    };

    timerfd.settime(timerspec.clone(), SetTimeFlags::Default).unwrap();

    loop {
        println!("{:?}", timerfd.read());
    }
}
