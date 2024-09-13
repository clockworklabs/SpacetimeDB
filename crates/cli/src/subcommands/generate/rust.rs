use super::code_indenter::CodeIndenter;
use super::{GenCtx, GenItem};
use convert_case::{Case, Casing};
use spacetimedb_lib::sats::{
    AlgebraicType, AlgebraicTypeRef, ArrayType, ProductType, ProductTypeElement, SumType, SumTypeVariant,
};
use spacetimedb_lib::{ReducerDef, TableDesc};
use spacetimedb_schema::schema::TableSchema;
use std::collections::BTreeSet;
use std::fmt::{self, Write};
use std::ops::Deref;

type Indenter = CodeIndenter<String>;

/// Pairs of (module_name, TypeName).
type Imports = BTreeSet<(String, String)>;

fn write_type_ctx<W: Write>(ctx: &GenCtx, out: &mut W, ty: &AlgebraicType) {
    write_type(&|r| type_ref_name(ctx, r), out, ty).unwrap()
}

pub fn write_type<W: Write>(ctx: &impl Fn(AlgebraicTypeRef) -> String, out: &mut W, ty: &AlgebraicType) -> fmt::Result {
    match ty {
        p if p.is_identity() => write!(out, "__sdk::Identity")?,
        p if p.is_address() => write!(out, "__sdk::Address")?,
        p if p.is_schedule_at() => write!(out, "__sdk::ScheduleAt")?,
        AlgebraicType::Sum(sum_type) => {
            if let Some(inner_ty) = sum_type.as_option() {
                write!(out, "Option::<")?;
                write_type(ctx, out, inner_ty)?;
                write!(out, ">")?;
            } else {
                write!(out, "enum ")?;
                print_comma_sep_braced(out, &sum_type.variants, |out: &mut W, elem: &_| {
                    if let Some(name) = &elem.name {
                        write!(out, "{name}: ")?;
                    }
                    write_type(ctx, out, &elem.algebraic_type)
                })?;
            }
        }
        AlgebraicType::Product(ProductType { elements }) => {
            print_comma_sep_braced(out, elements, |out: &mut W, elem: &ProductTypeElement| {
                if let Some(name) = &elem.name {
                    write!(out, "{name}: ")?;
                }
                write_type(ctx, out, &elem.algebraic_type)
            })?;
        }
        AlgebraicType::Bool => write!(out, "bool")?,
        AlgebraicType::I8 => write!(out, "i8")?,
        AlgebraicType::U8 => write!(out, "u8")?,
        AlgebraicType::I16 => write!(out, "i16")?,
        AlgebraicType::U16 => write!(out, "u16")?,
        AlgebraicType::I32 => write!(out, "i32")?,
        AlgebraicType::U32 => write!(out, "u32")?,
        AlgebraicType::I64 => write!(out, "i64")?,
        AlgebraicType::U64 => write!(out, "u64")?,
        AlgebraicType::I128 => write!(out, "i128")?,
        AlgebraicType::U128 => write!(out, "u128")?,
        AlgebraicType::I256 => write!(out, "i256")?,
        AlgebraicType::U256 => write!(out, "u256")?,
        AlgebraicType::F32 => write!(out, "f32")?,
        AlgebraicType::F64 => write!(out, "f64")?,
        AlgebraicType::String => write!(out, "String")?,
        AlgebraicType::Array(ArrayType { elem_ty }) => {
            write!(out, "Vec::<")?;
            write_type(ctx, out, elem_ty)?;
            write!(out, ">")?;
        }
        AlgebraicType::Map(ty) => {
            // TODO: Should `AlgebraicType::Map` translate to `HashMap`? This requires
            //       that any map-key type implement `Hash`. We'll have to derive hash
            //       on generated types, and notably, `HashMap` is not itself `Hash`,
            //       so any type that holds a `Map` cannot derive `Hash` and cannot
            //       key a `Map`.
            // UPDATE: No, `AlgebraicType::Map` is supposed to be `BTreeMap`. Fix this.
            //         This will require deriving `Ord` for generated types,
            //         and is likely to be a big headache.
            write!(out, "HashMap::<")?;
            write_type(ctx, out, &ty.key_ty)?;
            write!(out, ", ")?;
            write_type(ctx, out, &ty.ty)?;
            write!(out, ">")?;
        }
        AlgebraicType::Ref(r) => {
            write!(out, "{}", normalize_type_name(&ctx(*r)))?;
        }
    }
    Ok(())
}

pub fn type_name(ctx: &GenCtx, ty: &AlgebraicType) -> String {
    let mut s = String::new();
    write_type_ctx(ctx, &mut s, ty);
    s
}

pub fn normalize_type_name(name: &str) -> String {
    name.replace("r#", "").to_case(Case::Pascal)
}

fn print_comma_sep_braced<W: Write, T>(
    out: &mut W,
    elems: &[T],
    on: impl Fn(&mut W, &T) -> fmt::Result,
) -> fmt::Result {
    write!(out, "{{")?;

    let mut iter = elems.iter();

    // First factor.
    if let Some(elem) = iter.next() {
        write!(out, " ")?;
        on(out, elem)?;
    }
    // Other factors.
    for elem in iter {
        write!(out, ", ")?;
        on(out, elem)?;
    }

    if !elems.is_empty() {
        write!(out, " ")?;
    }

    write!(out, "}}")?;

    Ok(())
}

// This is (effectively) duplicated in [typescript.rs] as `typescript_typename` and in
// [csharp.rs] as `csharp_typename`, and should probably be lifted to a shared utils
// module.
fn type_ref_name(ctx: &GenCtx, typeref: AlgebraicTypeRef) -> String {
    ctx.names[typeref.idx()]
        .as_deref()
        .expect("TypeRefs should have names")
        .to_case(Case::Pascal)
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

const ALLOW_UNUSED: &str = "#![allow(unused)]";

const SPACETIMEDB_IMPORTS: &[&str] = &[
    "use spacetimedb_sdk::{",
    "\tself as __sdk,",
    "\tanyhow::{self as __anyhow, Context as _},",
    "\tspacetimedb_lib as __lib,",
    "\tws_messages as __ws,",
    "};",
];

fn print_spacetimedb_imports(output: &mut Indenter) {
    print_lines(output, SPACETIMEDB_IMPORTS);
}

fn print_file_header(output: &mut Indenter) {
    print_auto_generated_file_comment(output);
    write!(output, "{ALLOW_UNUSED}");
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

const ENUM_DERIVES: &[&str] = &["#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]"];

fn print_enum_derives(output: &mut Indenter) {
    print_lines(output, ENUM_DERIVES);
}

pub fn autogen_rust_type(ctx: &GenCtx, name: &str, ty_ref: AlgebraicTypeRef) -> (String, String) {
    let type_name = normalize_type_name(name);
    let file_name = type_file_name(&type_name);
    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    print_file_header(out);

    let this_file = (file_name.as_str(), name);

    let ty = ctx.typespace.get(ty_ref).unwrap();

    match ty {
        AlgebraicType::Sum(sum) => {
            gen_and_print_imports(ctx, out, &sum.variants[..], generate_imports_variants, this_file)
        }
        AlgebraicType::Product(prod) => {
            gen_and_print_imports(ctx, out, &prod.elements[..], generate_imports_elements, this_file)
        }
        _ => unimplemented!("SATS does not support type aliases except for product and sum types"),
    }

    out.newline();

    match ty {
        AlgebraicType::Sum(sum) => define_enum_for_sum(out, ctx, &type_name, sum),
        AlgebraicType::Product(prod) => define_struct_for_product(out, ctx, &type_name, &prod.elements),
        _ => unreachable!(),
    }

    out.newline();

    writeln!(
        out,
        "
impl __sdk::spacetime_module::InModule for {type_name} {{
    type Module = super::RemoteModule;
}}
",
    );

    (file_name, output.into_inner())
}

/// Generate a file which defines an `enum` corresponding to the `sum_type`.
pub fn define_enum_for_sum(out: &mut Indenter, ctx: &GenCtx, name: &str, sum_type: &SumType) {
    print_file_header(out);

    print_enum_derives(out);

    write!(out, "pub enum {name} ");

    out.delimited_block(
        "{",
        |out| {
            for variant in &*sum_type.variants {
                write_enum_variant(ctx, out, variant);
                out.newline();
            }
        },
        "}\n",
    );

    out.newline()
}

fn write_enum_variant(ctx: &GenCtx, out: &mut Indenter, variant: &SumTypeVariant) {
    let Some(name) = &variant.name else {
        panic!("Sum type variant has no name: {variant:?}");
    };
    let name = name.deref().to_case(Case::Pascal);
    write!(out, "{name}");
    match &variant.algebraic_type {
        AlgebraicType::Product(ProductType { elements }) if elements.is_empty() => {
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
            write_type_ctx(ctx, out, otherwise);
            write!(out, "),");
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
    out: &mut impl Write,
    elements: &[ProductTypeElement],

    // Written before each line. Useful for `pub`.
    prefix: Option<&str>,
) {
    write_arglist_no_delimiters(&|r| type_ref_name(ctx, r), out, elements, prefix).unwrap()
}

pub fn write_arglist_no_delimiters(
    ctx: &impl Fn(AlgebraicTypeRef) -> String,
    out: &mut impl Write,
    elements: &[ProductTypeElement],

    // Written before each line. Useful for `pub`.
    prefix: Option<&str>,
) -> fmt::Result {
    for elt in elements {
        if let Some(prefix) = prefix {
            write!(out, "{prefix} ")?;
        }

        let Some(name) = &elt.name else {
            panic!("Product type element has no name: {elt:?}");
        };
        let name = name.deref().to_case(Case::Snake);

        write!(out, "{name}: ")?;
        write_type(ctx, out, &elt.algebraic_type)?;
        writeln!(out, ",")?;
    }
    Ok(())
}

#[allow(deprecated)]
pub fn autogen_rust_table(ctx: &GenCtx, table: &TableDesc) -> (String, String) {
    let file_name = table_file_name(table);

    let type_ref = table.data;

    let table = TableSchema::from_def(0.into(), table.schema.clone())
        .validated()
        .expect("Failed to generate table due to validation errors");

    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    print_file_header(out);

    let row_type = type_ref_name(ctx, type_ref);
    let row_type_module = type_ref_module_name(ctx, type_ref);
    let table_name = table.table_name.to_string();
    let table_name_pascalcase = table_name.to_case(Case::Pascal);

    write!(out, "use super::{row_type_module}::{row_type};");

    // TODO: Import types of indexed and unique fields.

    out.newline();

    let table_handle = table_name_pascalcase.clone() + "TableHandle";
    let insert_callback_id = table_name_pascalcase.clone() + "InsertCallbackId";
    let delete_callback_id = table_name_pascalcase.clone() + "DeleteCallbackId";

    write!(
        out,
        "
pub struct {table_handle}<'ctx> {{
    imp: __sdk::db_connection::TableHandle<{row_type}>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}}

#[allow(non_camel_case_types)]
pub trait {table_name} {{
    fn {table_name}(&self) -> {table_handle}<'_>;
}}

impl {table_name} for super::RemoteTables {{
    fn {table_name}(&self) -> {table_handle}<'_> {{
        {table_handle} {{
            imp: self.imp.get_table::<{row_type}>({table_name:?}),
            ctx: std::marker::PhantomData,
        }}
    }}
}}

pub struct {insert_callback_id}(__sdk::callbacks::CallbackId);
pub struct {delete_callback_id}(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for {table_handle}<'ctx> {{
    type Row = {row_type};
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {{ self.imp.count() }}
    fn iter(&self) -> impl Iterator<Item = {row_type}> + '_ {{ self.imp.iter() }}

    type InsertCallbackId = {insert_callback_id};

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> {insert_callback_id} {{
        {insert_callback_id}(self.imp.on_insert(Box::new(callback)))
    }}

    fn remove_on_insert(&self, callback: {insert_callback_id}) {{
        self.imp.remove_on_insert(callback.0)
    }}

    type DeleteCallbackId = {delete_callback_id};

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> {delete_callback_id} {{
        {delete_callback_id}(self.imp.on_delete(Box::new(callback)))
    }}

    fn remove_on_delete(&self, callback: {delete_callback_id}) {{
        self.imp.remove_on_delete(callback.0)
    }}
}}
"
    );

    if let Some(pk_field) = table.pk() {
        let update_callback_id = table_name_pascalcase.clone() + "UpdateCallbackId";

        let pk_field_name = pk_field.col_name.deref().to_case(Case::Snake);
        let pk_field_type = type_name(ctx, &pk_field.col_type);

        // TODO: import pk_field_type

        write!(
            out,
            "
pub struct {update_callback_id}(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::TableWithPrimaryKey for {table_handle}<'ctx> {{
    type UpdateCallbackId = {update_callback_id};

    fn on_update(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row, &Self::Row) + Send + 'static,
    ) -> {update_callback_id} {{
        {update_callback_id}(self.imp.on_update(Box::new(callback)))
    }}

    fn remove_on_update(&self, callback: {update_callback_id}) {{
        self.imp.remove_on_update(callback.0)
    }}
}}

pub(super) fn parse_table_update(
    deletes: Vec<__ws::EncodedValue>,
    inserts: Vec<__ws::EncodedValue>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<{row_type}>> {{
    __sdk::spacetime_module::TableUpdate::parse_table_update_with_primary_key::<{pk_field_type}>(
        deletes,
        inserts,
        |row: &{row_type}| &row.{pk_field_name},
    ).context(\"Failed to parse table update for table \\\"{table_name}\\\"\")
}}
"
        );
    } else {
        write!(
            out,
            "
pub(super) fn parse_table_update(
    deletes: Vec<__ws::EncodedValue>,
    inserts: Vec<__ws::EncodedValue>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<{row_type}>> {{
    __sdk::spacetime_module::TableUpdate::parse_table_update_no_primary_key(deletes, inserts)
        .context(\"Failed to parse table update for table \\\"{table_name}\\\"\")
}}
"
        )
    }

    for index in &table.indexes {
        if index.is_unique {
            let col_id = index
                .columns
                .as_singleton()
                .expect("Multi-column unique indexes are not supported");
            let unique_col = table.get_column(col_id.idx()).unwrap();

            let unique_field_name = unique_col.col_name.deref().to_case(Case::Snake);
            let unique_field_name_pascalcase = unique_field_name.to_case(Case::Pascal);

            let unique_constraint = table_name_pascalcase.clone() + &unique_field_name_pascalcase + "Unique";

            let unique_field_type = type_name(ctx, &unique_col.col_type);

            write!(
                out,
                "
pub struct {unique_constraint}<'ctx> {{
    imp: __sdk::client_cache::UniqueConstraint<{row_type}, {unique_field_type}>,
    phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
}}

impl<'ctx> {table_handle}<'ctx> {{
    pub fn {unique_field_name}(&self) -> {unique_constraint}<'ctx> {{
        {unique_constraint} {{
            imp: self.imp.get_unique_constraint::<{unique_field_type}>({unique_field_name:?}, |row| &row.{unique_field_name}),
            phantom: std::marker::PhantomData,
        }}
    }}
}}

impl<'ctx> {unique_constraint}<'ctx> {{
    pub fn find(&self, col_val: &{unique_field_type}) -> Option<{row_type}> {{
        self.imp.find(col_val)
    }}
}}
"
            );
        } else {
            todo!("Expose filter methods for non-unique indexes");
        }
    }

    (file_name, output.into_inner())
}

// TODO: figure out if/when product types should derive:
// - Clone
// - Debug
// - Copy
// - PartialEq, Eq
// - Hash
//    - Complicated because `HashMap` is not `Hash`.
// - others?

fn table_file_name(table: &TableDesc) -> String {
    table_module_name(table) + ".rs"
}

fn type_file_name(type_name: &str) -> String {
    type_module_name(type_name) + ".rs"
}

fn reducer_file_name(reducer: &ReducerDef) -> String {
    reducer_module_name(reducer) + ".rs"
}

const STRUCT_DERIVES: &[&str] = &["#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]"];

fn print_struct_derives(output: &mut Indenter) {
    print_lines(output, STRUCT_DERIVES);
}

fn define_struct_for_product(out: &mut Indenter, ctx: &GenCtx, name: &str, elements: &[ProductTypeElement]) {
    print_struct_derives(out);

    write!(out, "pub struct {name} ");

    // TODO: if elements is empty, define a unit struct with no brace-delimited list of fields.
    write_struct_type_fields_in_braces(
        ctx, out, elements, true, // `pub`-qualify fields.
    );

    out.newline();
}

fn type_ref_module_name(ctx: &GenCtx, typeref: AlgebraicTypeRef) -> String {
    type_module_name(&type_ref_name(ctx, typeref))
}

fn type_module_name(type_name: &str) -> String {
    type_name.to_case(Case::Snake) + "_type"
}

fn table_module_name(desc: &TableDesc) -> String {
    let mut name = desc.schema.table_name.to_string().to_case(Case::Snake);
    name.push_str("_table");
    name
}

fn reducer_args_type_name(reducer: &ReducerDef) -> String {
    normalize_type_name(&reducer.name)
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

pub fn autogen_rust_reducer(ctx: &GenCtx, reducer: &ReducerDef) -> (String, String) {
    let file_name = reducer_file_name(reducer);
    let func_name = reducer_function_name(reducer);
    let args_type = reducer_args_type_name(reducer);

    let callback_id = args_type.clone() + "CallbackId";

    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    print_file_header(out);

    define_struct_for_product(out, ctx, &args_type, &reducer.args);

    let mut arglist = String::new();
    write_arglist_no_delimiters_ctx(ctx, &mut arglist, &reducer.args, None);

    let mut arg_types_ref_list = String::new();
    let mut arg_names_list = String::new();
    let mut unboxed_arg_refs = String::new();
    for arg in &reducer.args {
        arg_types_ref_list += "&";
        write_type_ctx(ctx, &mut arg_types_ref_list, &arg.algebraic_type);
        arg_types_ref_list += ", ";
        let arg_name = arg.name.as_ref().expect("Reducer arguments must be named");
        arg_names_list += arg_name;
        arg_names_list += ", ";
        unboxed_arg_refs += &format!("&args.{arg_name}, ");
    }

    writeln!(
        out,
        "
impl __sdk::spacetime_module::InModule for {args_type} {{
    type Module = super::RemoteModule;
}}

pub struct {callback_id}(__sdk::callbacks::CallbackId);

#[allow(non_camel_case_types)]
pub trait {func_name} {{
    fn {func_name}(&self, {arglist}) -> __anyhow::Result<()>;
    fn on_{func_name}(&self, callback: impl FnMut(&super::EventContext, {arg_types_ref_list}) + Send + 'static) -> {callback_id};
    fn remove_on_{func_name}(&self, callback: {callback_id});
}}

impl {func_name} for super::RemoteReducers {{
    fn {func_name}(&self, {arglist}) -> __anyhow::Result<()> {{
        self.imp.call_reducer({func_name:?}, {args_type} {{ {arg_names_list} }})
    }}
    fn on_{func_name}(
        &self,
        mut callback: impl FnMut(&super::EventContext, {arg_types_ref_list}) + Send + 'static,
    ) -> {callback_id} {{
        {callback_id}(self.imp.on_reducer::<{args_type}>(
            {func_name:?},
            Box::new(move |ctx: &super::EventContext, args: &{args_type}| callback(ctx, {unboxed_arg_refs})),
        ))
    }}
    fn remove_on_{func_name}(&self, callback: {callback_id}) {{
        self.imp.remove_on_reducer::<{args_type}>({func_name:?}, callback.0)
    }}
}}
"
    );

    (file_name, output.into_inner())
}

/// Generate a `mod.rs` as the entry point into the autogenerated code.
pub fn autogen_rust_globals(ctx: &GenCtx, items: &[GenItem]) -> Vec<(String, String)> {
    let mut output = CodeIndenter::new(String::new());
    let out = &mut output;

    print_file_header(out);

    out.newline();

    // Declare `pub mod` for each of the files generated.
    print_module_decls(out, items);

    out.newline();

    // Re-export all the modules for the generated files.
    print_module_reexports(out, items);

    out.newline();

    // Define `enum Reducer`.
    print_reducer_enum_defn(out, items);

    out.newline();

    // Define `DbUpdate`.
    print_db_update_defn(ctx, out, items);

    out.newline();

    // Define `RemoteModule`, `DbConnection`, `EventContext`, `RemoteTables`, `RemoteReducers` and `SubscriptionHandle`.
    // Note that these do not change based on the module.
    print_const_db_context_types(out);

    vec![("mod.rs".to_string(), output.into_inner())]
}

fn iter_reducer_items(items: &[GenItem]) -> impl Iterator<Item = &ReducerDef> {
    items.iter().filter_map(|item| match item {
        GenItem::Reducer(reducer) => Some(reducer),
        _ => None,
    })
}

fn iter_table_items(items: &[GenItem]) -> impl Iterator<Item = &TableDesc> {
    items.iter().filter_map(|item| match item {
        GenItem::Table(table) => Some(table),
        _ => None,
    })
}

fn iter_module_names(items: &[GenItem]) -> impl Iterator<Item = String> + '_ {
    items.iter().map(|item| match item {
        GenItem::Table(table) => table_module_name(table),
        GenItem::TypeAlias(ty) => type_module_name(&ty.name),
        GenItem::Reducer(reducer) => reducer_module_name(reducer),
    })
}

/// Print `pub mod` declarations for all the files that will be generated for `items`.
fn print_module_decls(out: &mut Indenter, items: &[GenItem]) {
    for module_name in iter_module_names(items) {
        writeln!(out, "pub mod {module_name};");
    }
}

/// Print `pub use *` declarations for all the files that will be generated for `items`.
fn print_module_reexports(out: &mut Indenter, items: &[GenItem]) {
    for module_name in iter_module_names(items) {
        writeln!(out, "pub use {module_name}::*;");
    }
}

fn print_reducer_enum_defn(out: &mut Indenter, items: &[GenItem]) {
    print_enum_derives(out);
    out.delimited_block(
        "pub enum Reducer {",
        |out| {
            for reducer in iter_reducer_items(items) {
                writeln!(
                    out,
                    "{}({}::{}),",
                    reducer_variant_name(reducer),
                    reducer_module_name(reducer),
                    reducer_args_type_name(reducer),
                );
            }
        },
        "}\n",
    );
    out.newline();
    writeln!(
        out,
        "
impl __sdk::spacetime_module::InModule for Reducer {{
    type Module = RemoteModule;
}}
",
    );

    out.delimited_block(
        "impl __sdk::spacetime_module::Reducer for Reducer {",
        |out| {
            out.delimited_block(
                "fn reducer_name(&self) -> &'static str {",
                |out| {
                    out.delimited_block(
                        "match self {",
                        |out| {
                            for reducer in iter_reducer_items(items) {
                                writeln!(
                                    out,
                                    "Reducer::{}(_) => {:?},",
                                    reducer_variant_name(reducer),
                                    reducer.name,
                                );
                            }
                        },
                        "}\n",
                    );
                },
                "}\n",
            );
            out.delimited_block(
                "fn reducer_args(&self) -> &dyn std::any::Any {",
                |out| {
                    out.delimited_block(
                        "match self {",
                        |out| {
                            for reducer in iter_reducer_items(items) {
                                writeln!(out, "Reducer::{}(args) => args,", reducer_variant_name(reducer));
                            }
                        },
                        "}\n",
                    );
                },
                "}\n",
            );
        },
        "}\n",
    );

    out.delimited_block(
        "impl TryFrom<__ws::ReducerCallInfo> for Reducer {",
        |out| {
            writeln!(out, "type Error = __anyhow::Error;");
            out.delimited_block(
                "fn try_from(value: __ws::ReducerCallInfo) -> __anyhow::Result<Self> {",
                    |out| {
                        out.delimited_block(
                            "match &value.reducer_name[..] {",
                            |out| {
                                for reducer in iter_reducer_items(items) {
                                    writeln!(
                                        out,
                                        "{:?} => Ok(Reducer::{}(__sdk::spacetime_module::parse_reducer_args({:?}, &value.args)?)),",
                                        reducer.name,
                                        reducer_variant_name(reducer),
                                        reducer.name,
                                    );
                                }
                                writeln!(
                                    out,
                                    "_ => Err(__anyhow::anyhow!(\"Unknown reducer {{:?}}\", value.reducer_name)),",
                                );
                            },
                            "}\n",
                        )
                    },
                "}\n",
            );
        },
        "}\n",
    )
}

fn print_db_update_defn(ctx: &GenCtx, out: &mut Indenter, items: &[GenItem]) {
    writeln!(out, "#[derive(Default)]");
    out.delimited_block(
        "pub struct DbUpdate {",
        |out| {
            for table in iter_table_items(items) {
                writeln!(
                    out,
                    "{}: __sdk::spacetime_module::TableUpdate<{}>,",
                    table.schema.table_name,
                    type_ref_name(ctx, table.data),
                );
            }
        },
        "}\n",
    );

    out.newline();

    out.delimited_block(
        "
impl TryFrom<__ws::DatabaseUpdate> for DbUpdate {
    type Error = __anyhow::Error;
    fn try_from(raw: __ws::DatabaseUpdate) -> Result<Self, Self::Error> {
        let mut db_update = DbUpdate::default();
        for table_update in raw.tables {
            match &table_update.table_name[..] {
",
        |out| {
            for table in iter_table_items(items) {
                writeln!(
                    out,
                    "{:?} => db_update.{} = {}::parse_table_update(table_update.deletes, table_update.inserts)?,",
                    table.schema.table_name,
                    table.schema.table_name,
                    table_module_name(table),
                );
            }
        },
        "
                unknown => __anyhow::bail!(\"Unknown table {unknown:?} in DatabaseUpdate\"),
            }
        }
        Ok(db_update)
    }
}",
    );

    out.newline();

    writeln!(
        out,
        "
impl __sdk::spacetime_module::InModule for DbUpdate {{
    type Module = RemoteModule;
}}
",
    );

    out.delimited_block(
        "impl __sdk::spacetime_module::DbUpdate for DbUpdate {",
        |out| {
            out.delimited_block(
                "fn apply_to_client_cache(&self, cache: &mut __sdk::client_cache::ClientCache<RemoteModule>) {",
                |out| {
                    for table in iter_table_items(items) {
                        writeln!(
                            out,
                            "cache.apply_diff_to_table::<{}>({:?}, &self.{});",
                            type_ref_name(ctx, table.data),
                            table.schema.table_name,
                            table.schema.table_name,
                        );
                    }
                },
                "}\n",
            );

            out.delimited_block(
                "fn invoke_row_callbacks(&self, event: &EventContext, callbacks: &mut __sdk::callbacks::DbCallbacks<RemoteModule>) {",
                |out| {
                    for table in iter_table_items(items) {
                        writeln!(
                            out,
                            "callbacks.invoke_table_row_callbacks::<{}>({:?}, &self.{}, event);",
                            type_ref_name(ctx, table.data),
                            table.schema.table_name,
                            table.schema.table_name,
                        );
                    }
                },
                "}\n",
            );
        },
        "}\n",
    );
}

fn print_const_db_context_types(out: &mut Indenter) {
    writeln!(
        out,
        "
pub struct RemoteModule;

impl __sdk::spacetime_module::InModule for RemoteModule {{
    type Module = Self;
}}

impl __sdk::spacetime_module::SpacetimeModule for RemoteModule {{
    type DbConnection = DbConnection;
    type EventContext = EventContext;
    type Reducer = Reducer;
    type DbView = RemoteTables;
    type Reducers = RemoteReducers;
    type DbUpdate = DbUpdate;
    type SubscriptionHandle = SubscriptionHandle;
}}

pub struct RemoteReducers {{
    imp: __sdk::db_connection::DbContextImpl<RemoteModule>,
}}

impl __sdk::spacetime_module::InModule for RemoteReducers {{
    type Module = RemoteModule;
}}

pub struct RemoteTables {{
    imp: __sdk::db_connection::DbContextImpl<RemoteModule>,
}}

impl __sdk::spacetime_module::InModule for RemoteTables {{
    type Module = RemoteModule;
}}

pub struct DbConnection {{
    pub db: RemoteTables,
    pub reducers: RemoteReducers,

    imp: __sdk::db_connection::DbContextImpl<RemoteModule>,
}}

impl __sdk::spacetime_module::InModule for DbConnection {{
    type Module = RemoteModule;
}}

impl __sdk::db_context::DbContext for DbConnection {{
    type DbView = RemoteTables;
    type Reducers = RemoteReducers;

    fn db(&self) -> &Self::DbView {{
        &self.db
    }}
    fn reducers(&self) -> &Self::Reducers {{
        &self.reducers
    }}

    fn is_active(&self) -> bool {{
        self.imp.is_active()
    }}

    fn disconnect(&self) -> __anyhow::Result<()> {{
        self.imp.disconnect()
    }}

    type SubscriptionBuilder = __sdk::subscription::SubscriptionBuilder<RemoteModule>;

    fn subscription_builder(&self) -> Self::SubscriptionBuilder {{
        __sdk::subscription::SubscriptionBuilder::new(&self.imp)
    }}
}}

impl DbConnection {{
    pub fn builder() -> __sdk::db_connection::DbConnectionBuilder<RemoteModule> {{
        __sdk::db_connection::DbConnectionBuilder::new()
    }}

    pub fn advance_one_message(&self) -> __anyhow::Result<bool> {{
        self.imp.advance_one_message()
    }}

    pub fn advance_one_message_blocking(&self) -> __anyhow::Result<()> {{
        self.imp.advance_one_message_blocking()
    }}

    pub async fn advance_one_message_async(&self) -> __anyhow::Result<()> {{
        self.imp.advance_one_message_async().await
    }}

    pub fn frame_tick(&self) -> __anyhow::Result<()> {{
        self.imp.frame_tick()
    }}

    pub fn run_threaded(&self) -> std::thread::JoinHandle<()> {{
        self.imp.run_threaded()
    }}

    pub async fn run_async(&self) -> __anyhow::Result<()> {{
        self.imp.run_async().await
    }}
}}

impl __sdk::spacetime_module::DbConnection for DbConnection {{
    fn new(imp: __sdk::db_connection::DbContextImpl<RemoteModule>) -> Self {{
        Self {{
            db: RemoteTables {{ imp: imp.clone() }},
            reducers: RemoteReducers {{ imp: imp.clone() }},
            imp,
        }}
    }}
}}

pub struct EventContext {{
    pub db: RemoteTables,
    pub reducers: RemoteReducers,
    pub event: __sdk::event::Event<Reducer>,
    imp: __sdk::db_connection::DbContextImpl<RemoteModule>,
}}

impl __sdk::spacetime_module::InModule for EventContext {{
    type Module = RemoteModule;
}}

impl __sdk::db_context::DbContext for EventContext {{
    type DbView = RemoteTables;
    type Reducers = RemoteReducers;

    fn db(&self) -> &Self::DbView {{
        &self.db
    }}
    fn reducers(&self) -> &Self::Reducers {{
        &self.reducers
    }}

    fn is_active(&self) -> bool {{
        self.imp.is_active()
    }}

    fn disconnect(&self) -> spacetimedb_sdk::anyhow::Result<()> {{
        self.imp.disconnect()
    }}

    type SubscriptionBuilder = __sdk::subscription::SubscriptionBuilder<RemoteModule>;

    fn subscription_builder(&self) -> Self::SubscriptionBuilder {{
        __sdk::subscription::SubscriptionBuilder::new(&self.imp)
    }}
}}

impl __sdk::spacetime_module::EventContext for EventContext {{
    fn event(&self) -> &__sdk::event::Event<Reducer> {{
        &self.event
    }}
    fn new(imp: __sdk::db_connection::DbContextImpl<RemoteModule>, event: __sdk::event::Event<Reducer>) -> Self {{
        Self {{
            db: RemoteTables {{ imp: imp.clone() }},
            reducers: RemoteReducers {{ imp: imp.clone() }},
            event,
            imp,
        }}
    }}
}}

pub struct SubscriptionHandle {{
    imp: __sdk::subscription::SubscriptionHandleImpl<RemoteModule>,
}}

impl __sdk::spacetime_module::InModule for SubscriptionHandle {{
    type Module = RemoteModule;
}}

impl __sdk::spacetime_module::SubscriptionHandle for SubscriptionHandle {{
    fn new(imp: __sdk::subscription::SubscriptionHandleImpl<RemoteModule>) -> Self {{
        Self {{ imp }}
    }}
}}
",
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

fn generate_imports(ctx: &GenCtx, imports: &mut Imports, ty: &AlgebraicType) {
    match ty {
        AlgebraicType::Array(ArrayType { elem_ty }) => generate_imports(ctx, imports, elem_ty),
        AlgebraicType::Map(map_type) => {
            generate_imports(ctx, imports, &map_type.key_ty);
            generate_imports(ctx, imports, &map_type.ty);
        }
        AlgebraicType::Ref(r) => {
            let type_name = type_ref_name(ctx, *r);
            let module_name = type_ref_module_name(ctx, *r);
            imports.insert((module_name, type_name));
        }
        // Recurse into variants of anonymous sum types, e.g. for `Option<T>`, import `T`.
        AlgebraicType::Sum(s) => generate_imports_variants(ctx, imports, &s.variants),
        // Products, scalars, and strings.
        // Do we need to generate imports for fields of anonymous product types?
        _ => {}
    }
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
fn gen_and_print_imports<Roots, SearchFn>(
    ctx: &GenCtx,
    out: &mut Indenter,
    roots: Roots,
    search_fn: SearchFn,
    this_file: (&str, &str),
) where
    SearchFn: FnOnce(&GenCtx, &mut Imports, Roots),
{
    let mut imports = BTreeSet::new();
    search_fn(ctx, &mut imports, roots);

    print_imports(out, imports, this_file);
}
