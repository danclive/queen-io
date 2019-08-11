use std::sync::{mpsc, Arc};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::error;
use std::any::Any;
use std::fmt;

use crate::sys::io;
use crate::{Awakener, Ready, Evented, Epoll, Token, EpollOpt};

pub fn channel<T>() -> io::Result<(Sender<T>, Receiver<T>)> {
    let (tx_ctl, rx_ctl) = ctl_pair()?;
    let (tx, rx) = mpsc::channel();

    let tx = Sender {
        tx,
        ctl: tx_ctl
    };

    let rx = Receiver {
        rx,
        ctl: rx_ctl
    };

    Ok((tx, rx))
}

pub fn sync_channel<T>(bound: usize) -> io::Result<(SyncSender<T>, Receiver<T>)> {
    let (tx_ctl, rx_ctl) = ctl_pair()?;
    let (tx, rx) = mpsc::sync_channel(bound);

    let tx = SyncSender {
        tx,
        ctl: tx_ctl
    };

    let rx = Receiver {
        rx,
        ctl: rx_ctl
    };

    Ok((tx, rx))
}

pub fn ctl_pair() -> io::Result<(SenderCtl, ReceiverCtl)> {
    let awakener = Awakener::new()?;

    let inner = Arc::new(Inner {
        pending: AtomicUsize::new(0),
        senders: AtomicUsize::new(1),
        awakener
    });

    let tx = SenderCtl {
        inner: inner.clone()
    };

    let rx = ReceiverCtl {
        inner
    };

    Ok((tx, rx))
}

#[derive(Debug)]
pub struct Sender<T> {
    tx: mpsc::Sender<T>,
    ctl: SenderCtl
}

#[derive(Debug)]
pub struct SyncSender<T> {
    tx: mpsc::SyncSender<T>,
    ctl: SenderCtl
}

#[derive(Debug)]
pub struct Receiver<T> {
    rx: mpsc::Receiver<T>,
    ctl: ReceiverCtl
}

#[derive(Debug)]
pub struct SenderCtl {
    inner: Arc<Inner>
}

#[derive(Debug)]
pub struct ReceiverCtl {
    inner: Arc<Inner>
}

#[derive(Debug)]
struct Inner {
    pending: AtomicUsize,
    senders: AtomicUsize,
    awakener: Awakener
}

pub enum SendError<T> {
    Io(io::Error),
    Disconnected(T),
}

pub enum TrySendError<T> {
    Io(io::Error),
    Full(T),
    Disconnected(T),
}

impl<T> Sender<T> {
    pub fn send(&self, t: T) -> Result<(), SendError<T>> {
        self.tx.send(t).map_err(SendError::from).and_then(|_| { self.ctl.inc()?; Ok(()) })
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Sender<T> {
        Sender {
            tx: self.tx.clone(),
            ctl: self.ctl.clone()
        }
    }
}

impl<T> SyncSender<T> {
    pub fn send(&self, t: T) -> Result<(), SendError<T>> {
        self.tx.send(t).map_err(From::from).and_then(|_| { self.ctl.inc()?; Ok(()) })
    }

    pub fn try_send(&self, t: T) -> Result<(), TrySendError<T>> {
        self.tx.try_send(t).map_err(From::from).and_then(|_| { self.ctl.inc()?; Ok(()) })
    }
}

impl<T> Clone for SyncSender<T> {
    fn clone(&self) -> SyncSender<T> {
        SyncSender {
            tx: self.tx.clone(),
            ctl: self.ctl.clone()
        }
    }
}

impl<T> Receiver<T> {
    pub fn try_recv(&self) -> Result<T, mpsc::TryRecvError> {
        self.rx.try_recv().and_then(|res| {
            let _ = self.ctl.dec();
            Ok(res)
        })
    }

    pub fn recv(&self) -> Result<T, mpsc::RecvError> {
        self.rx.recv().and_then(|res| {
            let _ = self.ctl.dec();
            Ok(res)
        })
    }

    pub fn try_iter(&self) -> TryIter<T> {
        TryIter { rx: self }
    }
}

impl<T> Evented for Receiver<T> {
    fn add(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        self.ctl.add(epoll, token, interest, opts)
    }

    fn modify(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        self.ctl.modify(epoll, token, interest, opts)
    }

    fn delete(&self, epoll: &Epoll) -> io::Result<()> {
        self.ctl.delete(epoll)
    }
}

impl SenderCtl {
    pub fn inc(&self) -> io::Result<()> {
        let cnt = self.inner.pending.fetch_add(1, Ordering::Acquire);

        if 0 == cnt {
            self.inner.awakener.set_readiness(Ready::readable())?;
        }

        Ok(())
    }
}

impl Clone for SenderCtl {
    fn clone(&self) -> SenderCtl {
        self.inner.senders.fetch_add(1, Ordering::Relaxed);
        SenderCtl { inner: self.inner.clone() }
    }
}

impl Drop for SenderCtl {
    fn drop(&mut self) {
        if self.inner.senders.fetch_sub(1, Ordering::Release) == 1 {
            let _ = self.inc();
        }
    }
}

impl ReceiverCtl {
    pub fn dec(&self) -> io::Result<()> {
        let first = self.inner.pending.load(Ordering::Acquire);

        if first == 1 {
            self.inner.awakener.set_readiness(Ready::empty())?;
        }

        let second = self.inner.pending.fetch_sub(1, Ordering::AcqRel);

        if first == 1 && second > 1 {
            self.inner.awakener.set_readiness(Ready::readable())?;
        }

        Ok(())
    }
}

impl Evented for ReceiverCtl {
    fn add(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        self.inner.awakener.add(epoll, token, interest, opts)?;

        if self.inner.pending.load(Ordering::Relaxed) > 0 {
            self.inner.awakener.set_readiness(Ready::readable())?;
        }

        Ok(())
    }

    fn modify(&self, epoll: &Epoll, token: Token, interest: Ready, opts: EpollOpt) -> io::Result<()> {
        self.inner.awakener.modify(epoll, token, interest, opts)
    }

    fn delete(&self, epoll: &Epoll) -> io::Result<()> {
        self.inner.awakener.delete(epoll)
    }
}

#[derive(Debug)]
pub struct TryIter<'a, T: 'a> {
    rx: &'a Receiver<T>
}

impl<'a, T> Iterator for TryIter<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> { self.rx.try_recv().ok() }
}


impl<T> From<mpsc::SendError<T>> for SendError<T> {
    fn from(src: mpsc::SendError<T>) -> SendError<T> {
        SendError::Disconnected(src.0)
    }
}

impl<T> From<io::Error> for SendError<T> {
    fn from(src: io::Error) -> SendError<T> {
        SendError::Io(src)
    }
}

impl<T> From<mpsc::TrySendError<T>> for TrySendError<T> {
    fn from(src: mpsc::TrySendError<T>) -> TrySendError<T> {
        match src {
            mpsc::TrySendError::Full(v) => TrySendError::Full(v),
            mpsc::TrySendError::Disconnected(v) => TrySendError::Disconnected(v),
        }
    }
}

impl<T> From<mpsc::SendError<T>> for TrySendError<T> {
    fn from(src: mpsc::SendError<T>) -> TrySendError<T> {
        TrySendError::Disconnected(src.0)
    }
}

impl<T> From<io::Error> for TrySendError<T> {
    fn from(src: io::Error) -> TrySendError<T> {
        TrySendError::Io(src)
    }
}

impl<T: Any> error::Error for SendError<T> {
    fn description(&self) -> &str {
        match *self {
            SendError::Io(ref io_err) => io_err.description(),
            SendError::Disconnected(..) => "Disconnected",
        }
    }
}

impl<T: Any> error::Error for TrySendError<T> {
    fn description(&self) -> &str {
        match *self {
            TrySendError::Io(ref io_err) => io_err.description(),
            TrySendError::Full(..) => "Full",
            TrySendError::Disconnected(..) => "Disconnected",
        }
    }
}

impl<T> fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        format_send_error(self, f)
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        format_send_error(self, f)
    }
}

impl<T> fmt::Debug for TrySendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        format_try_send_error(self, f)
    }
}

impl<T> fmt::Display for TrySendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        format_try_send_error(self, f)
    }
}

#[inline]
fn format_send_error<T>(e: &SendError<T>, f: &mut fmt::Formatter) -> fmt::Result {
    match *e {
        SendError::Io(ref io_err) => write!(f, "{}", io_err),
        SendError::Disconnected(..) => write!(f, "Disconnected"),
    }
}

#[inline]
fn format_try_send_error<T>(e: &TrySendError<T>, f: &mut fmt::Formatter) -> fmt::Result {
    match *e {
        TrySendError::Io(ref io_err) => write!(f, "{}", io_err),
        TrySendError::Full(..) => write!(f, "Full"),
        TrySendError::Disconnected(..) => write!(f, "Disconnected"),
    }
}
