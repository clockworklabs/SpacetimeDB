use super::code_indenter::CodeIndenter;
use super::{GenCtx, GenItem};
use convert_case::{Case, Casing};
use spacetimedb_lib::sats::{
    AlgebraicType, AlgebraicTypeRef, ArrayType, BuiltinType, MapType, ProductType, ProductTypeElement, SumType,
    SumTypeVariant,
};
use spacetimedb_lib::{ColumnIndexAttribute, ReducerDef, TableDef};
use std::collections::HashSet;
use std::fmt::Write;

type Indenter = CodeIndenter<String>;

/// Pairs of (module_name, TypeName).
type Imports = HashSet<(String, String)>;

enum MaybePrimitive<'a> {
    Primitive(&'a str),
    Array(&'a ArrayType),
    Map(&'a MapType),
}

fn maybe_primitive(b: &BuiltinType) -> MaybePrimitive {
    MaybePrimitive::Primitive(match b {
        BuiltinType::Bool => "bool",
        BuiltinType::I8 => "i8",
        BuiltinType::U8 => "u8",
        BuiltinType::I16 => "i16",
        BuiltinType::U16 => "u16",
        BuiltinType::I32 => "i32",
        BuiltinType::U32 => "u32",
        BuiltinType::I64 => "i64",
        BuiltinType::U64 => "u64",
        BuiltinType::I128 => "i128",
        BuiltinType::U128 => "u128",
        BuiltinType::String => "String",
        BuiltinType::F32 => "f32",
        BuiltinType::F64 => "f64",
        BuiltinType::Array(ty) => return MaybePrimitive::Array(ty),
        BuiltinType::Map(m) => return MaybePrimitive::Map(m),
    })
}

fn is_empty_product(ty: &AlgebraicType) -> bool {
    if let AlgebraicType::Product(none_type) = ty {
        none_type.elements.is_empty()
    } else {
        false
    }
}

// This function is duplicated in [typescript.rs] and [csharp.rs], and should maybe be
// lifted into a module, or be a part of SATS itself.
fn is_option_type(ty: &SumType) -> bool {
    let name_is = |variant: &SumTypeVariant, name| variant.name.as_ref().expect("Variants should have names!") == name;
    matches!(
        &ty.variants[..],
        [a, b] if name_is(a, "some")
            && name_is(b, "none")
            && is_empty_product(&b.algebraic_type)
    )
}

fn write_type_ctx(ctx: &GenCtx, out: &mut Indenter, ty: &AlgebraicType) {
    write_type(&|r| type_name(ctx, r), out, ty)
}

pub fn write_type<W: Write>(ctx: &impl Fn(AlgebraicTypeRef) -> String, out: &mut W, ty: &AlgebraicType) {
    match ty {
        AlgebraicType::Sum(sum_type) => {
            if is_option_type(sum_type) {
                write!(out, "Option::<").unwrap();
                write_type(ctx, out, &sum_type.variants[0].algebraic_type);
                write!(out, ">").unwrap();
            } else {
                write!(out, "enum ").unwrap();
                print_comma_sep_braced(out, &sum_type.variants, |out: &mut W, elem: &_| {
                    if let Some(name) = &elem.name {
                        write!(out, "{}: ", name).unwrap();
                    }
                    write_type(ctx, out, &elem.algebraic_type);
                });
            }
        }
        AlgebraicType::Product(ProductType { elements }) => {
            print_comma_sep_braced(out, elements, |out: &mut W, elem: &ProductTypeElement| {
                if let Some(name) = &elem.name {
                    write!(out, "{}: ", name).unwrap();
                }
                write_type(ctx, out, &elem.algebraic_type);
            });
        }
        AlgebraicType::Builtin(b) => match maybe_primitive(b) {
            MaybePrimitive::Primitive(p) => write!(out, "{}", p).unwrap(),
            MaybePrimitive::Array(ArrayType { elem_ty }) => {
                write!(out, "Vec::<").unwrap();
                write_type(ctx, out, elem_ty);
                write!(out, ">").unwrap();
            }
            MaybePrimitive::Map(ty) => {
                // TODO: Should `BuiltinType::Map` translate to `HashMap`? This requires
                //       that any map-key type implement `Hash`. We'll have to derive hash
                //       on generated types, and notably, `HashMap` is not itself `Hash`,
                //       so any type that holds a `Map` cannot derive `Hash` and cannot
                //       key a `Map`.
                write!(out, "HashMap::<").unwrap();
                write_type(ctx, out, &ty.key_ty);
                write!(out, ", ").unwrap();
                write_type(ctx, out, &ty.ty);
                write!(out, ">").unwrap();
            }
        },
        AlgebraicType::Ref(r) => {
            write!(out, "{}", ctx(*r)).unwrap();
        }
    }
}

fn print_comma_sep_braced<W: Write, T>(out: &mut W, elems: &[T], on: impl Fn(&mut W, &T)) {
    write!(out, "{{").unwrap();

    let mut iter = elems.iter();

    // First factor.
    if let Some(elem) = iter.next() {
        write!(out, " ").unwrap();
        on(out, elem);
    }
    // Other factors.
    for elem in iter {
        write!(out, ", ").unwrap();
        on(out, elem);
    }

    if !elems.is_empty() {
        write!(out, " ").unwrap();
    }

    write!(out, "}}").unwrap();
}

// This is (effectively) duplicated in [typescript.rs] as `typescript_typename` and in
// [csharp.rs] as `csharp_typename`, and should probably be lifted to a shared utils
// module.
fn type_name(ctx: &GenCtx, typeref: AlgebraicTypeRef) -> String {
    ctx.names[typeref.idx()]
        .as_deref()
        .expect("TypeRefs should have names")
        .to_case(Case::Pascal)
}

fn print_lines(output: &mut Indenter, lines: &[&str]) {
    for line in lines {
        writeln!(output, "{}", line).unwrap();
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

const SPACETIMEDB_IMPORTS: &[&str] = &[
    ALLOW_UNUSED,
    "use spacetimedb_sdk::{",
    "\tsats::{ser::Serialize, de::Deserialize},",
    "\ttable::{TableType, TableIter, TableWithPrimaryKey},",
    "\treducer::{Reducer},",
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
pub fn autogen_rust_sum(ctx: &GenCtx, name: &str, sum_type: &SumType) -> String {
    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    let sum_type_name = name.replace("r#", "").to_case(Case::Pascal);

    print_file_header(out);

    // For some reason, deref coercion doesn't work on `&sum_type.variants` here - rustc
    // wants to pass it as `&Vec<_>`, not `&[_]`. The slicing index `[..]` forces passing
    // as a slice.
    gen_and_print_imports(ctx, out, &sum_type.variants[..], generate_imports_variants);

    out.newline();

    print_enum_derives(out);

    write!(out, "pub enum {} ", sum_type_name).unwrap();

    out.delimited_block(
        "{",
        |out| {
            for variant in &sum_type.variants {
                write_enum_variant(ctx, out, variant);
                out.newline();
            }
        },
        "}\n",
    );

    output.into_inner()
}

fn write_enum_variant(ctx: &GenCtx, out: &mut Indenter, variant: &SumTypeVariant) {
    let Some(name) = &variant.name else {
        panic!("Sum type variant has no name: {:?}", variant);
    };
    let name = name.to_case(Case::Pascal);
    write!(out, "{}", name).unwrap();
    match &variant.algebraic_type {
        AlgebraicType::Product(ProductType { elements }) if elements.is_empty() => {
            // If the contained type is the unit type, i.e. this variant has no members,
            // write it without parens or braces, like
            // ```
            // Foo,
            // ```
            writeln!(out, ",").unwrap();
        }
        AlgebraicType::Product(ProductType { elements }) => {
            // If the contained type is a non-empty product, i.e. this variant is
            // struct-like, write it with braces and named fields.
            write_struct_type_fields_in_braces(
                ctx, out, elements,
                // Do not `pub`-qualify fields because enum fields are always public, and
                // rustc errors on the redundant `pub`.
                false,
            );
        }
        otherwise => {
            // If the contained type is not a product, i.e. this variant has a single
            // member, write it tuple-style, with parens.
            write!(out, "(").unwrap();
            write_type_ctx(ctx, out, otherwise);
            write!(out, "),").unwrap();
        }
    }
}

fn write_struct_type_fields_in_braces(
    ctx: &GenCtx,
    out: &mut Indenter,
    elements: &[ProductTypeElement],

    // Whether to print a `pub` qualifier on the fields. Necessary for `struct` defns,
    // disallowed for `enum` defns.
    pub_qualifier: bool,
) {
    out.delimited_block(
        "{",
        |out| write_arglist_no_delimiters_ctx(ctx, out, elements, pub_qualifier.then_some("pub")),
        "}",
    );
}

fn write_arglist_no_delimiters_ctx(
    ctx: &GenCtx,
    out: &mut Indenter,
    elements: &[ProductTypeElement],

    // Written before each line. Useful for `pub`.
    prefix: Option<&str>,
) {
    write_arglist_no_delimiters(&|r| type_name(ctx, r), out, elements, prefix)
}

pub fn write_arglist_no_delimiters(
    ctx: &impl Fn(AlgebraicTypeRef) -> String,
    out: &mut impl Write,
    elements: &[ProductTypeElement],

    // Written before each line. Useful for `pub`.
    prefix: Option<&str>,
) {
    for elt in elements {
        if let Some(prefix) = prefix {
            write!(out, "{} ", prefix).unwrap();
        }

        let Some(name) = &elt.name else {
            panic!("Product type element has no name: {:?}", elt);
        };
        let name = name.to_case(Case::Snake);

        write!(out, "{}: ", name).unwrap();
        write_type(ctx, out, &elt.algebraic_type);
        writeln!(out, ",").unwrap();
    }
}

/// Generate a file which defines a `struct` corresponding to the `product` type.
pub fn autogen_rust_tuple(ctx: &GenCtx, name: &str, product: &ProductType) -> String {
    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    let type_name = name.to_case(Case::Pascal);

    begin_rust_struct_def_shared(ctx, out, &type_name, &product.elements);

    output.into_inner()
}

fn find_product_type(ctx: &GenCtx, ty: AlgebraicTypeRef) -> &ProductType {
    ctx.typespace[ty].as_product().unwrap()
}

/// Generate a file which defines a `struct` corresponding to the `table`'s `ProductType`,
/// and implements `spacetimedb_sdk::table::TableType` for it.
pub fn autogen_rust_table(ctx: &GenCtx, table: &TableDef) -> String {
    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    let type_name = table.name.to_case(Case::Pascal);

    begin_rust_struct_def_shared(ctx, out, &type_name, &find_product_type(ctx, table.data).elements);

    out.newline();

    print_impl_tabletype(ctx, out, table);

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

const STRUCT_DERIVES: &[&str] = &["#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]"];

fn print_struct_derives(output: &mut Indenter) {
    print_lines(output, STRUCT_DERIVES);
}

fn begin_rust_struct_def_shared(ctx: &GenCtx, out: &mut Indenter, name: &str, elements: &[ProductTypeElement]) {
    print_auto_generated_file_comment(out);

    print_spacetimedb_imports(out);

    gen_and_print_imports(ctx, out, elements, generate_imports_elements);

    out.newline();

    print_struct_derives(out);

    write!(out, "pub struct {} ", name).unwrap();

    // TODO: if elements is empty, define a unit struct with no brace-delimited list of fields.
    write_struct_type_fields_in_braces(
        ctx, out, elements, // `pub`-qualify fields.
        true,
    );

    out.newline();
}

fn find_primary_key_column_index(ctx: &GenCtx, table: &TableDef) -> Option<usize> {
    // Search the whole slice, rather than stopping early like `Iter::position`, so we can
    // report an error if multiple columns are primary.
    let primaries = table
        .column_attrs
        .iter()
        .enumerate()
        .filter_map(|(i, attr)| attr.is_primary().then_some(i))
        .collect::<Vec<_>>();
    match primaries.len() {
        2.. => {
            let names = primaries
                .iter()
                .map(|&i| &find_product_type(ctx, table.data).elements[i])
                .collect::<Vec<_>>();
            panic!(
                "Multiple primary columns defined for table {:?}: {:?}",
                table.name, names
            );
        }
        1 => Some(primaries[0]),
        0 => None,

        // rustc refuses to do exhaustiveness checking on `usize`, even in this case where
        // one pattern is a range with no upper bound, so the entire range of `usize` is
        // covered on any platform.
        _ => unreachable!(),
    }
}

fn print_impl_tabletype(ctx: &GenCtx, out: &mut Indenter, table: &TableDef) {
    let type_name = table.name.to_case(Case::Pascal);

    write!(out, "impl TableType for {} ", type_name).unwrap();

    out.delimited_block(
        "{",
        |out| {
            writeln!(out, "const TABLE_NAME: &'static str = {:?};", table.name).unwrap();
            writeln!(out, "type ReducerEvent = super::ReducerEvent;").unwrap();
        },
        "}\n",
    );

    out.newline();

    if let Some(primary_column_index) = find_primary_key_column_index(ctx, table) {
        let pk_field = &find_product_type(ctx, table.data).elements[primary_column_index];
        let pk_field_name = pk_field
            .name
            .as_ref()
            .expect("Fields designated as primary key should have names!")
            .to_case(Case::Snake);
        // TODO: ensure that primary key types are always `Eq`, `Hash`, `Clone`.
        write!(out, "impl TableWithPrimaryKey for {} ", type_name).unwrap();
        out.delimited_block(
            "{",
            |out| {
                write!(out, "type PrimaryKey = ").unwrap();
                write_type_ctx(ctx, out, &pk_field.algebraic_type);
                writeln!(out, ";").unwrap();

                out.delimited_block(
                    "fn primary_key(&self) -> &Self::PrimaryKey {",
                    |out| writeln!(out, "&self.{}", pk_field_name).unwrap(),
                    "}\n",
                )
            },
            "}\n",
        );
    }

    out.newline();

    print_table_filter_methods(
        ctx,
        out,
        &type_name,
        &find_product_type(ctx, table.data).elements,
        &table.column_attrs,
    );
}

fn print_table_filter_methods(
    ctx: &GenCtx,
    out: &mut Indenter,
    table_type_name: &str,
    elements: &[ProductTypeElement],
    attrs: &[ColumnIndexAttribute],
) {
    write!(out, "impl {} ", table_type_name).unwrap();
    out.delimited_block(
        "{",
        |out| {
            for (elt, attr) in elements.iter().zip(attrs) {
                let field_name = elt
                    .name
                    .as_ref()
                    .expect("Table columns should have names!")
                    .to_case(Case::Snake);
                // TODO: ensure that fields are PartialEq
                writeln!(out, "{}", ALLOW_UNUSED).unwrap();
                write!(out, "pub fn filter_by_{}({}: ", field_name, field_name).unwrap();
                // TODO: the filter methods should take the target value by
                //       reference. String fields should take &str, and array/vector
                //       fields should take &[T]. Determine if integer typeso should be by
                //       value. Is there a trait for this?
                //       Look at `Borrow` or Deref or AsRef?
                write_type_ctx(ctx, out, &elt.algebraic_type);
                write!(out, ") -> ").unwrap();
                if attr.is_unique() {
                    write!(out, "Option<Self>").unwrap();
                } else {
                    write!(out, "TableIter<Self>").unwrap();
                }
                out.delimited_block(
                    " {",
                    |out| {
                        writeln!(
                            out,
                            "Self::{}(|row| row.{} == {})",
                            // TODO: for primary keys, we should be able to do better than
                            //       `find` or `filter`. We should be able to look up
                            //       directly in the `TableCache`.
                            if attr.is_unique() { "find" } else { "filter" },
                            field_name,
                            field_name,
                        )
                        .unwrap()
                    },
                    "}\n",
                );
            }
        },
        "}\n",
    )
}

fn reducer_type_name(reducer: &ReducerDef) -> String {
    let mut name = reducer.name.to_case(Case::Pascal);
    name.push_str("Args");
    name
}

fn reducer_variant_name(reducer: &ReducerDef) -> String {
    reducer.name.to_case(Case::Pascal)
}

fn reducer_module_name(reducer: &ReducerDef) -> String {
    let mut name = reducer.name.to_case(Case::Snake);
    name.push_str("_reducer");
    name
}

fn reducer_function_name(reducer: &ReducerDef) -> String {
    reducer.name.to_case(Case::Snake)
}

/// Generate a file which defines a struct corresponding to the `reducer`'s arguments,
/// implements `spacetimedb_sdk::table::Reducer` for it, and defines a helper
/// function which invokes the reducer.
pub fn autogen_rust_reducer(ctx: &GenCtx, reducer: &ReducerDef) -> String {
    let func_name = reducer_function_name(reducer);
    let type_name = reducer_type_name(reducer);

    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    begin_rust_struct_def_shared(ctx, out, &type_name, &reducer.args);

    out.newline();

    write!(out, "impl Reducer for {} ", type_name).unwrap();

    out.delimited_block(
        "{",
        |out| writeln!(out, "const REDUCER_NAME: &'static str = {:?};", &reducer.name).unwrap(),
        "}\n",
    );

    out.newline();

    // Function definition for the convenient caller, which takes normal args, constructs
    // an instance of the struct, and calls `invoke` on it.
    write!(out, "{}", ALLOW_UNUSED).unwrap();
    write!(out, "pub fn {}", func_name).unwrap();

    // arglist
    // TODO: if reducer.args is empty, just write "()" with no newlines
    out.delimited_block(
        "(",
        |out| write_arglist_no_delimiters_ctx(ctx, out, &reducer.args, None),
        ") ",
    );

    // body
    out.delimited_block(
        "{",
        |out| {
            // This is a struct literal.
            write!(out, "{} ", type_name).unwrap();
            // TODO: if reducer.args is empty, write a unit struct.
            out.delimited_block(
                "{",
                |out| {
                    for arg in &reducer.args {
                        let Some(name) = &arg.name else {
                        panic!("Reducer {} arg has no name: {:?}", reducer.name, arg);
                    };
                        let name = name.to_case(Case::Snake);
                        writeln!(out, "{},", name).unwrap();
                    }
                },
                "}.invoke();\n",
            );
        },
        "}\n",
    );

    // TODO: generate `pub fn on_{REDUCER_NAME}` function which calls
    //       `Reducer::on_reducer` to register a callback. Like the relationship between
    //       `pub fn {REDUCER_NAME}` and `Reducer::invoke`, the callback passed to
    //       `on_{REDUCER_NAME}` should take an arglist, not an instance of the reducer
    //       struct. The fn should wrap the passed callback in a closure of the
    //       appropriate type for `Reducer::on_reducer` and unpacks the instance.

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
/// 3. `fn handle_table_update`, which dispatches on table name to find the appropriate type
///    to parse the rows and insert or remove them into/from the
///    `spacetimedb_sdk::client_cache::ClientCache`. The other SDKs avoid needing
///    such a dispatch function by dynamically discovering the set of table types,
///    e.g. using C#'s `AppDomain`. Rust's type system prevents this.
///
/// 4. `fn invoke_row_callbacks`, which is invoked after `handle_table_update` and `handle_resubscribe`
///    to distribute a new client cache state and an optional `ReducerEvent`
///    to the `DbCallbacks` worker alongside each row callback for the preceding table change.
///
/// 5. `fn handle_resubscribe`, which serves the same role as `handle_table_update`, but for
///    re-subscriptions in a `SubscriptionUpdate` following an outgoing `Subscribe`.
///
/// 6. `fn handle_event`, which serves the same role as `handle_table_update`, but for
///    reducers.
///
/// 7. `fn connect`, which invokes
///    `spacetimedb_sdk::background_connection::BackgroundDbConnection::connect`
///    to connect to a remote database, and passes the `handle_row_update`
///    and `handle_event` functions so the `BackgroundDbConnection` can spawn workers
///    which use those functions to dispatch on the content of messages.
pub fn autogen_rust_globals(ctx: &GenCtx, items: &[GenItem]) -> Vec<Vec<(String, String)>> {
    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    // Warn people not to edit the file by hand.
    print_auto_generated_file_comment(out);

    // Import everything all the other files import.
    print_spacetimedb_imports(out);

    // Import some extra stuff, just for the root module.
    print_dispatch_imports(out);

    out.newline();

    // Declare `pub mod` for each of the files generated.
    print_module_decls(out, items);

    out.newline();

    // Re-export all the modules for the generated files.
    print_module_reexports(out, items);

    out.newline();

    // Define `enum ReducerEvent`.
    print_reducer_event_defn(out, items);

    out.newline();

    // Define `fn handle_table_update`.
    print_handle_table_update_defn(ctx, out, items);

    out.newline();

    // Define `fn invoke_row_callbacks`.
    print_invoke_row_callbacks_defn(out, items);

    out.newline();

    // Define `fn handle_resubscribe`.
    print_handle_resubscribe_defn(out, items);

    out.newline();

    // Define `fn handle_event`.
    print_handle_event_defn(out, items);

    out.newline();

    // Define `fn connect`.
    print_connect_defn(out);

    vec![vec![("mod.rs".to_string(), output.into_inner())]]
}

/// Extra imports required by the `mod.rs` file, in addition to the [`SPACETIMEDB_IMPORTS`].
const DISPATCH_IMPORTS: &[&str] = &[
    "use spacetimedb_sdk::client_api_messages::{TableUpdate, Event};",
    "use spacetimedb_sdk::client_cache::{ClientCache, RowCallbackReminders};",
    "use spacetimedb_sdk::identity::Credentials;",
    "use spacetimedb_sdk::callbacks::{DbCallbacks, ReducerCallbacks};",
    "use spacetimedb_sdk::reducer::AnyReducerEvent;",
    "use spacetimedb_sdk::global_connection::with_connection_mut;",
    "use std::sync::Arc;",
];

fn print_dispatch_imports(out: &mut Indenter) {
    print_lines(out, DISPATCH_IMPORTS);
}

fn is_init(reducer: &ReducerDef) -> bool {
    reducer.name == "__init__"
}

fn iter_reducer_items(items: &[GenItem]) -> impl Iterator<Item = &ReducerDef> {
    items.iter().filter_map(|item| match item {
        GenItem::Reducer(reducer) if !is_init(reducer) => Some(reducer),
        _ => None,
    })
}

fn iter_table_items(items: &[GenItem]) -> impl Iterator<Item = &TableDef> {
    items.iter().filter_map(|item| match item {
        GenItem::Table(table) => Some(table),
        _ => None,
    })
}

fn iter_module_names(items: &[GenItem]) -> impl Iterator<Item = String> + '_ {
    items.iter().filter_map(|item| match item {
        GenItem::Table(table) => Some(table.name.to_case(Case::Snake)),
        GenItem::TypeAlias(ty) => Some(ty.name.to_case(Case::Snake)),
        GenItem::Reducer(reducer) => (!is_init(reducer)).then_some(reducer_module_name(reducer)),
    })
}

/// Print `pub mod` declarations for all the files that will be generated for `items`.
fn print_module_decls(out: &mut Indenter, items: &[GenItem]) {
    for module_name in iter_module_names(items) {
        writeln!(out, "pub mod {};", module_name).unwrap();
    }
}

/// Print `pub use *` declarations for all the files that will be generated for `items`.
fn print_module_reexports(out: &mut Indenter, items: &[GenItem]) {
    for module_name in iter_module_names(items) {
        writeln!(out, "pub use {}::*;", module_name).unwrap();
    }
}

/// Define the `handle_table_update` function,
/// which dispatches on the table name in a `TableUpdate` message
/// to call an appropriate method on the `ClientCache`.
fn print_handle_table_update_defn(ctx: &GenCtx, out: &mut Indenter, items: &[GenItem]) {
    // Muffle unused warning for handle_table_update, which is not supposed to be visible to
    // users. It will be used if and only if `connect` is used, so that unused warning is
    // sufficient, and not as confusing.
    writeln!(out, "{}", ALLOW_UNUSED).unwrap();
    out.delimited_block(
        "fn handle_table_update(table_update: TableUpdate, client_cache: &mut ClientCache, callbacks: &mut RowCallbackReminders) {",
        |out| {
            writeln!(out, "let table_name = &table_update.table_name[..];").unwrap();
            out.delimited_block(
                "match table_name {",
                |out| {
                    for table in iter_table_items(items) {
                        writeln!(
                            out,
                            "{:?} => client_cache.{}::<{}::{}>(callbacks, table_update),",
                            table.name,
                            if find_primary_key_column_index(ctx, table).is_some() {
                                "handle_table_update_with_primary_key"
                            } else {
                                "handle_table_update_no_primary_key"
                            },
                            table.name.to_case(Case::Snake),
                            table.name.to_case(Case::Pascal),
                        ).unwrap();
                    }
                    writeln!(
                        out,
                        "_ => spacetimedb_sdk::log::error!(\"TableRowOperation on unknown table {{:?}}\", table_name),",
                    ).unwrap();
                },
                "}\n",
            );
        },
        "}\n",
    );
}

/// Define the `invoke_row_callbacks` function,
/// which does `RowCallbackReminders::invoke_callbacks` on each table type defined in the `items`.
fn print_invoke_row_callbacks_defn(out: &mut Indenter, items: &[GenItem]) {
    // Like `handle_table_update`, muffle unused warning for `invoke_row_callbacks`.
    writeln!(out, "{}", ALLOW_UNUSED).unwrap();
    out.delimited_block(
        "fn invoke_row_callbacks(reminders: &mut RowCallbackReminders, worker: &mut DbCallbacks, reducer_event: Option<Arc<AnyReducerEvent>>, state: &Arc<ClientCache>) {",
        |out| {
            for table in iter_table_items(items) {
                writeln!(
                    out,
                    "reminders.invoke_callbacks::<{}::{}>(worker, &reducer_event, state);",
                    table.name.to_case(Case::Snake),
                    table.name.to_case(Case::Pascal),
                ).unwrap();
            }
        },
        "}\n",
    );
}

/// Define the `handle_resubscribe` function,
/// which dispatches on the table name in a `TableUpdate`
/// to invoke `ClientCache::handle_resubscribe_for_type` with an appropriate type arg.
fn print_handle_resubscribe_defn(out: &mut Indenter, items: &[GenItem]) {
    // Like `handle_table_update`, muffle unused warning for `handle_resubscribe`.
    writeln!(out, "{}", ALLOW_UNUSED).unwrap();
    out.delimited_block(
        "fn handle_resubscribe(new_subs: TableUpdate, client_cache: &mut ClientCache, callbacks: &mut RowCallbackReminders) {",
        |out| {
            writeln!(out, "let table_name = &new_subs.table_name[..];").unwrap();
            out.delimited_block(
                "match table_name {",
                |out| {
                    for table in iter_table_items(items) {
                        writeln!(
                            out,
                            "{:?} => client_cache.handle_resubscribe_for_type::<{}::{}>(callbacks, new_subs),",
                            table.name,
                            table.name.to_case(Case::Snake),
                            table.name.to_case(Case::Pascal),
                        ).unwrap();
                    }
                    writeln!(
                        out,
                        "_ => spacetimedb_sdk::log::error!(\"TableRowOperation on unknown table {{:?}}\", table_name)," ,
                    ).unwrap();
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
fn print_handle_event_defn(out: &mut Indenter, items: &[GenItem]) {
    // Like `handle_table_update`, muffle unused warning for `handle_event`.
    writeln!(out, "{}", ALLOW_UNUSED).unwrap();
    out.delimited_block(
        "fn handle_event(event: Event, reducer_callbacks: &mut ReducerCallbacks, state: Arc<ClientCache>) -> Option<Arc<AnyReducerEvent>> {",
        |out| {
            out.delimited_block(
                "let Some(function_call) = &event.function_call else {",
                |out| writeln!(out, "spacetimedb_sdk::log::warn!(\"Received Event with None function_call\"); return None;")
                    .unwrap(),
                "};\n",
            );
            out.delimited_block(
                "match &function_call.reducer[..] {",
                |out| {
                    for reducer in iter_reducer_items(items) {
                        writeln!(
                            out,
                            "{:?} => reducer_callbacks.handle_event_of_type::<{}::{}, ReducerEvent>(event, state, ReducerEvent::{}),",
                            reducer.name,
                            reducer_module_name(reducer),
                            reducer_type_name(reducer),
                            reducer_variant_name(reducer),
                        ).unwrap();
                    }
                    writeln!(
                        out,
                        "unknown => {{ spacetimedb_sdk::log::error!(\"Event on an unknown reducer: {{:?}}\", unknown); None }}",
                    ).unwrap();
                },
                "}\n",
            );
        },
        "}\n",
    );
}

const CONNECT_DOCSTRING: &[&str] = &[
    "/// Connect to a database named `db_name` accessible over the internet at the URI `host`.",
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
        "pub fn connect<Host>(host: Host, db_name: &str, credentials: Option<Credentials>) -> Result<()>
where
\tHost: TryInto<spacetimedb_sdk::http::Uri>,
\t<Host as TryInto<spacetimedb_sdk::http::Uri>>::Error: std::error::Error + Send + Sync + 'static,
{",
        |out| out.delimited_block(
            "with_connection_mut(|connection| {",
            |out| {
                writeln!(
                    out,
                    "connection.connect(host, db_name, credentials, handle_table_update, handle_resubscribe, invoke_row_callbacks, handle_event)?;"
                ).unwrap();
                writeln!(out, "Ok(())").unwrap();
            },
            "})\n",
        ),
        "}\n",
    );
}

fn print_reducer_event_defn(out: &mut Indenter, items: &[GenItem]) {
    writeln!(out, "{}", ALLOW_UNUSED).unwrap();

    print_enum_derives(out);
    out.delimited_block(
        "pub enum ReducerEvent {",
        |out| {
            for item in items {
                if let GenItem::Reducer(reducer) = item {
                    if !is_init(reducer) {
                        writeln!(
                            out,
                            "{}({}::{}),",
                            reducer_variant_name(reducer),
                            reducer_module_name(reducer),
                            reducer_type_name(reducer),
                        )
                        .unwrap();
                    }
                }
            }
        },
        "}\n",
    );
}

fn generate_imports_variants(ctx: &GenCtx, imports: &mut Imports, variants: &[SumTypeVariant]) {
    for variant in variants {
        generate_imports(ctx, imports, &variant.algebraic_type);
    }
}

fn generate_imports_elements(ctx: &GenCtx, imports: &mut Imports, elements: &[ProductTypeElement]) {
    for element in elements {
        generate_imports(ctx, imports, &element.algebraic_type);
    }
}

fn module_name(name: &str) -> String {
    name.to_case(Case::Snake)
}

fn generate_imports(ctx: &GenCtx, imports: &mut Imports, ty: &AlgebraicType) {
    match ty {
        AlgebraicType::Builtin(BuiltinType::Array(ArrayType { elem_ty })) => generate_imports(ctx, imports, elem_ty),
        AlgebraicType::Builtin(BuiltinType::Map(map_type)) => {
            generate_imports(ctx, imports, &map_type.key_ty);
            generate_imports(ctx, imports, &map_type.ty);
        }
        AlgebraicType::Builtin(_) => (),
        AlgebraicType::Ref(r) => {
            let type_name = type_name(ctx, *r);
            let module_name = module_name(&type_name);
            imports.insert((module_name, type_name));
        }
        // Recurse into variants of anonymous sum types, e.g. for `Option<T>`, import `T`.
        AlgebraicType::Sum(s) => generate_imports_variants(ctx, imports, &s.variants),
        // Do we need to generate imports for fields of anonymous product types?
        _ => (),
    }
}

fn print_imports(out: &mut Indenter, imports: Imports) {
    for (module_name, type_name) in imports {
        writeln!(out, "use super::{}::{};", module_name, type_name).unwrap();
    }
}

fn gen_and_print_imports<Roots, SearchFn>(ctx: &GenCtx, out: &mut Indenter, roots: Roots, search_fn: SearchFn)
where
    SearchFn: FnOnce(&GenCtx, &mut Imports, Roots),
{
    let mut imports = HashSet::new();
    search_fn(ctx, &mut imports, roots);
    print_imports(out, imports);
}
