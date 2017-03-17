use std::time::Duration;
use soio::*;

#[test]
fn test1() {
    let poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(128);

    let (_r, set) = Registration::new_prev(&poll, Token(123), Ready::readable(), PollOpt::edge());

    let n = poll.poll(&mut events, Some(Duration::from_millis(0))).unwrap();
    assert_eq!(n, 0);

    set.set_readiness(Ready::readable()).unwrap();

    let n = poll.poll(&mut events, Some(Duration::from_millis(0))).unwrap();
    assert_eq!(n, 1);

    assert_eq!(events.get(0).unwrap().token(), Token(123));
}

#[test]
fn set_readiness_before_register() {
    use std::thread;

    let poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(128);

    for _ in 0..5000 {
        let (r, set) = Registration::new();

        let th = thread::spawn(move || {
            set.set_readiness(Ready::readable()).unwrap();
        });

        poll.register(&r, Token(123), Ready::readable(), PollOpt::edge()).unwrap();

        loop {
            let n = poll.poll(&mut events, None).unwrap();

            if n == 0 {
                continue;
            }

            assert_eq!(n, 1);
            assert_eq!(events.get(0).unwrap().token(), Token(123));
            break;
        }

        th.join().unwrap();
    }
}

#[test]
fn registration_new() {
    //use soio::{Events, Ready, Registration, Poll, PollOpt, Token};
    use std::thread;

    let (r, set) = Registration::new();

    thread::spawn(move || {
        use std::time::Duration;
        thread::sleep(Duration::from_millis(500));

        set.set_readiness(Ready::readable()).unwrap();
    });

    let poll = Poll::new().unwrap();
    poll.register(&r, Token(123), Ready::readable() | Ready::writable(), PollOpt::edge()).unwrap();

    let mut events = Events::with_capacity(256);

    loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == Token(123) && event.readiness().is_readable() {
                return;
            }
        }
    }
}

#[test]
fn registration_prev_new() {
    use std::thread;

    let poll = Poll::new().unwrap();

    let (_r, set) = Registration::new_prev(&poll, Token(123), Ready::readable() | Ready::writable(), PollOpt::edge());

    thread::spawn(move || {
        use std::time::Duration;
        thread::sleep(Duration::from_millis(500));

        set.set_readiness(Ready::readable()).unwrap();
    });

    let mut events = Events::with_capacity(256);

    loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == Token(123) && event.readiness().is_readable() {
                return;
            }
        }
    }
}

#[test]
fn stress_single_threaded_poll() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::{Acquire, Release};
    use std::thread;

    const NUM_ATTEMPTS: usize = 30;
    const NUM_ITERS: usize = 500;
    const NUM_THREADS: usize = 4;
    const NUM_REGISTRATIONS: usize = 128;

    for _ in 0..NUM_ATTEMPTS {
        let poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(128);

        let registrations: Vec<_> = (0..NUM_REGISTRATIONS).map(|i| {
            Registration::new_prev(&poll, Token(i), Ready::readable(), PollOpt::edge())
        }).collect();

        let mut ready: Vec<_> = (0..NUM_REGISTRATIONS).map(|_| Ready::empty()).collect();

        let remaining = Arc::new(AtomicUsize::new(NUM_THREADS));

        for _ in 0..NUM_THREADS {
            let remaining = remaining.clone();

            let set_readiness: Vec<SetReadiness> =
                registrations.iter().map(|r| r.1.clone()).collect();

            thread::spawn(move || {
                for _ in 0..NUM_ITERS {
                    for i in 0..NUM_REGISTRATIONS {
                        set_readiness[i].set_readiness(Ready::readable()).unwrap();
                        set_readiness[i].set_readiness(Ready::empty()).unwrap();
                        set_readiness[i].set_readiness(Ready::writable()).unwrap();
                        set_readiness[i].set_readiness(Ready::readable() | Ready::writable()).unwrap();
                        set_readiness[i].set_readiness(Ready::empty()).unwrap();
                    }
                }

                for i in 0..NUM_REGISTRATIONS {
                    set_readiness[i].set_readiness(Ready::readable()).unwrap();
                }

                remaining.fetch_sub(1, Release);
            });
        }

        while remaining.load(Acquire) > 0 {
            for (i, &(ref r, _)) in registrations.iter().enumerate() {
                r.register(&poll, Token(i), Ready::writable(), PollOpt::edge()).unwrap();
            }

            poll.poll(&mut events, Some(Duration::from_millis(0))).unwrap();

            for event in &events {
                ready[event.token().0] = event.readiness();
            }

            for (i, &(ref r, _)) in registrations.iter().enumerate() {
                r.reregister(&poll, Token(i), Ready::readable(), PollOpt::edge()).unwrap();
            }
        }

        poll.poll(&mut events, Some(Duration::from_millis(0))).unwrap();

        for event in &events {
            ready[event.token().0] = event.readiness();
        }

        for ready in ready {
            assert_eq!(ready, Ready::readable());
        }
    }
}

#[test]
fn stress_with_small_events_collection() {
    const N: usize = 8;
    const ITER: usize = 1000;

    use std::sync::{Arc, Barrier};
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering::{Acquire, Release};
    use std::thread;

    let poll = Poll::new().unwrap();
    let mut registrations = vec![];

    let barrier = Arc::new(Barrier::new(N + 1));
    let done = Arc::new(AtomicBool::new(false));

    for i in 0..N {
        let (r, set) = Registration::new();
        poll.register(&r, Token(i), Ready::readable(), PollOpt::edge()).unwrap();

        registrations.push(r);

        let barrier = barrier.clone();
        let done = done.clone();

        thread::spawn(move || {
            barrier.wait();

            while !done.load(Acquire) {
                set.set_readiness(Ready::readable()).unwrap();
            }

            set.set_readiness(Ready::readable()).unwrap();
        });
    }

    let mut events = Events::with_capacity(4);

    barrier.wait();

    for  _ in 0..ITER {
        poll.poll(&mut events, None).unwrap();
    }

    done.store(true, Release);

    let mut final_ready = vec![false; N];

    for _ in 0..5 {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            final_ready[event.token().0] = true;
        }

        if final_ready.iter().all(|v| *v) {
            return;
        }

        thread::sleep(Duration::from_millis(10))
    }

    panic!("dead lock?");
}
