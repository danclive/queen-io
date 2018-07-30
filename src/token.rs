#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Token(pub usize);

impl From<usize> for Token {
    #[inline]
    fn from(val: usize) -> Token {
        Token(val)
    }
}

impl From<Token> for usize {
    #[inline]
    fn from(val: Token) -> usize {
        val.0
    }
}