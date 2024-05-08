use crate::rel_ops::RelOps;
use crate::relation::RelValue;

/// Turns an iterator over `ProductValue`s into a `RelOps`.
#[derive(Debug)]
pub struct RelIter<I> {
    pub iter: I,
}

impl<I> RelIter<I> {
    pub fn new(iter: impl IntoIterator<IntoIter = I>) -> Self {
        let iter = iter.into_iter();
        Self { iter }
    }
}

impl<'a, I: Iterator<Item = RelValue<'a>>> RelOps<'a> for RelIter<I> {
    fn next(&mut self) -> Option<RelValue<'a>> {
        self.iter.next()
    }
}
