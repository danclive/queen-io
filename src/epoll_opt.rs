use std::{fmt, ops};

#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct EpollOpt(usize);

const EDGE: usize    = 0b0001;
const LEVEL: usize   = 0b0010;
const ONESHOT: usize = 0b0100;

impl EpollOpt {
    #[inline]
    pub fn empty() -> EpollOpt {
        EpollOpt(0)
    }

    #[inline]
    pub fn edge() -> EpollOpt {
        EpollOpt(EDGE)
    }

    #[inline]
    pub fn level() -> EpollOpt {
        EpollOpt(LEVEL)
    }

    #[inline]
    pub fn oneshot() -> EpollOpt {
        EpollOpt(ONESHOT)
    }

    #[inline]
    pub fn is_edge(self) -> bool {
        self.contains(EpollOpt::edge())
    }

    #[inline]
    pub fn is_level(self) -> bool {
        self.contains(EpollOpt::level())
    }

    #[inline]
    pub fn is_oneshot(self) -> bool {
        self.contains(EpollOpt::oneshot())
    }

    #[inline]
    pub fn contains(self, other: EpollOpt) -> bool {
        (self & other) == other
    }

    #[inline]
    pub fn insert(&mut self, other: EpollOpt) {
        self.0 |= other.0;
    }

    #[inline]
    pub fn remove(&mut self, other: EpollOpt) {
        self.0 &= !other.0;
    }

    pub fn as_usize(self) -> usize {
        self.0
    }
}

impl ops::BitOr for EpollOpt {
    type Output = EpollOpt;

    #[inline]
    fn bitor(self, other: EpollOpt) -> EpollOpt {
        EpollOpt(self.0 | other.0)
    }
}

impl ops::BitXor for EpollOpt {
    type Output = EpollOpt;

    #[inline]
    fn bitxor(self, other: EpollOpt) -> EpollOpt {
        EpollOpt(self.0 ^ other.0)
    }
}

impl ops::BitAnd for EpollOpt {
    type Output = EpollOpt;

    #[inline]
    fn bitand(self, other: EpollOpt) -> EpollOpt {
        EpollOpt(self.0 & other.0)
    }
}

impl ops::Sub for EpollOpt {
    type Output = EpollOpt;

    #[inline]
    fn sub(self, other: EpollOpt) -> EpollOpt {
        EpollOpt(self.0 & !other.0)
    }
}

impl ops::Not for EpollOpt {
    type Output = EpollOpt;

    #[inline]
    fn not(self) -> EpollOpt {
        EpollOpt(!self.0)
    }
}

impl From<usize> for EpollOpt {
    fn from(opt: usize) -> EpollOpt {
        EpollOpt(opt)
    }
}

impl fmt::Debug for EpollOpt {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (EpollOpt::edge(), "Edge-Triggered"),
            (EpollOpt::level(), "Level-Triggered"),
            (EpollOpt::oneshot(), "OneShot")];

        for &(flag, msg) in &flags {
            if self.contains(flag) {
                if one { write!(fmt, " | ")? }
                write!(fmt, "{}", msg)?;

                one = true
            }
        }

        Ok(())
    }
}

