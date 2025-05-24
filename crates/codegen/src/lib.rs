use spacetimedb_schema::def::{ModuleDef, ReducerDef, ScopedTypeName, TableDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;

mod code_indenter;
pub mod csharp;
pub mod go;
pub mod rust;
pub mod typescript;
mod util;

pub use self::csharp::Csharp;
pub use self::go::Go;
pub use self::rust::Rust;
pub use self::typescript::TypeScript;
pub use util::AUTO_GENERATED_PREFIX;

pub fn generate(module: &ModuleDef, lang: &dyn Lang) -> Vec<(String, String)> {
    itertools::chain!(
        module
            .tables()
            .map(|tbl| (lang.table_filename(module, tbl), lang.generate_table(module, tbl))),
        module
            .types()
            .map(|typ| (lang.type_filename(&typ.name), lang.generate_type(module, typ))),
        util::iter_reducers(module).map(|reducer| {
            (
                lang.reducer_filename(&reducer.name),
                lang.generate_reducer(module, reducer),
            )
        }),
        lang.generate_globals(module),
    )
    .collect()
}

pub trait Lang {
    fn table_filename(&self, module: &ModuleDef, table: &TableDef) -> String;
    fn type_filename(&self, type_name: &ScopedTypeName) -> String;
    fn reducer_filename(&self, reducer_name: &Identifier) -> String;

    fn generate_table(&self, module: &ModuleDef, tbl: &TableDef) -> String;
    fn generate_type(&self, module: &ModuleDef, typ: &TypeDef) -> String;
    fn generate_reducer(&self, module: &ModuleDef, reducer: &ReducerDef) -> String;
    fn generate_globals(&self, module: &ModuleDef) -> Vec<(String, String)>;
}
