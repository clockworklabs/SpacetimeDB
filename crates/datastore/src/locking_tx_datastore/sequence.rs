use spacetimedb_data_structures::map::IntMap;
use spacetimedb_primitives::SequenceId;
use spacetimedb_schema::schema::SequenceSchema;
use spacetimedb_table::MemoryUsage;

#[derive(Debug, PartialEq)]
pub(super) struct Sequence {
    schema: SequenceSchema,
    pub(super) value: i128,
}

impl MemoryUsage for Sequence {
    fn heap_usage(&self) -> usize {
        // MEMUSE: intentionally ignoring schema
        let Self { schema: _, value } = self;
        value.heap_usage()
    }
}

impl Sequence {
    pub(super) fn new(schema: SequenceSchema) -> Self {
        Self {
            value: schema.start,
            schema,
        }
    }

    pub(super) fn id(&self) -> SequenceId {
        self.schema.sequence_id
    }

    /// Returns the next value in the sequence given the params.
    ///
    /// Examples:
    /// (min: 1, max: 10, increment: 1, value: 9) -> 1
    /// (min: 1, max: 10, increment: 20, value: 5) -> 5
    /// (min: 1, max: 10, increment: 3, value: 5) -> 8
    /// (min: 1, max: 10, increment: 3, value: 9) -> 2
    /// (min: 1, max: 10, increment: -3, value: 4) -> 1
    /// (min: 1, max: 10, increment: -3, value: 1) -> 8
    fn next_in_sequence(min: i128, max: i128, increment: i128, value: i128) -> i128 {
        // calculate the next value
        let mut next = value + increment;
        // handle wrapping around the sequence
        if increment > 0 {
            if next > max {
                next = min + (next - max - 1) % (max - min + 1);
            }
        } else if next < min {
            next = max - (min - next - 1) % (max - min + 1);
        }
        next
    }

    /// Returns the next value iff no allocation is needed.
    pub(super) fn gen_next_value(&mut self) -> Option<i128> {
        if self.needs_allocation() {
            return None;
        }
        let value = self.value;
        self.value = self.next_value();
        Some(value)
    }

    pub(super) fn allocated(&self) -> i128 {
        self.schema.allocated
    }

    fn next_value(&self) -> i128 {
        self.nth_value(1)
    }

    pub(super) fn nth_value(&self, n: usize) -> i128 {
        let mut value = self.value;
        for _ in 0..n {
            value = Self::next_in_sequence(
                self.schema.min_value,
                self.schema.max_value,
                self.schema.increment,
                value,
            );
        }
        value
    }

    /// The allocated value represents the place where the sequence would
    /// start from if the system memory was lost. Therefore we cannot generate
    /// the next value in the sequence without the risk of using the same
    /// value twice in two separate transactions.
    /// e.g.
    /// 1. incr = 1, allocated = 10, value = 10
    /// 2. next_value() -> 11
    /// 3. commit transaction
    /// 4. crash
    /// 5. restart
    /// 6. incr = 1 allocated = 10, value = 10
    /// 7. next_value() -> 11
    fn needs_allocation(&self) -> bool {
        // In order to yield a value, it must be strictly less than the allocation amount,
        // because on restart we will begin at the allocation amount.
        self.value >= self.schema.allocated
    }

    pub(super) fn set_allocation(&mut self, allocated: i128) {
        self.schema.allocated = allocated;
    }
}

/// A map of [`SequenceId`] -> [`Sequence`].
#[derive(Default, Debug)]
pub(super) struct SequencesState {
    sequences: IntMap<SequenceId, Sequence>,
}

impl MemoryUsage for SequencesState {
    fn heap_usage(&self) -> usize {
        let Self { sequences } = self;
        sequences.heap_usage()
    }
}

impl SequencesState {
    pub(super) fn get_sequence_mut(&mut self, seq_id: SequenceId) -> Option<&mut Sequence> {
        self.sequences.get_mut(&seq_id)
    }

    pub(super) fn insert(&mut self, seq: Sequence) {
        self.sequences.insert(seq.id(), seq);
    }

    pub(super) fn remove(&mut self, seq_id: SequenceId) -> Option<Sequence> {
        self.sequences.remove(&seq_id)
    }
}
