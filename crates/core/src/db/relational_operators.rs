use core::marker::PhantomData;
use spacetimedb_data_structures::map::{HashCollectionExt, HashSet};
use spacetimedb_sats::ProductValue;

// NOTE
// Currently anything that is IntoIterator is also a relation
// We're using IntoIterator rather than Iterator because an
// Iterator is an IntoIterator but IntoIterator is not an Iterator
// This then allows a Vec of ProductValues to be a Relation
//
// In the future we might not want an IntoIterator to implicitly
// be a Relation, but rather allow it to be convertible to a
// Relation so that we can deduplicate the IntoIterator upon
// conversion. Currently we assume that if you're using an
// IntoIterator as a relation it already is deduplicated.
//
// In this case a Relation should just be a trait which implements
// From<IntoIterator>.
// See: https://github.com/frankmcsherry/blog/blob/master/posts/2018-05-19.md
pub trait Relation: IntoIterator<Item = ProductValue> {
    // TODO: Technically need to dedupe again after removing potentially
    // distinguishing columns
    fn project(self, mut cols: Vec<u32>) -> Project<Self::IntoIter>
    where
        Self: Sized,
    {
        cols.sort();
        Project {
            source: self.into_iter(),
            cols,
        }
    }

    fn select(self, filter: fn(&ProductValue) -> bool) -> Select<Self>
    where
        Self: Sized,
    {
        Select { source: self, filter }
    }

    fn union_all<O: Relation>(self, other: O) -> UnionAll<Self, O>
    where
        Self: Sized,
    {
        UnionAll { s: self, u: other }
    }

    fn union<'a, O: Relation>(self, other: O) -> Union<'a, Self, O>
    where
        Self: Sized,
    {
        Union {
            s: self,
            u: other,
            phantom: PhantomData {},
        }
    }

    fn intersect<'a, O: Relation>(self, other: O) -> Intersection<'a, Self, O>
    where
        Self: Sized,
    {
        Intersection {
            s: self,
            u: other,
            phantom: PhantomData {},
        }
    }

    fn difference<'a, O: Relation>(self, other: O) -> Difference<'a, Self, O>
    where
        Self: Sized,
    {
        Difference {
            s: self,
            u: other,
            phantom: PhantomData {},
        }
    }
}

impl<T> Relation for T where T: IntoIterator<Item = ProductValue> {}

pub struct Select<S: Relation> {
    source: S,
    filter: fn(&ProductValue) -> bool,
}

impl<S> IntoIterator for Select<S>
where
    S: Relation,
{
    type Item = ProductValue;
    type IntoIter = std::iter::Filter<<S as IntoIterator>::IntoIter, for<'r> fn(&'r ProductValue) -> bool>;

    fn into_iter(self) -> Self::IntoIter {
        self.source.into_iter().filter(self.filter)
    }
}

// See: https://users.rust-lang.org/t/how-to-use-adapters-closures-for-intoiterator-implementation/46121
pub struct Project<S: Iterator<Item = ProductValue>> {
    source: S,
    cols: Vec<u32>,
}

impl<S: Iterator<Item = ProductValue>> Iterator for Project<S> {
    type Item = ProductValue;

    fn next(&mut self) -> Option<ProductValue> {
        self.source.next().map(|row| {
            let mut row: Vec<_> = row.elements.into();
            for &i in self.cols.iter().rev() {
                row.remove(i as usize);
            }
            row.into()
        })
    }
}

pub struct UnionAll<S: Relation, U: Relation> {
    s: S,
    u: U,
}

impl<S: Relation, U: Relation> IntoIterator for UnionAll<S, U> {
    type Item = ProductValue;
    type IntoIter = std::iter::Chain<S::IntoIter, U::IntoIter>;

    fn into_iter(self) -> Self::IntoIter {
        itertools::chain(self.s, self.u)
    }
}

pub struct Union<'a, S: Relation, U: Relation> {
    s: S,
    u: U,
    phantom: PhantomData<&'a S>,
}

impl<'a, S: Relation, U: Relation> IntoIterator for Union<'a, S, U> {
    type Item = ProductValue;
    type IntoIter = std::vec::IntoIter<ProductValue>;

    fn into_iter(self) -> Self::IntoIter {
        let mut set_s: HashSet<ProductValue> = HashSet::new();
        let mut set_u: HashSet<ProductValue> = HashSet::new();
        for next in self.s {
            set_s.insert(next);
        }
        for next in self.u {
            set_u.insert(next);
        }
        HashSet::union(&set_s, &set_u).cloned().collect::<Vec<_>>().into_iter()
    }
}

pub struct Intersection<'a, S: Relation, U: Relation> {
    s: S,
    u: U,
    phantom: PhantomData<&'a S>,
}

impl<'a, S: Relation, U: Relation> IntoIterator for Intersection<'a, S, U> {
    type Item = ProductValue;
    type IntoIter = std::vec::IntoIter<ProductValue>;

    fn into_iter(self) -> Self::IntoIter {
        let mut set_s: HashSet<ProductValue> = HashSet::new();
        let mut set_u: HashSet<ProductValue> = HashSet::new();
        for next in self.s {
            set_s.insert(next);
        }
        for next in self.u {
            set_u.insert(next);
        }
        HashSet::intersection(&set_s, &set_u)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
    }
}

pub struct Difference<'a, S: Relation, U: Relation> {
    s: S,
    u: U,
    phantom: PhantomData<&'a S>,
}

impl<'a, S: Relation, U: Relation> IntoIterator for Difference<'a, S, U> {
    type Item = ProductValue;
    type IntoIter = std::vec::IntoIter<ProductValue>;

    fn into_iter(self) -> Self::IntoIter {
        let mut set_s: HashSet<ProductValue> = HashSet::new();
        let mut set_u: HashSet<ProductValue> = HashSet::new();
        for next in self.s {
            set_s.insert(next);
        }
        for next in self.u {
            set_u.insert(next);
        }
        HashSet::difference(&set_s, &set_u)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
    }
}
