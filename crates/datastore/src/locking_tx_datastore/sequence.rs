use spacetimedb_data_structures::map::IntMap;
use spacetimedb_primitives::SequenceId;
use spacetimedb_sats::memory_usage::MemoryUsage;
use spacetimedb_schema::schema::SequenceSchema;

#[derive(Debug, PartialEq)]
// TODO(cloutiertyler): The below was made `pub` for the datastore split. We should
// investigate if this should be private again.
pub struct Sequence {
    schema: SequenceSchema,
    // The next value to be returned by this sequence.
    value: i128,
    // The number we have persisted as a lower bound for the next restart.
    // This is the first value to be returned after a restart, so when we
    // reach this value, the user needs to call allocate_steps and update
    // the corresponding system table row.
    allocated: i128,
}

impl MemoryUsage for Sequence {
    fn heap_usage(&self) -> usize {
        // MEMUSE: intentionally ignoring schema
        self.value.heap_usage()
    }
}

impl Sequence {
    pub(super) fn new(schema: SequenceSchema, previous_allocation: Option<i128>) -> Self {
        if schema.start < schema.min_value || schema.start > schema.max_value {
            panic!(
                "Invalid sequence: start value {} is out of bounds for sequence with min_value {} and max_value {}",
                schema.start, schema.min_value, schema.max_value
            );
        }
        if schema.max_value <= schema.min_value {
            panic!("Invalid sequence: max_value must be greater than min_value");
        }
        if schema.increment == 0 {
            panic!("Invalid sequence: increment must be non-zero");
        }
        if schema.increment.unsigned_abs() >= (schema.max_value - schema.min_value) as u128 {
            panic!(
                "Invalid sequence: increment must be less than or equal to the range between min_value and max_value"
            );
        }
        let start = if let Some(prev) = previous_allocation {
            if prev < schema.min_value || prev > schema.max_value {
                // Previous versions set allocated to 0 as a default,
                // so we have this special case.
                if prev == 0 {
                    schema.start
                } else {
                    panic!(
                        "Invalid sequence: previous allocation value {prev} is out of bounds for sequence with min_value {} and max_value {}",
                        schema.min_value, schema.max_value
                    );
                }
            } else {
                prev
            }
        } else {
            schema.start
        };
        // We will always need to allocate before generating any values.
        Self {
            value: start,
            allocated: start,
            schema,
        }
    }

    /// Update the current value of the sequence.
    /// This is used on very specific occasions,
    /// such as cloning a sequence
    pub(super) fn update_value(&mut self, new_value: i128) {
        if new_value < self.schema.min_value || new_value > self.schema.max_value {
            panic!(
                "Invalid sequence update: new value {} is out of bounds for sequence with min_value {} and max_value {}",
                new_value, self.schema.min_value, self.schema.max_value
            );
        }
        self.value = new_value;
    }

    pub(super) fn get_value(&self) -> i128 {
        self.value
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

    fn next_value(&self) -> i128 {
        self.nth_value(1)
    }

    fn nth_value(&self, n: usize) -> i128 {
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
        // On restart we are allowed to begin at the allocation amount, so we stop before we
        // reach it. It is important that the allocated value is one that would be returned
        // by the sequence, so we can use equality here. Otherwise, to handle wrapping sequences
        // correctly, we would need to check if the next value is on the same side of the allocated
        // value as the current value, which seems more complex.
        self.value == self.allocated
    }

    /// Allocate up to `steps` new values in the sequence. This returns the new allocated value,
    /// which should be written to the corresponding system table row, so that we start generating
    /// at that value on the next restart.
    /// This may allocate fewer steps if it is possible to fully loop around the sequence in that
    /// many steps.
    pub(super) fn allocate_steps(&mut self, steps: usize) -> i128 {
        if !self.needs_allocation() {
            // No allocation needed, return the current allocation.
            return self.allocated;
        }
        let original_allocation = self.allocated;
        for _ in 0..steps {
            let next = Self::next_in_sequence(
                self.schema.min_value,
                self.schema.max_value,
                self.schema.increment,
                self.allocated,
            );
            if next == original_allocation {
                // We have looped all the way around, stop here.
                break;
            }
            self.allocated = next;
        }
        if self.needs_allocation() {
            // This should only be possible if |max - min| == |increment|.
            // This should be unreachable, since `new` will panic if this would happen.
            panic!("Unable to allocate new sequence value. Sequence parameters are invalid.")
        }
        self.allocated
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

#[cfg(test)]
mod tests {

    use crate::locking_tx_datastore::sequence::Sequence;
    use spacetimedb_primitives::{ColId, SequenceId, TableId};
    use spacetimedb_schema::schema::SequenceSchema;

    #[derive(Clone, Copy)]
    struct SequenceParams {
        min: i128,
        max: i128,
        increment: i128,
        start: i128,
        previous_allocation: Option<i128>,
    }
    fn make_test_sequence_schema(params: SequenceParams) -> Sequence {
        let schema = SequenceSchema {
            sequence_id: SequenceId(1),
            min_value: params.min,
            max_value: params.max,
            increment: params.increment,
            start: params.start,
            col_pos: ColId(1),
            table_id: TableId(1),
            sequence_name: "test_sequence".to_owned().into_boxed_str(),
        };
        Sequence::new(schema, params.previous_allocation)
    }
    #[test]
    fn test_double_allocation_noops() {
        // A simple sequence that increments by 1 from 1 to 10.
        let seq_params = SequenceParams {
            min: 1,
            max: 10,
            increment: 1,
            start: 1,
            previous_allocation: None,
        };
        let mut seq = make_test_sequence_schema(seq_params);
        assert_eq!(seq.gen_next_value(), None);
        let new_alloc = seq.allocate_steps(1);
        assert_eq!(new_alloc, 2);
        // Check that trying to allocate again will do nothing if we haven't exhausted
        // the existing allocation.
        let new_alloc = seq.allocate_steps(2);
        assert_eq!(new_alloc, 2);
        assert_eq!(seq.gen_next_value(), Some(1));
        assert_eq!(seq.gen_next_value(), None);
    }

    #[test]
    fn test_simple_loop() {
        // A simple sequence that increments by 1 from 1 to 10.
        let seq_params = SequenceParams {
            min: 1,
            max: 10,
            increment: 1,
            start: 1,
            previous_allocation: None,
        };
        let mut seq = make_test_sequence_schema(seq_params);
        assert_sequence_works(&mut seq, seq_params, seq_params.start, 100);
    }

    #[test]
    fn test_loop_with_odd_increment() {
        // A simple sequence that increments by 1 from 1 to 10.
        let seq_params = SequenceParams {
            min: 1,
            max: 100,
            increment: 3,
            start: 1,
            previous_allocation: None,
        };
        let mut seq = make_test_sequence_schema(seq_params);
        assert_sequence_works(&mut seq, seq_params, seq_params.start, 100);
    }

    #[test]
    fn test_loop_with_odd_increment_and_even_start() {
        // A simple sequence that increments by 1 from 1 to 10.
        let seq_params = SequenceParams {
            min: 1,
            max: 100,
            increment: 3,
            start: 10,
            previous_allocation: None,
        };
        let mut seq = make_test_sequence_schema(seq_params);
        assert_sequence_works(&mut seq, seq_params, seq_params.start, 100);
    }

    #[test]
    fn test_loop_with_fully_negative_range() {
        // A simple sequence that increments by 1 from 1 to 10.
        let seq_params = SequenceParams {
            min: -100,
            max: -1,
            increment: 3,
            start: -50,
            previous_allocation: None,
        };
        let mut seq = make_test_sequence_schema(seq_params);
        assert_sequence_works(&mut seq, seq_params, seq_params.start, 100);
    }

    #[test]
    fn test_simple_negative_loop() {
        // A simple sequence that increments by 1 from 1 to 10.
        let seq_params = SequenceParams {
            min: 1,
            max: 10,
            increment: -1,
            start: 1,
            previous_allocation: None,
        };
        let mut seq = make_test_sequence_schema(seq_params);
        assert_sequence_works(&mut seq, seq_params, seq_params.start, 100);
    }

    // This function tests that a sequence works correctly by generating `steps` values.
    // This uses a different way of calculating the next value is that will be more likely to hit
    // overflow issues, so it won't work if steps * increment overflows i128.
    fn assert_sequence_works(seq: &mut Sequence, seq_params: SequenceParams, initial_value: i128, steps: i128) {
        for i in 0..steps {
            if seq.needs_allocation() {
                seq.allocate_steps(10);
            }
            let val = seq.gen_next_value().unwrap();
            assert!(
                val >= seq_params.min && val <= seq_params.max,
                "Generated value {val} out of bounds [{}, {}]",
                seq_params.min,
                seq_params.max
            );

            let range = seq_params.max - seq_params.min + 1;
            let raw_next = initial_value + i * seq_params.increment;
            // This is an alternate way to handling wrapping. Since the mod operator can return
            // negative values in rust, we do the `(n % max + max) % max` trick.
            let wrapped_next = ((raw_next - seq_params.min) % range + range) % range + seq_params.min;
            assert_eq!(val, wrapped_next, "Failed at iteration {i} (0 indexed)");
        }
    }

    #[test]
    fn test_restarting_after_allocation() {
        // A simple sequence that increments by 1 from 1 to 100.
        let seq_params = SequenceParams {
            min: 1,
            max: 100,
            increment: 1,
            start: 1,
            previous_allocation: None,
        };
        let mut seq = make_test_sequence_schema(seq_params);
        assert!(seq.needs_allocation());
        // We are picking a number lower than the max to avoid wrapping.
        let new_allocation = seq.allocate_steps(40);
        let mut previous_value = 0;
        // Keep going until we exhaust the allocation.
        while !seq.needs_allocation() {
            previous_value = seq.gen_next_value().unwrap();
            // Since this won't wrap, we should get values strictly less than the allocation.
            assert!(previous_value <= new_allocation);
        }
        assert_eq!(previous_value, new_allocation - 1);
        let restarted_params = SequenceParams {
            previous_allocation: Some(new_allocation),
            ..seq_params
        };
        let mut restarted_seq = make_test_sequence_schema(restarted_params);
        assert!(restarted_seq.needs_allocation());
        restarted_seq.allocate_steps(1);
        let next_value = restarted_seq.gen_next_value().unwrap();
        assert_eq!(next_value, new_allocation);
    }

    #[test]
    fn test_first_value_is_prev_allocation() {
        // A simple sequence that increments by 1 from 1 to 100.
        // The start is set to 1, but this is overridden by the previous allocation of 7.
        let seq_params = SequenceParams {
            min: 1,
            max: 100,
            increment: 1,
            start: 1,
            previous_allocation: Some(7),
        };
        let mut seq = make_test_sequence_schema(seq_params);
        assert!(seq.needs_allocation());
        // We are picking a number lower than the max to avoid wrapping.
        let _ = seq.allocate_steps(1);
        assert_eq!(7, seq.gen_next_value().unwrap());
    }
    #[test]
    #[should_panic(expected = "Invalid sequence:")]
    fn test_increment_range() {
        // This is a sequence that would only ever be able to generate one value.
        let seq_params = SequenceParams {
            min: 1,
            max: 10,
            increment: 10,
            start: 1,
            previous_allocation: None,
        };
        make_test_sequence_schema(seq_params);
    }

    #[test]
    #[should_panic(expected = "Invalid sequence:")]
    fn test_previous_out_of_range() {
        // This is a sequence that would only ever be able to generate one value.
        let seq_params = SequenceParams {
            min: 1,
            max: 10,
            increment: 1,
            start: 1,
            previous_allocation: Some(100),
        };
        make_test_sequence_schema(seq_params);
    }

    #[test]
    fn test_previous_out_of_range_but_zero() {
        // This is a sequence that would only ever be able to generate one value.
        let seq_params = SequenceParams {
            min: 1,
            max: 10,
            increment: 1,
            start: 1,
            previous_allocation: Some(0),
        };
        let mut seq = make_test_sequence_schema(seq_params);
        assert!(seq.needs_allocation());
        seq.allocate_steps(1);
        assert_eq!(1, seq.gen_next_value().unwrap());
    }

    #[test]
    #[should_panic(expected = "Invalid sequence:")]
    fn test_start_out_of_range() {
        // This is a sequence that would only ever be able to generate one value.
        let seq_params = SequenceParams {
            min: 1,
            max: 10,
            increment: 1,
            start: 100,
            previous_allocation: None,
        };
        make_test_sequence_schema(seq_params);
    }
}
