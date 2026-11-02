//! The bindings of a subscription template: one row per distinct set of parameter values

use spacetimedb_lib::{AlgebraicType, AlgebraicValue, ProductValue};
use spacetimedb_physical_plan::plan::PARAM_BINDING_FIRST;
use spacetimedb_physical_plan::plan::{BindingIndexId, ParamResolver, ParamSlot};
use spacetimedb_primitives::ColList;
use spacetimedb_sats::Typespace;
use std::collections::{BTreeMap, BTreeSet};

/// Identifies one binding of a template
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BindingId(pub u64);

impl BindingId {
    /// The id of a binding row, which is always its first column
    pub fn of_row(row: &ProductValue) -> Self {
        let id = row.elements.first().and_then(AlgebraicValue::as_u64);
        Self(*id.expect("binding rows start with their id"))
    }
}

/// The distinct parameter values of a template, one row per binding.
///
/// Rows are `[id, values..]`, so the id can be read without knowing how many values there are.
#[derive(Debug)]
pub struct BindingRelation {
    /// The type of each value, starting at column 1
    value_types: Vec<AlgebraicType>,
    /// In id order, so that a scan of every binding is deterministic
    rows: BTreeMap<BindingId, ProductValue>,
    /// Clients that subscribe with equal values share one binding
    ids: BTreeMap<ProductValue, BindingId>,
    /// Indexed by [`BindingIndexId`]
    indexes: Vec<BindingIndex>,
    /// Ids are never reused, so a stale id can't name a newer binding
    id_next: u64,
}

#[derive(Debug)]
struct BindingIndex {
    /// Columns of the binding rows, so never 0
    cols: ColList,
    /// A set per key, so a probe is in id order and a removal doesn't shift the other ids
    entries: BTreeMap<AlgebraicValue, BTreeSet<BindingId>>,
}

impl BindingIndex {
    /// The key of a binding row
    fn key_of_row(&self, row: &ProductValue) -> AlgebraicValue {
        let values = self.cols.iter().map(|column| row.elements.get(column.idx()));
        let values = values.map(|value| value.expect("index columns are checked in `new`").clone());
        self.key(values)
    }

    /// Rows and probes both build their keys here, so they can't disagree on the shape
    fn key(&self, values: impl IntoIterator<Item = AlgebraicValue>) -> AlgebraicValue {
        let column_count = self.cols.len();
        assert!(column_count > 0, "a binding key needs a value");
        let mut values = values.into_iter();
        // One column is a bare value, like `ProductValue::project`
        let key = if column_count == 1 {
            values.next()
        } else {
            let product = ProductValue::from_iter(values.by_ref().take(column_count as usize));
            let complete = product.elements.len() as u64 == column_count as u64;
            complete.then_some(AlgebraicValue::Product(product))
        };
        let Some(key) = key.filter(|_| values.next().is_none()) else {
            panic!("wrong number of key values");
        };
        key
    }
}

impl BindingRelation {
    /// An empty relation with these value types and indexes
    pub fn new(value_types: Vec<AlgebraicType>, indexes: Vec<ColList>) -> Self {
        // Value `i` is read through slot `PARAM_BINDING_FIRST + i`
        let slot_count = (u16::MAX - PARAM_BINDING_FIRST.0) as u64 + 1;
        let value_count = value_types.len() as u64;
        assert!(value_count <= slot_count, "too many binding values: {value_count}");
        let column_count = value_count + 1;
        assert!(column_count <= u16::MAX as u64);
        assert!(indexes.len() as u64 <= u32::MAX as u64, "too many binding indexes");
        for ty in &value_types {
            // Values are checked without a typespace, which a ref would need
            assert!(!ty.contains_refs(), "binding types must be resolved: {ty:?}");
        }

        let mut built = Vec::with_capacity(indexes.len());
        for cols in indexes {
            assert!(!cols.is_empty(), "a binding index needs a column");
            for column in cols.iter() {
                // Probes compare parameter values, and the id isn't one
                assert!(column.0 != 0, "binding ids are not indexed: {cols:?}");
                let in_range = (column.0 as u64) < column_count;
                assert!(in_range, "no binding column {}: {cols:?}", column.0);
            }
            built.push(BindingIndex {
                cols,
                entries: BTreeMap::new(),
            });
        }

        Self {
            value_types,
            rows: BTreeMap::new(),
            ids: BTreeMap::new(),
            indexes: built,
            id_next: 0,
        }
    }

    /// The number of bindings
    pub fn len(&self) -> u64 {
        let len = self.rows.len() as u64;
        // Every binding took an id, and ids are never reused
        assert!(len <= self.id_next);
        assert_eq!(self.rows.len(), self.ids.len());
        len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the binding for these values, adding it if it is new.
    ///
    /// Callers type-check the values against the template first, so a mismatch here is a bug.
    pub fn insert(&mut self, values: ProductValue) -> BindingId {
        assert_eq!(
            values.elements.len(),
            self.value_types.len(),
            "wrong number of binding values: {values:?}"
        );
        for (value, ty) in values.elements.iter().zip(&self.value_types) {
            assert_type(value, ty);
        }
        if let Some(id) = self.ids.get(&values) {
            return *id;
        }

        assert!(self.id_next < u64::MAX, "binding ids are exhausted");
        let id = BindingId(self.id_next);
        self.id_next += 1;

        let row: ProductValue = std::iter::once(AlgebraicValue::U64(id.0))
            .chain(values.elements.iter().cloned())
            .collect();
        for index in &mut self.indexes {
            let key = index.key_of_row(&row);
            let inserted = index.entries.entry(key).or_default().insert(id);
            assert!(inserted);
        }

        let previous = self.ids.insert(values, id);
        assert!(previous.is_none());
        let previous = self.rows.insert(id, row);
        assert!(previous.is_none());
        id
    }

    /// The binding for these values, if there is one
    pub fn find(&self, values: &ProductValue) -> Option<BindingId> {
        let id = self.ids.get(values).copied();
        // `insert` and `remove` keep `ids` and `rows` in step
        assert!(id.is_none_or(|id| self.rows.contains_key(&id)));
        id
    }

    /// Removes a binding that is no longer used
    pub fn remove(&mut self, id: BindingId) {
        let Some(row) = self.rows.remove(&id) else {
            panic!("unknown binding: {id:?}");
        };
        assert_eq!(BindingId::of_row(&row), id);

        for index in &mut self.indexes {
            let key = index.key_of_row(&row);
            let ids = index.entries.get_mut(&key).expect("every row is in every index");
            let removed = ids.remove(&id);
            assert!(removed, "every row is in every index");
            if ids.is_empty() {
                index.entries.remove(&key);
            }
        }

        let values: ProductValue = row.elements[1..].iter().cloned().collect();
        let removed = self.ids.remove(&values);
        assert_eq!(removed, Some(id));
    }

    /// The row of a binding
    pub fn get(&self, id: BindingId) -> &ProductValue {
        let Some(row) = self.rows.get(&id) else {
            panic!("unknown binding: {id:?}");
        };
        assert_eq!(BindingId::of_row(row), id);
        row
    }

    /// Every binding row, in id order
    pub fn iter(&self) -> impl Iterator<Item = &ProductValue> {
        assert_eq!(self.rows.len(), self.ids.len());
        self.rows.values()
    }

    /// The bindings whose indexed columns equal `key`, one value per column, in id order
    pub fn probe<Key>(&self, index: BindingIndexId, key: Key) -> &BTreeSet<BindingId>
    where
        Key: IntoIterator<Item = AlgebraicValue>,
    {
        const NONE: &BTreeSet<BindingId> = &BTreeSet::new();
        let Some(index) = self.indexes.get(index.0 as usize) else {
            panic!("unknown binding index: {index:?}");
        };
        let ids = index.entries.get(&index.key(key));
        // `remove` drops a key with its last id
        assert!(ids.is_none_or(|ids| !ids.is_empty()));
        ids.unwrap_or(NONE)
    }
}

/// Resolves the binding slots of a template's plan from one binding row
pub struct BindingParams<'a> {
    row: &'a ProductValue,
}

impl<'a> BindingParams<'a> {
    pub fn new(row: &'a ProductValue) -> Self {
        let id = row.elements.first();
        assert!(matches!(id, Some(AlgebraicValue::U64(_))), "not a binding row: {row:?}");
        Self { row }
    }
}

impl ParamResolver for BindingParams<'_> {
    fn resolve_param(&self, param: ParamSlot, ty: &AlgebraicType) -> AlgebraicValue {
        // Every parameter of a template is a binding value, so there is nothing to fall back to
        let Some(i) = param.binding_index() else {
            panic!("not a binding slot: {}", param.0);
        };
        let col = i as u32 + 1;
        let Some(value) = self.row.elements.get(col as usize) else {
            panic!("binding slot {} has no value: {:?}", param.0, self.row);
        };
        assert_type(value, ty);
        value.clone()
    }
}

/// Asserts that a value has a type, which `new` checked has no refs
fn assert_type(value: &AlgebraicValue, ty: &AlgebraicType) {
    // `type_check` accepts bounds as any type. Only the outer value is checked, since range
    // scans build bounds on their own and never nest them inside a value.
    let bound = matches!(value, AlgebraicValue::Min | AlgebraicValue::Max);
    assert!(!bound, "binding value is a top-level bound: {value:?}");
    let valid = ty.type_check(value, Typespace::EMPTY);
    assert!(valid, "binding value has the wrong type: {value:?} is not {ty:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExecutionParams;
    use spacetimedb_lib::Identity;
    use spacetimedb_physical_plan::plan::PARAM_SENDER;

    const REGION: BindingIndexId = BindingIndexId(0);
    const REGION_RANK: BindingIndexId = BindingIndexId(1);

    /// Bindings of `region = ? and rank = ?`
    fn region_rank() -> BindingRelation {
        BindingRelation::new(
            vec![AlgebraicType::String, AlgebraicType::U32],
            vec![ColList::new(1.into()), ColList::from_iter([1u16, 2])],
        )
    }

    fn values(region: &str, rank: u32) -> ProductValue {
        ProductValue::from_iter([AlgebraicValue::String(region.into()), AlgebraicValue::U32(rank)])
    }

    fn west_3() -> Vec<AlgebraicValue> {
        vec![AlgebraicValue::String("west".into()), AlgebraicValue::U32(3)]
    }

    fn region(region: &str) -> Vec<AlgebraicValue> {
        vec![AlgebraicValue::String(region.into())]
    }

    #[test]
    fn insert_reuses_equal_values() {
        let mut bindings = region_rank();
        let b1 = bindings.insert(values("west", 3));
        let b2 = bindings.insert(values("west", 5));
        assert_ne!(b1, b2);
        assert_eq!(bindings.insert(values("west", 3)), b1);
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn find_returns_the_binding_of_equal_values() {
        let mut bindings = region_rank();
        let b1 = bindings.insert(values("west", 3));
        let b2 = bindings.insert(values("west", 5));
        assert_eq!(bindings.find(&values("west", 3)), Some(b1));
        assert_eq!(bindings.find(&values("west", 5)), Some(b2));
        assert_eq!(bindings.find(&values("east", 3)), None);
        bindings.remove(b1);
        assert_eq!(bindings.find(&values("west", 3)), None);
    }

    #[test]
    fn iter_is_in_id_order() {
        let mut bindings = region_rank();
        let ids = [
            bindings.insert(values("west", 3)),
            bindings.insert(values("east", 3)),
            bindings.insert(values("north", 3)),
        ];
        let scanned: Vec<_> = bindings.iter().map(BindingId::of_row).collect();
        assert_eq!(scanned, ids);
    }

    #[test]
    fn probe_finds_matching_bindings() {
        let mut bindings = region_rank();
        let b1 = bindings.insert(values("west", 3));
        let b2 = bindings.insert(values("west", 5));
        let b3 = bindings.insert(values("east", 3));
        assert_eq!(bindings.probe(REGION, region("west")), &BTreeSet::from([b1, b2]));
        assert_eq!(bindings.probe(REGION, region("east")), &BTreeSet::from([b3]));
        assert_eq!(bindings.probe(REGION_RANK, west_3()), &BTreeSet::from([b1]));
        assert!(bindings.probe(REGION, region("north")).is_empty());
    }

    #[test]
    fn remove_drops_the_row_and_its_index_entries() {
        let mut bindings = region_rank();
        let b1 = bindings.insert(values("west", 3));
        let b2 = bindings.insert(values("west", 5));
        bindings.remove(b1);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings.probe(REGION, region("west")), &BTreeSet::from([b2]));
        assert!(bindings.probe(REGION_RANK, west_3()).is_empty());
        let scanned: Vec<_> = bindings.iter().map(BindingId::of_row).collect();
        assert_eq!(scanned, [b2]);
    }

    #[test]
    fn ids_are_not_reused() {
        let mut bindings = region_rank();
        let b1 = bindings.insert(values("west", 3));
        bindings.remove(b1);
        let b2 = bindings.insert(values("west", 3));
        assert_ne!(b1, b2);
        assert_eq!(bindings.probe(REGION_RANK, west_3()), &BTreeSet::from([b2]));
    }

    #[test]
    fn params_read_binding_values() {
        let mut bindings = region_rank();
        let id = bindings.insert(values("west", 3));
        let params = BindingParams::new(bindings.get(id));
        let region = params.resolve_param(ParamSlot::binding(0), &AlgebraicType::String);
        let rank = params.resolve_param(ParamSlot::binding(1), &AlgebraicType::U32);
        assert_eq!(region, AlgebraicValue::String("west".into()));
        assert_eq!(rank, AlgebraicValue::U32(3));
    }

    #[test]
    #[should_panic(expected = "not a binding slot")]
    fn params_reject_built_in_slots() {
        let mut bindings = region_rank();
        let id = bindings.insert(values("west", 3));
        BindingParams::new(bindings.get(id)).resolve_param(PARAM_SENDER, &AlgebraicType::String);
    }

    #[test]
    #[should_panic(expected = "wrong type")]
    fn params_reject_the_wrong_type() {
        let mut bindings = region_rank();
        let id = bindings.insert(values("west", 3));
        let params = BindingParams::new(bindings.get(id));
        params.resolve_param(ParamSlot::binding(0), &AlgebraicType::U32);
    }

    /// The reason binding slots have their own range
    #[test]
    #[should_panic(expected = "unknown physical plan parameter slot")]
    fn execution_params_reject_binding_slots() {
        let params = ExecutionParams::from_sender(Identity::ZERO);
        params.resolve_param(ParamSlot::binding(0), &AlgebraicType::String);
    }

    #[test]
    #[should_panic(expected = "wrong number of key values")]
    fn probe_rejects_a_partial_key() {
        region_rank().probe(REGION_RANK, region("west"));
    }

    #[test]
    #[should_panic(expected = "wrong number of key values")]
    fn probe_rejects_an_extra_value() {
        region_rank().probe(REGION, west_3());
    }

    #[test]
    #[should_panic(expected = "binding ids are not indexed")]
    fn new_rejects_an_index_on_the_id() {
        BindingRelation::new(vec![AlgebraicType::U32], vec![ColList::new(0.into())]);
    }

    #[test]
    fn params_read_composite_values() {
        let mut bindings = BindingRelation::new(vec![AlgebraicType::identity()], vec![]);
        let identity = ProductValue::from_iter([Identity::ZERO.into()]);
        let id = bindings.insert(identity);
        let params = BindingParams::new(bindings.get(id));
        let value = params.resolve_param(ParamSlot::binding(0), &AlgebraicType::identity());
        assert_eq!(value, Identity::ZERO.into());
    }

    #[test]
    #[should_panic(expected = "wrong type")]
    fn insert_rejects_the_wrong_composite_type() {
        let mut bindings = BindingRelation::new(vec![AlgebraicType::identity()], vec![]);
        bindings.insert(ProductValue::from_iter([AlgebraicValue::U32(1)]));
    }
}
