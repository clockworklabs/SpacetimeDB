use smallvec::SmallVec;
use crate::map::{DefaultHashBuilder, HashMap};

pub enum SmallMap<K, V, const N: usize, S = DefaultHashBuilder> {
    Small(SmallVec<[(K, V); N]>),
    Large(HashMap<K, V, S>),
}
