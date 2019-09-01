
pub trait AsInner<Inner: ?Sized> {
    fn as_inner(&self) -> &Inner;
}

pub trait AsInnerMut<Inner: ?Sized> {
    fn as_inner_mut(&mut self) -> &mut Inner;
}

pub trait IntoInner<Inner> {
    fn into_inner(self) -> Inner;
}

pub trait FromInner<Inner> {
    fn from_inner(inner: Inner) -> Self;
}
