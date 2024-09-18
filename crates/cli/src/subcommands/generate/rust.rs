use super::code_indenter::CodeIndenter;
use convert_case::{Case, Casing};
use itertools::Itertools;
use spacetimedb_lib::sats::AlgebraicTypeRef;
use spacetimedb_primitives::ColList;
use spacetimedb_schema::def::{ModuleDef, ReducerDef, ScopedTypeName, TableDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_schema::type_for_generate::{
    AlgebraicTypeDef, AlgebraicTypeUse, PlainEnumTypeDef, PrimitiveType, ProductTypeDef, SumTypeDef,
};
use std::collections::BTreeSet;
use std::fmt::{self, Write};
use std::ops::Deref;

type Indenter = CodeIndenter<String>;

/// Pairs of (module_name, TypeName).
type Imports = BTreeSet<(String, String)>;

pub struct Rust;

impl super::Lang for Rust {
    fn table_filename(&self, module: &ModuleDef, table: &TableDef) -> String {
        let (name, _) = module.type_def_from_ref(table.product_type_ref).unwrap();
        collect_case(Case::Snake, name.name_segments()) + ".rs"
    }

    fn type_filename(&self, type_name: &ScopedTypeName) -> String {
        collect_case(Case::Snake, type_name.name_segments()) + ".rs"
    }

    fn reducer_filename(&self, reducer_name: &Identifier) -> String {
        reducer_name.deref().to_case(Case::Snake) + "_reducer.rs"
    }

    fn generate_table(&self, module: &ModuleDef, _namespace: &str, table: &TableDef) -> String {
        autogen_rust_table(module, table)
    }

    fn generate_type(&self, module: &ModuleDef, _namespace: &str, typ: &TypeDef) -> String {
        let name = &collect_case(Case::Pascal, typ.name.name_segments());
        match &module.typespace_for_generate()[typ.ty] {
            AlgebraicTypeDef::Product(product) => autogen_rust_tuple(module, name, product),
            AlgebraicTypeDef::Sum(sum_type) => autogen_rust_sum(module, name, sum_type),
            AlgebraicTypeDef::PlainEnum(plain_enum) => autogen_rust_plain_enum(name, plain_enum),
        }
    }

    fn generate_reducer(&self, module: &ModuleDef, _namespace: &str, reducer: &ReducerDef) -> String {
        autogen_rust_reducer(module, reducer)
    }

    fn generate_globals(&self, module: &ModuleDef, _namespace: &str) -> Vec<(String, String)> {
        autogen_rust_globals(module)
    }
}

fn collect_case<'a>(case: Case, segs: impl Iterator<Item = &'a Identifier>) -> String {
    segs.map(|s| s.deref().to_case(case)).join(case.delim())
}

fn write_type(module: &ModuleDef, out: &mut Indenter, ty: &AlgebraicTypeUse) {
    write_type_generic(module, out, ty).ok();
}

pub fn write_type_generic<W: Write>(module: &ModuleDef, out: &mut W, ty: &AlgebraicTypeUse) -> fmt::Result {
    match ty {
        AlgebraicTypeUse::Unit => write!(out, "()")?,
        AlgebraicTypeUse::Never => write!(out, "std::convert::Infallible")?,
        AlgebraicTypeUse::Identity => write!(out, "Identity")?,
        AlgebraicTypeUse::Address => write!(out, "Address")?,
        AlgebraicTypeUse::ScheduleAt => write!(out, "ScheduleAt")?,
        AlgebraicTypeUse::Option(inner_ty) => {
            write!(out, "Option::<")?;
            write_type_generic(module, out, inner_ty)?;
            write!(out, ">")?;
        }
        AlgebraicTypeUse::Primitive(prim) => match prim {
            PrimitiveType::Bool => write!(out, "bool")?,
            PrimitiveType::I8 => write!(out, "i8")?,
            PrimitiveType::U8 => write!(out, "u8")?,
            PrimitiveType::I16 => write!(out, "i16")?,
            PrimitiveType::U16 => write!(out, "u16")?,
            PrimitiveType::I32 => write!(out, "i32")?,
            PrimitiveType::U32 => write!(out, "u32")?,
            PrimitiveType::I64 => write!(out, "i64")?,
            PrimitiveType::U64 => write!(out, "u64")?,
            PrimitiveType::I128 => write!(out, "i128")?,
            PrimitiveType::U128 => write!(out, "u128")?,
            PrimitiveType::I256 => write!(out, "i256")?,
            PrimitiveType::U256 => write!(out, "u256")?,
            PrimitiveType::F32 => write!(out, "f32")?,
            PrimitiveType::F64 => write!(out, "f64")?,
        },
        AlgebraicTypeUse::String => write!(out, "String")?,
        AlgebraicTypeUse::Array(elem_ty) => {
            write!(out, "Vec::<")?;
            write_type_generic(module, out, elem_ty)?;
            write!(out, ">")?;
        }
        AlgebraicTypeUse::Map { key, value } => {
            // TODO: Should `AlgebraicType::Map` translate to `HashMap`? This requires
            //       that any map-key type implement `Hash`. We'll have to derive hash
            //       on generated types, and notably, `HashMap` is not itself `Hash`,
            //       so any type that holds a `Map` cannot derive `Hash` and cannot
            //       key a `Map`.
            // UPDATE: No, `AlgebraicType::Map` is supposed to be `BTreeMap`. Fix this.
            //         This will require deriving `Ord` for generated types,
            //         and is likely to be a big headache.
            write!(out, "HashMap::<")?;
            write_type_generic(module, out, key)?;
            write!(out, ", ")?;
            write_type_generic(module, out, value)?;
            write!(out, ">")?;
        }
        AlgebraicTypeUse::Ref(r) => {
            write!(out, "{}", type_name(module, *r))?;
        }
    }
    Ok(())
}

// This is (effectively) duplicated in [typescript.rs] as `typescript_typename` and in
// [csharp.rs] as `csharp_typename`, and should probably be lifted to a shared utils
// module.
fn type_name(module: &ModuleDef, typeref: AlgebraicTypeRef) -> String {
    let (name, _def) = module.type_def_from_ref(typeref).unwrap();
    collect_case(Case::Pascal, name.name_segments())
}

fn print_lines(output: &mut Indenter, lines: &[&str]) {
    for line in lines {
        writeln!(output, "{line}");
    }
}

// This is (effectively) duplicated in both [typescript.rs] and [csharp.rs], and should
// probably be lifted to a shared module.
const AUTO_GENERATED_FILE_COMMENT: &[&str] = &[
    "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE",
    "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.",
    "",
];

fn print_auto_generated_file_comment(output: &mut Indenter) {
    print_lines(output, AUTO_GENERATED_FILE_COMMENT);
}

const ALLOW_UNUSED: &str = "#[allow(unused)]";
const ALLOW_UNUSED_IMPORTS: &str = "#![allow(unused_imports)]";

const SPACETIMEDB_IMPORTS: &[&str] = &[
    "use spacetimedb_sdk::{",
    "\tAddress, ScheduleAt,",
    "\tsats::{ser::Serialize, de::Deserialize, i256, u256},",
    "\ttable::{TableType, TableIter, TableWithPrimaryKey},",
    "\treducer::{Reducer, ReducerCallbackId, Status},",
    "\tidentity::Identity,",
    // The `Serialize` and `Deserialize` macros depend on `spacetimedb_lib` existing in
    // the root namespace.
    "\tspacetimedb_lib,",
    "\tanyhow::{Result, anyhow},",
    "};",
];

fn print_spacetimedb_imports(output: &mut Indenter) {
    print_lines(output, SPACETIMEDB_IMPORTS);
}

fn print_file_header(output: &mut Indenter) {
    print_auto_generated_file_comment(output);
    write!(output, "{ALLOW_UNUSED_IMPORTS}");
    print_spacetimedb_imports(output);
}

// TODO: figure out if/when sum types should derive:
// - Clone
// - Debug
// - Copy
// - PartialEq, Eq
// - Hash
//    - Complicated because `HashMap` is not `Hash`.
// - others?

const ENUM_DERIVES: &[&str] = &["#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]"];

fn print_enum_derives(output: &mut Indenter) {
    print_lines(output, ENUM_DERIVES);
}

/// Generate a file which defines an `enum` corresponding to the `sum_type`.
pub fn autogen_rust_sum(module: &ModuleDef, name: &str, sum_type: &SumTypeDef) -> String {
    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    let sum_type_name = name.replace("r#", "").to_case(Case::Pascal);

    print_file_header(out);

    // Pass this file into `gen_and_print_imports` to avoid recursively importing self
    // for recursive types.
    let file_name = name.to_case(Case::Snake);
    let this_file = (file_name.as_str(), name);

    // For some reason, deref coercion doesn't work on `&sum_type.variants` here - rustc
    // wants to pass it as `&Vec<_>`, not `&[_]`. The slicing index `[..]` forces passing
    // as a slice.
    gen_and_print_imports(module, out, &sum_type.variants[..], this_file);

    out.newline();

    print_enum_derives(out);

    write!(out, "pub enum {sum_type_name} ");

    out.delimited_block(
        "{",
        |out| {
            for (name, ty) in &*sum_type.variants {
                write_enum_variant(module, out, name, ty);
                out.newline();
            }
        },
        "}\n",
    );

    output.into_inner()
}

fn write_enum_variant(module: &ModuleDef, out: &mut Indenter, name: &Identifier, ty: &AlgebraicTypeUse) {
    let name = name.deref().to_case(Case::Pascal);
    write!(out, "{name}");
    match ty {
        AlgebraicTypeUse::Unit => {
            // If the contained type is the unit type, i.e. this variant has no members,
            // write it without parens or braces, like
            // ```
            // Foo,
            // ```
            writeln!(out, ",");
        }
        otherwise => {
            // If the contained type is not a product, i.e. this variant has a single
            // member, write it tuple-style, with parens.
            write!(out, "(");
            write_type(module, out, otherwise);
            write!(out, "),");
        }
    }
}

/// Generate a file which defines an `enum` corresponding to the `sum_type`.
pub fn autogen_rust_plain_enum(name: &str, plain_enum: &PlainEnumTypeDef) -> String {
    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    let sum_type_name = name.replace("r#", "").to_case(Case::Pascal);

    print_file_header(out);
    out.newline();

    print_enum_derives(out);

    write!(out, "pub enum {sum_type_name} ");

    out.delimited_block(
        "{",
        |out| {
            for name in &plain_enum.variants[..] {
                writeln!(out, "{name},");
            }
        },
        "}\n",
    );

    output.into_inner()
}

/// Generate a file which defines a `struct` corresponding to the `product` type.
pub fn autogen_rust_tuple(module: &ModuleDef, name: &str, product: &ProductTypeDef) -> String {
    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    let type_name = name.to_case(Case::Pascal);

    begin_rust_struct_def_shared(module, out, &type_name, product);

    output.into_inner()
}

/// Generate a file which defines a `struct` corresponding to the `table`'s `ProductType`,
/// and implements `spacetimedb_sdk::table::TableType` for it.
pub fn autogen_rust_table(module: &ModuleDef, table: &TableDef) -> String {
    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    let type_name = type_name(module, table.product_type_ref);

    let product_def = module.typespace_for_generate()[table.product_type_ref]
        .as_product()
        .unwrap();
    begin_rust_struct_def_shared(module, out, &type_name, product_def);

    out.newline();

    let table = TableSchema::from_module_def(table, 0.into())
        .validated()
        .expect("Failed to generate table due to validation errors");
    print_impl_tabletype(module, out, &type_name, product_def, &table);

    output.into_inner()
}

// TODO: figure out if/when product types should derive:
// - Clone
// - Debug
// - Copy
// - PartialEq, Eq
// - Hash
//    - Complicated because `HashMap` is not `Hash`.
// - others?

pub fn rust_type_file_name(type_name: &str) -> String {
    let filename = type_name.replace('.', "").to_case(Case::Snake);
    filename + ".rs"
}

pub fn rust_reducer_file_name(type_name: &str) -> String {
    let filename = type_name.replace('.', "").to_case(Case::Snake);
    filename + "_reducer.rs"
}

const STRUCT_DERIVES: &[&str] = &["#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]"];

fn print_struct_derives(output: &mut Indenter) {
    print_lines(output, STRUCT_DERIVES);
}

fn begin_rust_struct_def_shared(module: &ModuleDef, out: &mut Indenter, name: &str, def: &ProductTypeDef) {
    print_file_header(out);

    // Pass this file into `gen_and_print_imports` to avoid recursively importing self
    // for recursive types.
    //
    // The file_name will be incorrect for reducer arg structs, but that doesn't matter
    // because it's impossible for a reducer arg struct to be recursive.
    let file_name = name.to_case(Case::Snake);
    let this_file = (file_name.as_str(), name);

    gen_and_print_imports(module, out, &def.elements, this_file);

    out.newline();

    print_struct_derives(out);

    write!(out, "pub struct {name} ");

    // TODO: if elements is empty, define a unit struct with no brace-delimited list of fields.
    out.delimited_block(
        "{",
        |out| {
            for (name, ty) in def {
                write!(out, "pub {}: ", name.deref().to_case(Case::Snake));
                write_type(module, out, ty);
                writeln!(out, ",");
            }
        },
        "}",
    );

    out.newline();
}

fn find_primary_key_column_index(table: &TableSchema) -> Option<usize> {
    table.pk().map(|x| x.col_pos.idx())
}

fn print_impl_tabletype(
    module: &ModuleDef,
    out: &mut Indenter,
    type_name: &str,
    product_def: &ProductTypeDef,
    table: &TableSchema,
) {
    write!(out, "impl TableType for {type_name} ");

    out.delimited_block(
        "{",
        |out| {
            writeln!(out, "const TABLE_NAME: &'static str = {:?};", table.table_name);
            writeln!(out, "type ReducerEvent = super::ReducerEvent;");
        },
        "}\n",
    );

    out.newline();

    if let Some(pk_field) = table.pk() {
        let pk_field_name = pk_field.col_name.deref().to_case(Case::Snake);
        // TODO: ensure that primary key types are always `Eq`, `Hash`, `Clone`.
        write!(out, "impl TableWithPrimaryKey for {type_name} ");
        out.delimited_block(
            "{",
            |out| {
                write!(out, "type PrimaryKey = ");
                write_type(module, out, &product_def.elements[pk_field.col_pos.idx()].1);
                writeln!(out, ";");

                out.delimited_block(
                    "fn primary_key(&self) -> &Self::PrimaryKey {",
                    |out| writeln!(out, "&self.{pk_field_name}"),
                    "}\n",
                )
            },
            "}\n",
        );
    }

    out.newline();

    print_table_filter_methods(module, out, type_name, product_def, table);
}

fn print_table_filter_methods(
    module: &ModuleDef,
    out: &mut Indenter,
    table_type_name: &str,
    product_def: &ProductTypeDef,
    table: &TableSchema,
) {
    write!(out, "impl {table_type_name} ");
    let constraints = table.column_constraints();
    out.delimited_block(
        "{",
        |out| {
            for field in table.columns() {
                let field_name = field.col_name.deref().to_case(Case::Snake);
                let col_ty = &product_def.elements[field.col_pos.idx()].1;
                match col_ty {
                    AlgebraicTypeUse::Ref(_)
                    | AlgebraicTypeUse::Array(_)
                    | AlgebraicTypeUse::Map { .. }
                    | AlgebraicTypeUse::Option(_) => continue,
                    _ => {}
                }
                writeln!(out, "{ALLOW_UNUSED}");
                write!(out, "pub fn filter_by_{field_name}({field_name}: ");
                // TODO: the filter methods should take the target value by
                //       reference. String fields should take &str, and array/vector
                //       fields should take &[T]. Determine if integer types should be by
                //       value. Is there a trait for this?
                //       Look at `Borrow` or Deref or AsRef?
                write_type(module, out, col_ty);
                write!(out, ") -> ");
                let ct = constraints[&ColList::new(field.col_pos)];

                write!(out, "TableIter<Self>");
                out.delimited_block(
                    " {",
                    |out| {
                        writeln!(
                            out,
                            // TODO: for primary keys, we should be able to do better than
                            //       `find` or `filter`. We should be able to look up
                            //       directly in the `TableCache`.
                            "Self::filter(|row| row.{field_name} == {field_name})",
                        )
                    },
                    "}\n",
                );
                if ct.has_unique() {
                    writeln!(out, "{ALLOW_UNUSED}");
                    write!(out, "pub fn find_by_{field_name}({field_name}: ");
                    write_type(module, out, col_ty);
                    write!(out, ") -> Option<Self> ");
                    out.delimited_block(
                        "{",
                        |out| writeln!(out, "Self::find(|row| row.{field_name} == {field_name})"),
                        "}\n",
                    );
                }
            }
        },
        "}\n",
    )
}

fn reducer_type_name(reducer: &ReducerDef) -> String {
    let mut name = reducer.name.deref().to_case(Case::Pascal);
    name.push_str("Args");
    name
}

fn reducer_variant_name(reducer: &ReducerDef) -> String {
    reducer.name.deref().to_case(Case::Pascal)
}

fn reducer_module_name(reducer: &ReducerDef) -> String {
    let mut name = reducer.name.deref().to_case(Case::Snake);
    name.push_str("_reducer");
    name
}

fn reducer_function_name(reducer: &ReducerDef) -> String {
    reducer.name.deref().to_case(Case::Snake)
}

fn print_reducer_struct_literal(out: &mut Indenter, reducer: &ReducerDef) {
    write!(out, "{} ", reducer_type_name(reducer));
    // TODO: if reducer.args is empty, write a unit struct.
    out.delimited_block(
        "{",
        |out| {
            for (name, _) in &reducer.params_for_generate {
                writeln!(out, "{},", name.deref().to_case(Case::Snake));
            }
        },
        "}",
    );
}

/// Generate a file which defines a struct corresponding to the `reducer`'s arguments,
/// implements `spacetimedb_sdk::table::Reducer` for it, and defines a helper
/// function which invokes the reducer.
pub fn autogen_rust_reducer(module: &ModuleDef, reducer: &ReducerDef) -> String {
    let func_name = reducer_function_name(reducer);
    let type_name = reducer_type_name(reducer);

    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    begin_rust_struct_def_shared(module, out, &type_name, &reducer.params_for_generate);

    out.newline();

    write!(out, "impl Reducer for {type_name} ");

    out.delimited_block(
        "{",
        |out| writeln!(out, "const REDUCER_NAME: &'static str = {:?};", &reducer.name),
        "}\n",
    );

    out.newline();

    // Function definition for the convenient caller, which takes normal args, constructs
    // an instance of the struct, and calls `invoke` on it.
    writeln!(out, "{ALLOW_UNUSED}");
    write!(out, "pub fn {func_name}");

    // arglist
    // TODO: if reducer.args is empty, just write "()" with no newlines
    out.delimited_block(
        "(",
        |out| {
            for (name, ty) in &reducer.params_for_generate {
                write!(out, "{}: ", name.deref().to_case(Case::Snake));
                write_type(module, out, ty);
                writeln!(out, ",");
            }
        },
        ") ",
    );

    // body
    out.delimited_block(
        "{",
        |out| {
            print_reducer_struct_literal(out, reducer);
            writeln!(out, ".invoke();");
        },
        "}\n",
    );

    out.newline();

    let mut on_func = |on_prefix, mut_, fn_kind| {
        // Function definition for convenient callback function,
        // which takes a closure fromunpacked args,
        // and wraps it in a closure from the args struct.
        writeln!(out, "{ALLOW_UNUSED}");
        write!(
            out,
            "pub fn {on_prefix}_{func_name}({mut_}__callback: impl {fn_kind}(&Identity, Option<Address>, &Status"
        );
        for (_, arg_type) in &reducer.params_for_generate {
            write!(out, ", &");
            write_type(module, out, arg_type);
        }
        writeln!(out, ") + Send + 'static) -> ReducerCallbackId<{type_name}> ");
        out.delimited_block(
            "{",
            |out| {
                write!(out, "{type_name}::{on_prefix}_reducer");
                out.delimited_block(
                    "(move |__identity, __addr, __status, __args| {",
                    |out| {
                        write!(out, "let ");
                        print_reducer_struct_literal(out, reducer);
                        writeln!(out, " = __args;");
                        out.delimited_block(
                            "__callback(",
                            |out| {
                                writeln!(out, "__identity,");
                                writeln!(out, "__addr,");
                                writeln!(out, "__status,");
                                for (arg_name, _) in &reducer.params_for_generate {
                                    writeln!(out, "{},", arg_name.deref().to_case(Case::Snake));
                                }
                            },
                            ");\n",
                        );
                    },
                    "})\n",
                );
            },
            "}\n",
        );

        out.newline();
    };

    on_func("on", "mut ", "FnMut");
    on_func("once_on", "", "FnOnce");

    // Function definition for callback-canceling `remove_on_{reducer}` function.
    writeln!(out, "{ALLOW_UNUSED}");
    write!(out, "pub fn remove_on_{func_name}(id: ReducerCallbackId<{type_name}>) ");
    out.delimited_block(
        "{",
        |out| {
            writeln!(out, "{type_name}::remove_on_reducer(id);");
        },
        "}\n",
    );

    output.into_inner()
}

/// Generate a `mod.rs` as the entry point into the autogenerated code.
///
/// The `mod.rs` contains several things:
///
/// 1. `pub mod` and `pub use` declarations for all the other files generated.
///    Without these, either the other files wouldn't get compiled,
///    or users would have to `mod`-declare each file manually.
///
/// 2. `enum ReducerEvent`, which has variants for each reducer in the module.
///    Row callbacks are passed an optional `ReducerEvent` as an additional argument,
///    so they can know what reducer caused the row to change.
///
/// 3. `struct Module`, which implements `SpacetimeModule`.
///    The methods on `SpacetimeModule` implement passing appropriate type parameters
///    to various SDK internal functions.
///
/// 4. `fn connect`, which invokes
///    `spacetimedb_sdk::background_connection::BackgroundDbConnection::connect`
///    to connect to a remote database, and passes the `handle_row_update`
///    and `handle_event` functions so the `BackgroundDbConnection` can spawn workers
///    which use those functions to dispatch on the content of messages.
pub fn autogen_rust_globals(module: &ModuleDef) -> Vec<(String, String)> {
    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    print_file_header(out);

    // Import some extra stuff, just for the root module.
    print_dispatch_imports(out);

    out.newline();

    // Declare `pub mod` for each of the files generated.
    print_module_decls(module, out);

    out.newline();

    // Re-export all the modules for the generated files.
    print_module_reexports(module, out);

    out.newline();

    // Define `enum ReducerEvent`.
    print_reducer_event_defn(module, out);

    out.newline();

    print_spacetime_module_struct_defn(module, out);

    out.newline();

    // Define `fn connect`.
    print_connect_defn(out);

    vec![("mod.rs".to_string(), output.into_inner())]
}

/// Extra imports required by the `mod.rs` file, in addition to the [`SPACETIMEDB_IMPORTS`].
const DISPATCH_IMPORTS: &[&str] = &[
    "use spacetimedb_sdk::ws_messages::{TableUpdate, TransactionUpdate};",
    "use spacetimedb_sdk::client_cache::{ClientCache, RowCallbackReminders};",
    "use spacetimedb_sdk::identity::Credentials;",
    "use spacetimedb_sdk::callbacks::{DbCallbacks, ReducerCallbacks};",
    "use spacetimedb_sdk::reducer::AnyReducerEvent;",
    "use spacetimedb_sdk::global_connection::with_connection_mut;",
    "use spacetimedb_sdk::spacetime_module::SpacetimeModule;",
    "use std::sync::Arc;",
];

fn print_dispatch_imports(out: &mut Indenter) {
    print_lines(out, DISPATCH_IMPORTS);
}

fn iter_module_names(module: &ModuleDef) -> impl Iterator<Item = String> + '_ {
    dbg!(module.types().map(|ty| (&ty.name, ty.ty)).collect::<Vec<_>>());
    itertools::chain!(
        module
            .types()
            .sorted_by_key(|ty| &ty.name)
            .map(|ty| collect_case(Case::Snake, ty.name.name_segments())),
        iter_reducers(module).map(reducer_module_name),
    )
}

fn iter_tables(module: &ModuleDef) -> impl Iterator<Item = &TableDef> {
    module.tables().sorted_by_key(|tbl| &tbl.name)
}

fn iter_reducers(module: &ModuleDef) -> impl Iterator<Item = &ReducerDef> {
    module
        .reducers()
        .filter(|r| r.lifecycle.is_none())
        .sorted_by_key(|r| &r.name)
}

/// Print `pub mod` declarations for all the files that will be generated for `items`.
fn print_module_decls(module: &ModuleDef, out: &mut Indenter) {
    for module_name in iter_module_names(module) {
        writeln!(out, "pub mod {module_name};");
    }
}

/// Print `pub use *` declarations for all the files that will be generated for `items`.
fn print_module_reexports(module: &ModuleDef, out: &mut Indenter) {
    for module_name in iter_module_names(module) {
        writeln!(out, "pub use {module_name}::*;");
    }
}

/// Define a unit struct which implements `SpacetimeModule`,
/// with methods responsible for supplying type parameters to various functions.
///
/// `SpacetimeModule`'s methods are:
///
/// - `handle_table_update`, which dispatches on table name to find the appropriate type
///    to parse the rows and insert or remove them into/from the
///    `spacetimedb_sdk::client_cache::ClientCache`. The other SDKs avoid needing
///    such a dispatch function by dynamically discovering the set of table types,
///    e.g. using C#'s `AppDomain`. Rust's type system prevents this.
///
/// - `invoke_row_callbacks`, which is invoked after `handle_table_update` and `handle_resubscribe`
///    to distribute a new client cache state and an optional `ReducerEvent`
///    to the `DbCallbacks` worker alongside each row callback for the preceding table change.
///
/// - `handle_resubscribe`, which serves the same role as `handle_table_update`, but for
///    re-subscriptions in a `SubscriptionUpdate` following an outgoing `Subscribe`.
///
/// - `handle_event`, which serves the same role as `handle_table_update`, but for
///    reducers.
fn print_spacetime_module_struct_defn(module: &ModuleDef, out: &mut Indenter) {
    // Muffle unused warning for `Module`, which is not supposed to be visible to
    // users. It will be used if and only if `connect` is used, so that unused warning is
    // sufficient, and not as confusing.
    writeln!(out, "{ALLOW_UNUSED}");
    writeln!(out, "pub struct Module;");
    out.delimited_block(
        "impl SpacetimeModule for Module {",
        |out| {
            print_handle_table_update_defn(module, out);
            print_invoke_row_callbacks_defn(module, out);
            print_handle_event_defn(module, out);
            print_handle_resubscribe_defn(module, out);
        },
        "}\n",
    );
}

/// Define the `handle_table_update` method,
/// which dispatches on the table name in a `TableUpdate` message
/// to call an appropriate method on the `ClientCache`.
#[allow(deprecated)]
fn print_handle_table_update_defn(module: &ModuleDef, out: &mut Indenter) {
    out.delimited_block(
        "fn handle_table_update(&self, table_update: TableUpdate, client_cache: &mut ClientCache, callbacks: &mut RowCallbackReminders) {",
        |out| {
            writeln!(out, "let table_name = &table_update.table_name[..];");
            out.delimited_block(
                "match table_name {",
                |out| {
                    for table_desc in iter_tables(module) {
                        let table = TableSchema::from_module_def(table_desc, 0.into()).validated().unwrap();
                        writeln!(
                            out,
                            "{:?} => client_cache.{}::<{}::{}>(callbacks, table_update),",
                            table.table_name,
                            if find_primary_key_column_index(&table).is_some() {
                                "handle_table_update_with_primary_key"
                            } else {
                                "handle_table_update_no_primary_key"
                            },
                            type_name(module, table_desc.product_type_ref).to_case(Case::Snake),
                            type_name(module, table_desc.product_type_ref).to_case(Case::Pascal),
                        );
                    }
                    writeln!(
                        out,
                        "_ => spacetimedb_sdk::log::error!(\"TableRowOperation on unknown table {{:?}}\", table_name),"
                    );
                },
                "}\n",
            );
        },
        "}\n",
    );
}

/// Define the `invoke_row_callbacks` function,
/// which does `RowCallbackReminders::invoke_callbacks` on each table type defined in the `items`.
fn print_invoke_row_callbacks_defn(module: &ModuleDef, out: &mut Indenter) {
    out.delimited_block(
        "fn invoke_row_callbacks(&self, reminders: &mut RowCallbackReminders, worker: &mut DbCallbacks, reducer_event: Option<Arc<AnyReducerEvent>>, state: &Arc<ClientCache>) {",
        |out| {
            for table in iter_tables(module) {
                writeln!(
                    out,
                    "reminders.invoke_callbacks::<{}::{}>(worker, &reducer_event, state);",
                    type_name(module, table.product_type_ref).to_case(Case::Snake),
                    type_name(module, table.product_type_ref).to_case(Case::Pascal),
                );
            }
        },
        "}\n",
    );
}

/// Define the `handle_resubscribe` function,
/// which dispatches on the table name in a `TableUpdate`
/// to invoke `ClientCache::handle_resubscribe_for_type` with an appropriate type arg.
fn print_handle_resubscribe_defn(module: &ModuleDef, out: &mut Indenter) {
    out.delimited_block(
        "fn handle_resubscribe(&self, new_subs: TableUpdate, client_cache: &mut ClientCache, callbacks: &mut RowCallbackReminders) {",
        |out| {
            writeln!(out, "let table_name = &new_subs.table_name[..];");
            out.delimited_block(
                "match table_name {",
                |out| {
                    for table in iter_tables(module) {
                        writeln!(
                            out,
                            "{:?} => client_cache.handle_resubscribe_for_type::<{}::{}>(callbacks, new_subs),",
                            table.name,
                            type_name(module, table.product_type_ref).to_case(Case::Snake),
                            type_name(module, table.product_type_ref).to_case(Case::Pascal),
                        );
                    }
                    writeln!(
                        out,
                        "_ => spacetimedb_sdk::log::error!(\"TableRowOperation on unknown table {{:?}}\", table_name),"
                    );
                },
                "}\n",
            );
        },
        "}\n"
    );
}

/// Define the `handle_event` function,
/// which dispatches on the reducer name in an `Event`
/// to `ReducerCallbacks::handle_event_of_type` with an appropriate type argument.
fn print_handle_event_defn(module: &ModuleDef, out: &mut Indenter) {
    out.delimited_block(
        "fn handle_event(&self, event: TransactionUpdate, _reducer_callbacks: &mut ReducerCallbacks, _state: Arc<ClientCache>) -> Option<Arc<AnyReducerEvent>> {",
        |out| {
            writeln!(out, "let reducer_call = &event.reducer_call;");

            // If the module defines no reducers,
            // we'll generate a single match arm, the fallthrough.
            // Clippy doesn't like this, as it could be a `let` binding,
            // but we're not going to add logic to handle that case,
            // so just quiet the lint.
            writeln!(out, "#[allow(clippy::match_single_binding)]");

            out.delimited_block(
                "match &reducer_call.reducer_name[..] {",
                |out| {
                    for reducer in iter_reducers(module) {
                        writeln!(
                            out,
                            "{:?} => _reducer_callbacks.handle_event_of_type::<{}::{}, ReducerEvent>(event, _state, ReducerEvent::{}),",
                            reducer.name,
                            reducer_module_name(reducer),
                            reducer_type_name(reducer),
                            reducer_variant_name(reducer),
                        );
                    }
                    writeln!(
                        out,
                        "unknown => {{ spacetimedb_sdk::log::error!(\"Event on an unknown reducer: {{:?}}\", unknown); None }}"
                    );
                },
                "}\n",
            );
        },
        "}\n",
    );
}

const CONNECT_DOCSTRING: &[&str] = &[
    "/// Connect to a database named `db_name` accessible over the internet at the URI `spacetimedb_uri`.",
    "///",
    "/// If `credentials` are supplied, they will be passed to the new connection to",
    "/// identify and authenticate the user. Otherwise, a set of `Credentials` will be",
    "/// generated by the server.",
];

fn print_connect_docstring(out: &mut Indenter) {
    print_lines(out, CONNECT_DOCSTRING);
}

/// Define the `connect` wrapper,
/// which passes all the autogenerated dispatch functions to `BackgroundDbConnection::connect`.
fn print_connect_defn(out: &mut Indenter) {
    print_connect_docstring(out);
    out.delimited_block(
        "pub fn connect<IntoUri>(spacetimedb_uri: IntoUri, db_name: &str, credentials: Option<Credentials>) -> Result<()>
where
\tIntoUri: TryInto<spacetimedb_sdk::http::Uri>,
\t<IntoUri as TryInto<spacetimedb_sdk::http::Uri>>::Error: std::error::Error + Send + Sync + 'static,
{",
        |out| out.delimited_block(
            "with_connection_mut(|connection| {",
            |out| {
                writeln!(
                    out,
                    "connection.connect(spacetimedb_uri, db_name, credentials, Arc::new(Module))?;"
                );
                writeln!(out, "Ok(())");
            },
            "})\n",
        ),
        "}\n",
    );
}

fn print_reducer_event_defn(module: &ModuleDef, out: &mut Indenter) {
    writeln!(out, "{ALLOW_UNUSED}");

    print_enum_derives(out);
    out.delimited_block(
        "pub enum ReducerEvent {",
        |out| {
            for reducer in iter_reducers(module) {
                writeln!(
                    out,
                    "{}({}::{}),",
                    reducer_variant_name(reducer),
                    reducer_module_name(reducer),
                    reducer_type_name(reducer),
                );
            }
        },
        "}\n",
    );
}

fn module_name(name: &str) -> String {
    name.to_case(Case::Snake)
}

/// Print `use super::` imports for each of the `imports`, except `this_file`.
///
/// `this_file` is passed and excluded for the case of recursive types:
/// without it, the definition for a type like `struct Foo { foos: Vec<Foo> }`
/// would attempt to include `import super::foo::Foo`, which fails to compile.
fn print_imports(out: &mut Indenter, imports: Imports, this_file: (&str, &str)) {
    for (module_name, type_name) in imports {
        if (module_name.as_str(), type_name.as_str()) != this_file {
            writeln!(out, "use super::{module_name}::{type_name};");
        }
    }
}

/// Use `search_function` on `roots` to detect required imports, then print them with `print_imports`.
///
/// `this_file` is passed and excluded for the case of recursive types:
/// without it, the definition for a type like `struct Foo { foos: Vec<Foo> }`
/// would attempt to include `import super::foo::Foo`, which fails to compile.
fn gen_and_print_imports(
    module: &ModuleDef,
    out: &mut Indenter,
    roots: &[(Identifier, AlgebraicTypeUse)],
    this_file: (&str, &str),
) {
    let mut imports = BTreeSet::new();
    for (_, ty) in roots {
        ty.for_each_ref(|r| {
            let type_name = type_name(module, r);
            let module_name = module_name(&type_name);
            imports.insert((module_name, type_name));
        });
    }

    print_imports(out, imports, this_file);
}
