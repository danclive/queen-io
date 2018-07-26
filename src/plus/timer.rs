use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::collections::{BinaryHeap, VecDeque};
use std::cmp::Ordering;
use std::thread;
use std::sync::{Arc, Mutex, Condvar};
use std::io;

use {Registration, Ready, Evented, Poll, Token, PollOpt};

#[derive(Debug, Clone, Default, Eq)]
pub struct Timespec {
    pub interval: Duration,
    pub value: Duration
}

impl PartialOrd for Timespec {
    fn partial_cmp(&self, other: &Timespec) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Timespec {
    fn cmp(&self, other: &Timespec) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl PartialEq for Timespec {
    fn eq(&self, other: &Timespec) -> bool {
        self.value == other.value
    }
}

#[derive(Clone, Debug)]
pub struct Task<T> {
    pub data: T,
    pub timespec: Timespec
}

impl<T> PartialOrd for Task<T> {
    fn partial_cmp(&self, other: &Task<T>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Task<T> {
    fn cmp(&self, other: &Task<T>) -> Ordering {
        match self.timespec.cmp(&other.timespec) {
            Ordering::Equal => Ordering::Equal,
            Ordering::Greater => Ordering::Less,
            Ordering::Less => Ordering::Greater
        }
    }
}

impl<T> PartialEq for Task<T> {
    fn eq(&self, other: &Task<T>) -> bool {
        self.timespec == other.timespec
    }
}

impl<T> Eq for Task<T> {}

pub struct Timer<T: Clone> {
    thread_handle: thread::JoinHandle<()>,
    inner: Arc<TimerInner<T>>
}

struct TimerInner<T> {
    tasks: Mutex<BinaryHeap<Task<T>>>,
    queue: Mutex<VecDeque<Task<T>>>,
    registration: Registration,
    condvar: Condvar
}

impl<T> Timer<T> where T: Clone + Send + PartialEq + 'static {
    pub fn new() -> Timer<T> {

        let inner = Arc::new(TimerInner {
            tasks: Mutex::new(BinaryHeap::new()),
            queue: Mutex::new(VecDeque::new()),
            registration: Registration::new().unwrap(),
            condvar: Condvar::new()
        });

        let inner2 = inner.clone();

        let thread_handle = thread::Builder::new().name("timer".to_owned()).spawn(move || {
            let inner = inner2;

            loop {
                let mut sleep_duration = Duration::from_secs(60);

                loop {
                    let mut tasks = inner.tasks.lock().unwrap();

                    if let Some(ref task) = tasks.peek().map(|v| v.to_owned()) {
                        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

                        if task.timespec.value > now {
                            sleep_duration = task.timespec.value - now;
                            break;
                        } else {
                            if let Some(mut task) = tasks.pop() {
                                let mut queue = inner.queue.lock().unwrap();
                                queue.push_back(task.clone());

                                if task.timespec.interval != Duration::default() {
                                    task.timespec.value += task.timespec.interval;
                                    tasks.push(task);
                                }

                                inner.condvar.notify_one();
                                inner.registration.set_readiness(Ready::readable()).unwrap();
                            }
                        }
                    } else {
                        break;
                    }
                }

                thread::park_timeout(sleep_duration);
            }
        }).unwrap();

        Timer {
            thread_handle,
            inner
        }
    }

    pub fn insert(&self, task: Task<T>) {
        let mut tasks = self.inner.tasks.lock().unwrap();
        tasks.push(task);
        self.thread_handle.thread().unpark();
    }

    pub fn remove(&mut self, token: T) {
        let mut tasks = self.inner.tasks.lock().unwrap();

        let mut tasks_vec: Vec<Task<T>> = Vec::from(tasks.clone());

        if let Some(pos) = tasks_vec.iter().position(|x| x.data == token) {
            tasks_vec.remove(pos);
        }

        *tasks = tasks_vec.into();
    }

    pub fn pop(&self) -> Task<T> {
        let mut queue = self.inner.queue.lock().unwrap();

        loop {
            if let Some(task) = queue.pop_front() {
                return task;
            }

            queue = self.inner.condvar.wait(queue).unwrap();
        }
    }

    pub fn try_pop(&self) -> io::Result<Option<Task<T>>> {
        let mut queue = self.inner.queue.lock().unwrap();

        if queue.len() <= 1 {
            self.inner.registration.set_readiness(Ready::empty())?;
        } else {
            self.inner.registration.set_readiness(Ready::readable())?;
        }

        Ok(queue.pop_front())
    }
}

impl<T> Evented for Timer<T> where T: Clone {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.inner.registration.register(poll, token, interest, opts)?;

        let queue = self.inner.queue.lock().unwrap();
        if queue.len() > 0 {
            self.inner.registration.set_readiness(Ready::readable())?;
        }

        Ok(())
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.inner.registration.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.inner.registration.deregister(poll)
    }
}
