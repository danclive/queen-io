use std::time::Duration;
use std::collections::BinaryHeap;
use std::cmp::Ordering;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timespec {
    interval: Duration,
    value: Duration
}

#[derive(Debug, Eq)]
struct Task {
    token: usize,
    timespec: Timespec
}

impl Ord for Task {
    fn cmp(&self, other: &Task) -> Ordering {
        match self.timespec.cmp(&other.timespec) {
            Ordering::Equal => Ordering::Equal,
            Ordering::Greater => Ordering::Less,
            Ordering::Less => Ordering::Greater
        }
    }
}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Task) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Task {
    fn eq(&self, other: &Task) -> bool {
        self.timespec == other.timespec
    }
}

pub struct Timer {
    tick_ms: u64,
    tasks: BinaryHeap<Task>
}

impl Timer {
    pub fn new(tick_ms: u64,) -> Timer {
        Timer {
            tick_ms,
            tasks: BinaryHeap::new()
        }
    }

    pub fn insert(&mut self, token: usize, timespec: Timespec) {
        
    }

    pub fn remove(&mut self, token: usize) {


        let a = 123;

    }

    pub fn pop(&mut self) -> usize {
        0
    }

    pub fn try_pop(&mut self) -> Option<usize> {
        None
    }
}
