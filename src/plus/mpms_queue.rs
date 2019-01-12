// http://www.1024cores.net/home/lock-free-algorithms/queues/bounded-mpmc-queue
// This queue is copy pasted from rust stdlib.

use std::sync::Arc;
use std::cell::UnsafeCell;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Relaxed, Release, Acquire, AcqRel};
use std::os::unix::io::{AsRawFd, RawFd};

use crate::sys::io;
use crate::{Awakener, Ready, Evented, Poll, Token, PollOpt};

struct Node<T> {
    sequence: AtomicUsize,
    value: Option<T>,
}

unsafe impl<T: Send> Send for Node<T> {}
unsafe impl<T: Sync> Sync for Node<T> {}

struct State<T> {
    _pad0: [u8; 64],
    buffer: Vec<UnsafeCell<Node<T>>>,
    mask: usize,
    _pad1: [u8; 64],
    enqueue_pos: AtomicUsize,
    _pad2: [u8; 64],
    dequeue_pos: AtomicUsize,
    _pad3: [u8; 64]
}

struct Inner<T> {
    state: State<T>,
    pending: AtomicUsize,
    awakener: Awakener
}

unsafe impl<T: Send> Send for State<T> {}
unsafe impl<T: Sync> Sync for State<T> {}

pub struct Queue<T: Send> {
    inner: Arc<Inner<T>>
}

impl<T: Send> State<T> {
    fn with_capacity(capacity: usize) -> State<T> {
        let capacity = if capacity < 2 || (capacity & (capacity - 1)) != 0 {
            if capacity < 2 {
                2
            } else {
                // use next power of 2 as capacity
                capacity.next_power_of_two()
            }
        } else {
            capacity
        };
        let buffer = (0..capacity)
            .map(|i| {
                UnsafeCell::new(Node {
                    sequence: AtomicUsize::new(i),
                    value: None,
                })
            })
            .collect::<Vec<_>>();
        State {
            _pad0: [0; 64],
            buffer: buffer,
            mask: capacity - 1,
            _pad1: [0; 64],
            enqueue_pos: AtomicUsize::new(0),
            _pad2: [0; 64],
            dequeue_pos: AtomicUsize::new(0),
            _pad3: [0; 64]
        }
    }

    fn push(&self, value: T) -> Result<(), T> {
        let mask = self.mask;
        let mut pos = self.enqueue_pos.load(Relaxed);
        loop {
            let node = &self.buffer[pos & mask];
            let seq = unsafe { (*node.get()).sequence.load(Acquire) };
            let diff: isize = seq as isize - pos as isize;

            if diff == 0 {
                let enqueue_pos = self.enqueue_pos.compare_and_swap(pos, pos + 1, Relaxed);
                if enqueue_pos == pos {
                    unsafe {
                        (*node.get()).value = Some(value);
                        (*node.get()).sequence.store(pos + 1, Release);
                    }
                    break;
                } else {
                    pos = enqueue_pos;
                }
            } else if diff < 0 {
                return Err(value);
            } else {
                pos = self.enqueue_pos.load(Relaxed);
            }
        }
        Ok(())
    }

    fn pop(&self) -> Option<T> {
        let mask = self.mask;
        let mut pos = self.dequeue_pos.load(Relaxed);
        loop {
            let node = &self.buffer[pos & mask];
            let seq = unsafe { (*node.get()).sequence.load(Acquire) };
            let diff: isize = seq as isize - (pos + 1) as isize;
            if diff == 0 {
                let dequeue_pos = self.dequeue_pos.compare_and_swap(pos, pos + 1, Relaxed);
                if dequeue_pos == pos {
                    unsafe {
                        let value = (*node.get()).value.take();
                        (*node.get()).sequence.store(pos + mask + 1, Release);
                        return value;
                    }
                } else {
                    pos = dequeue_pos;
                }
            } else if diff < 0 {
                return None;
            } else {
                pos = self.dequeue_pos.load(Relaxed);
            }
        }
    }
}

impl<T: Send> Queue<T> {
    pub fn with_capacity(capacity: usize) -> io::Result<Queue<T>> {
        Ok(Queue {
            inner: Arc::new(Inner {
                state: State::with_capacity(capacity),
                pending: AtomicUsize::new(0),
                awakener: Awakener::new()?
            })
        })
    }

    pub fn push(&self, value: T) -> Result<(), T> {
        self.inner.state.push(value).and_then(|_| {let _ = self.inc(); Ok(())})
    }

    pub fn pop(&self) -> Option<T> {
        self.inner.state.pop().and_then(|res| {let _ = self.dec(); Some(res)})
    }

    fn inc(&self) -> io::Result<()> {
        let cnt = self.inner.pending.fetch_add(1, Acquire);

        if 0 == cnt {
            self.inner.awakener.set_readiness(Ready::readable())?;
        }
        Ok(())
    }

    fn dec(&self) -> io::Result<()> {
        let first = self.inner.pending.load(Acquire);

        if first == 1 {
            self.inner.awakener.set_readiness(Ready::empty())?;
        }

        let second = self.inner.pending.fetch_sub(1, AcqRel);

        if first == 1 && second > 1 {
            self.inner.awakener.set_readiness(Ready::readable())?;
        }

        Ok(())
    }
}

impl<T: Send> Clone for Queue<T> {
    fn clone(&self) -> Queue<T> {
        Queue {
            inner: self.inner.clone()
        }
    }
}

impl<T: Send> AsRawFd for Queue<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.awakener.as_raw_fd()
    }
}

impl<T: Send> Evented for Queue<T> {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.inner.awakener.register(poll, token, interest, opts)?;

        if self.inner.pending.load(Relaxed) > 0 {
            self.inner.awakener.set_readiness(Ready::readable())?;
        }

        Ok(())
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.inner.awakener.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.inner.awakener.deregister(poll)
    }
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::sync::mpsc::channel;
    use super::Queue;

    #[test]
    fn test() {
        let nthreads = 8;
        let nmsgs = 1000;
        let q = Queue::with_capacity(nthreads * nmsgs).unwrap();
        assert_eq!(None, q.pop());
        let (tx, rx) = channel();

        for _ in 0..nthreads {
            let q = q.clone();
            let tx = tx.clone();
            thread::spawn(move || {
                let q = q;
                for i in 0..nmsgs {
                    assert!(q.push(i).is_ok());
                }
                tx.send(()).unwrap();
            });
        }

        let mut completion_rxs = vec![];
        for _ in 0..nthreads {
            let (tx, rx) = channel();
            completion_rxs.push(rx);
            let q = q.clone();
            thread::spawn(move || {
                let q = q;
                let mut i = 0;
                loop {
                    match q.pop() {
                        None => {}
                        Some(_) => {
                            i += 1;
                            if i == nmsgs {
                                break;
                            }
                        }
                    }
                }
                tx.send(i).unwrap();
            });
        }

        for rx in completion_rxs.iter_mut() {
            assert_eq!(nmsgs, rx.recv().unwrap());
        }
        for _ in 0..nthreads {
            rx.recv().unwrap();
        }
    }

    #[test]
    fn event() {
        use crate::*;

        let poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(256);
        let token = Token(1);

        let queue: Queue<i32> = Queue::with_capacity(16).unwrap();
        poll.register(&queue, token, Ready::readable(), PollOpt::oneshot() | PollOpt::edge()).unwrap();
    
        queue.push(123).unwrap();

        let size = poll.wait(&mut events, None).unwrap();
        assert!(size == 1);
    }

    #[test]
    fn event2() {
        use crate::*;

        let poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(256);
        let token = Token(1);

        let queue: Queue<i32> = Queue::with_capacity(16).unwrap();
        poll.register(&queue, token, Ready::readable(), PollOpt::oneshot() | PollOpt::edge()).unwrap();
    
        queue.push(123).unwrap();
        queue.push(456).unwrap();

        'out: loop {
            poll.wait(&mut events, None).unwrap();

            for _ in &events {
                if queue.pop().unwrap() == 456 {
                    break 'out;
                }
            }

            poll.reregister(&queue, token, Ready::readable(), PollOpt::oneshot() | PollOpt::edge()).unwrap();
        }
    }
}
