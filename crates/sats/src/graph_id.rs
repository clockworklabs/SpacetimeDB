use crate::{de::Deserialize, impl_st, ser::Serialize, AlgebraicType, AlgebraicValue};
use std::fmt;

/// A 64-bit identifier for graph entities (vertices and edges).
///
/// `GraphId` is the foundational type for the native graph extension.
/// Vertices and edges in a graph are identified by unique `GraphId` values.
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, Debug)]
#[sats(crate = crate)]
pub struct GraphId {
    __graph_id__: u64,
}

impl_st!([] GraphId, AlgebraicType::graph_id());

impl GraphId {
    /// The minimum possible `GraphId`.
    pub const MIN: Self = Self { __graph_id__: u64::MIN };

    /// The maximum possible `GraphId`.
    pub const MAX: Self = Self { __graph_id__: u64::MAX };

    /// Create a `GraphId` from a raw `u64`.
    pub const fn new(id: u64) -> Self {
        Self { __graph_id__: id }
    }

    /// Extract the raw `u64` value.
    pub const fn to_u64(self) -> u64 {
        self.__graph_id__
    }
}

impl fmt::Display for GraphId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.__graph_id__)
    }
}

impl From<u64> for GraphId {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

impl From<GraphId> for u64 {
    fn from(value: GraphId) -> Self {
        value.to_u64()
    }
}

impl From<GraphId> for AlgebraicValue {
    fn from(value: GraphId) -> Self {
        AlgebraicValue::product([value.to_u64().into()])
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::GroundSpacetimeType;

    #[test]
    fn graph_id_type_matches() {
        assert_eq!(AlgebraicType::graph_id(), GraphId::get_type());
        assert!(GraphId::get_type().is_graph_id());
        assert!(GraphId::get_type().is_special());
    }

    #[test]
    fn round_trip_u64() {
        let id = GraphId::new(42);
        let raw: u64 = id.into();
        let back = GraphId::from(raw);
        assert_eq!(id, back);
    }

    #[test]
    fn display_format() {
        assert_eq!("0", GraphId::MIN.to_string());
        assert_eq!("42", GraphId::new(42).to_string());
        assert_eq!("18446744073709551615", GraphId::MAX.to_string());
    }

    #[test]
    fn ordering() {
        let a = GraphId::new(1);
        let b = GraphId::new(2);
        assert!(a < b);
        assert!(b > a);
        assert_eq!(a, a);
        assert_ne!(a, b);
    }

    #[test]
    fn into_algebraic_value() {
        let id = GraphId::new(123);
        let av: AlgebraicValue = id.into();
        let prod = av.as_product().unwrap();
        assert_eq!(prod.elements.len(), 1);
        assert_eq!(prod.elements[0], AlgebraicValue::U64(123));
    }
}
