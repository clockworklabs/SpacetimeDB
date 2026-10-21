use std::hash::Hash;

use spacetimedb_lib::bsatn;
use spacetimedb_lib::buffer::HashWriter;
use spacetimedb_lib::db::raw_def::v10::{ExplicitNames, RawModuleDefV10, RawModuleDefV10Section};
use spacetimedb_lib::ser::Serialize;

use crate::def::ModuleDef;

impl Hash for ModuleDef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let mut raw = RawModuleDefV10::from(self.clone());
        canonicalize(&mut raw);
        bsatn::to_writer(HashWriter::from_mut(state), &raw).unwrap();
    }
}

/// Sorts every list that `RawModuleDefV10::from(ModuleDef)` fills from a `HashMap`,
/// so the hash doesn't depend on the process's hash seed.
fn canonicalize(raw: &mut RawModuleDefV10) {
    for section in &mut raw.sections {
        match section {
            RawModuleDefV10Section::Types(types) => sort_by_bsatn(types),
            RawModuleDefV10Section::Tables(tables) => {
                for table in tables.iter_mut() {
                    sort_by_bsatn(&mut table.indexes);
                    sort_by_bsatn(&mut table.constraints);
                    sort_by_bsatn(&mut table.sequences);
                }
                sort_by_bsatn(tables);
            }
            RawModuleDefV10Section::Schedules(schedules) => sort_by_bsatn(schedules),
            RawModuleDefV10Section::RowLevelSecurity(rls) => sort_by_bsatn(rls),
            RawModuleDefV10Section::ExplicitNames(names) => {
                let mut entries = std::mem::take(names).into_entries();
                sort_by_bsatn(&mut entries);
                *names = ExplicitNames::from_entries(entries);
            }
            RawModuleDefV10Section::Submodules(submodules) => {
                for submodule in submodules.iter_mut() {
                    canonicalize(&mut submodule.module);
                }
                sort_by_bsatn(submodules);
            }
            RawModuleDefV10Section::Migrations(migrations) => {
                for migration in migrations.iter_mut() {
                    canonicalize(&mut migration.dropped);
                }
            }
            _ => {}
        }
    }
}

fn sort_by_bsatn<T: Serialize>(items: &mut [T]) {
    items.sort_by_cached_key(|item| bsatn::to_vec(item).unwrap());
}

#[cfg(test)]
mod tests {
    use spacetimedb_lib::db::raw_def::v10::{RawModuleDefV10Builder, RawModuleDefV10Section};
    use spacetimedb_lib::db::raw_def::v9::btree;
    use spacetimedb_lib::{bsatn, AlgebraicType, ProductType};

    use super::*;

    // `HashMap` iteration order varies between processes, so check that canonicalizing
    // a reordered raw def gives the same bytes.
    #[test]
    fn canonical_bytes_are_independent_of_map_order() {
        let mut builder = RawModuleDefV10Builder::new();
        for i in 0..8 {
            builder
                .build_table_with_new_type(
                    format!("table_{i}"),
                    ProductType::from([("id", AlgebraicType::U64), ("value", AlgebraicType::String)]),
                    true,
                )
                .with_unique_constraint(0)
                .with_index_no_accessor_name(btree(0), format!("table_{i}_idx_0"))
                .with_index_no_accessor_name(btree(1), format!("table_{i}_idx_1"))
                .with_column_sequence(0)
                .finish();
        }
        let def: ModuleDef = builder.finish().try_into().unwrap();

        let mut raw = RawModuleDefV10::from(def);
        let mut reordered = raw.clone();
        for section in &mut reordered.sections {
            match section {
                RawModuleDefV10Section::Types(types) => types.reverse(),
                RawModuleDefV10Section::Tables(tables) => {
                    for table in tables.iter_mut() {
                        table.indexes.reverse();
                        table.constraints.reverse();
                    }
                    tables.reverse();
                }
                RawModuleDefV10Section::ExplicitNames(names) => {
                    let mut entries = std::mem::take(names).into_entries();
                    entries.reverse();
                    *names = ExplicitNames::from_entries(entries);
                }
                _ => {}
            }
        }
        assert_ne!(bsatn::to_vec(&raw).unwrap(), bsatn::to_vec(&reordered).unwrap());

        canonicalize(&mut raw);
        canonicalize(&mut reordered);
        assert_eq!(bsatn::to_vec(&raw).unwrap(), bsatn::to_vec(&reordered).unwrap());
    }
}
