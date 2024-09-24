use super::code_indenter::CodeIndenter;
use super::Lang;
use convert_case::{Case, Casing};
use itertools::Itertools;
use spacetimedb_lib::sats::AlgebraicTypeRef;
use spacetimedb_primitives::ColList;
use spacetimedb_schema::def::{ModuleDef, ReducerDef, ScopedTypeName, TableDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_schema::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse, PrimitiveType};
use std::collections::BTreeSet;
use std::fmt::{self, Write};
use std::ops::Deref;

type Indenter = CodeIndenter<String>;

/// Pairs of (module_name, TypeName).
type Imports = BTreeSet<AlgebraicTypeRef>;

fn collect_case<'a>(case: Case, segs: impl Iterator<Item = &'a Identifier>) -> String {
    segs.map(|s| s.deref().to_case(case)).join(case.delim())
}

fn namespace_is_acceptable(requested_namespace: &str) -> bool {
    // The `spacetime generate` CLI sets a default namespace of `SpacetimeDB.Types`,
    // which is intended to be consumed only by C# codegen,
    // but is passed to all codegen languages including Rust.
    // We want to assert that the user did not explicitly request a different namespace,
    // since we have no way to emit one.
    // So, check that the namespace either is empty or is the default.
    requested_namespace.is_empty() || requested_namespace == "SpacetimeDB.Types"
}

pub struct Rust;

impl Lang for Rust {
    fn table_filename(
        &self,
        _module: &spacetimedb_schema::def::ModuleDef,
        table: &spacetimedb_schema::def::TableDef,
    ) -> String {
        table_module_name(&table.name) + ".rs"
    }

    fn type_filename(&self, type_name: &ScopedTypeName) -> String {
        type_module_name(type_name) + ".rs"
    }

    fn reducer_filename(&self, reducer_name: &Identifier) -> String {
        reducer_module_name(reducer_name) + ".rs"
    }

    fn generate_type(&self, module: &ModuleDef, namespace: &str, typ: &TypeDef) -> String {
        assert!(
            namespace_is_acceptable(namespace),
            "Rust codegen does not support namespaces, as Rust equates namespaces with `mod`s.

Requested namespace: {namespace}",
        );
        let type_name = collect_case(Case::Pascal, typ.name.name_segments());

        let mut output = CodeIndenter::new(String::new());
        let out = &mut output;

        print_file_header(out);
        out.newline();

        match &module.typespace_for_generate()[typ.ty] {
            AlgebraicTypeDef::Product(product) => {
                gen_and_print_imports(module, out, &product.elements, &[typ.ty]);
                out.newline();
                define_struct_for_product(module, out, &type_name, &product.elements);
            }
            AlgebraicTypeDef::Sum(sum) => {
                gen_and_print_imports(module, out, &sum.variants, &[typ.ty]);
                out.newline();
                define_enum_for_sum(module, out, &type_name, &sum.variants);
            }
            AlgebraicTypeDef::PlainEnum(plain_enum) => {
                let variants = plain_enum
                    .variants
                    .iter()
                    .cloned()
                    .map(|var| (var, AlgebraicTypeUse::Unit))
                    .collect::<Vec<_>>();
                define_enum_for_sum(module, out, &type_name, &variants);
            }
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

        output.into_inner()
    }
    fn generate_table(&self, module: &ModuleDef, namespace: &str, table: &TableDef) -> String {
        assert!(
            namespace_is_acceptable(namespace),
            "Rust codegen does not support namespaces, as Rust equates namespaces with `mod`s.

Requested namespace: {namespace}",
        );

        let schema = TableSchema::from_module_def(table, 0.into())
            .validated()
            .expect("Failed to generate table due to validation errors");

        let type_ref = table.product_type_ref;

        let mut output = CodeIndenter::new(String::new());
        let out = &mut output;

        print_file_header(out);

        let row_type = type_ref_name(module, type_ref);
        let row_type_module = type_ref_module_name(module, type_ref);

        writeln!(out, "use super::{row_type_module}::{row_type};");

        let product_def = module.typespace_for_generate()[type_ref].as_product().unwrap();

        // Import the types of all fields.
        // We only need to import fields which have indices or unique constraints,
        // but it's easier to just import all of 'em, since we have `#![allow(unused)]` anyway.
        gen_and_print_imports(
            module,
            out,
            &product_def.elements,
            &[], // No need to skip any imports; we're not defining a type, so there's no chance of circular imports.
        );

        let table_name = table.name.deref();
        let table_name_pascalcase = table.name.deref().to_case(Case::Pascal);
        let table_handle = table_name_pascalcase.clone() + "TableHandle";
        let insert_callback_id = table_name_pascalcase.clone() + "InsertCallbackId";
        let delete_callback_id = table_name_pascalcase.clone() + "DeleteCallbackId";
        let accessor_trait = table_name_pascalcase.clone() + "TableAccess";
        let accessor_method = table_method_name(&table.name);

        write!(
            out,
            "
pub struct {table_handle}<'ctx> {{
    imp: __sdk::db_connection::TableHandle<{row_type}>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}}

#[allow(non_camel_case_types)]
pub trait {accessor_trait} {{
    #[allow(non_snake_case)]
    fn {accessor_method}(&self) -> {table_handle}<'_>;
}}

impl {accessor_trait} for super::RemoteTables {{
    fn {accessor_method}(&self) -> {table_handle}<'_> {{
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

        if let Some(pk_field) = schema.pk() {
            let update_callback_id = table_name_pascalcase.clone() + "UpdateCallbackId";

            let (pk_field_ident, pk_field_type_use) = &product_def.elements[pk_field.col_pos.idx()];
            let pk_field_name = pk_field_ident.deref().to_case(Case::Snake);
            let pk_field_type = type_name(module, pk_field_type_use);

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
            );
        }

        let constraints = schema.column_constraints();

        for field in schema.columns() {
            if constraints[&ColList::from(field.col_pos)].has_unique() {
                let (unique_field_ident, unique_field_type_use) = &product_def.elements[field.col_pos.idx()];
                let unique_field_name = unique_field_ident.deref().to_case(Case::Snake);
                let unique_field_name_pascalcase = unique_field_name.to_case(Case::Pascal);

                let unique_constraint = table_name_pascalcase.clone() + &unique_field_name_pascalcase + "Unique";
                let unique_field_type = type_name(module, unique_field_type_use);

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
            }
        }

        // TODO: expose non-unique indices.

        output.into_inner()
    }
    fn generate_reducer(&self, module: &ModuleDef, namespace: &str, reducer: &ReducerDef) -> String {
        assert!(
            namespace_is_acceptable(namespace),
            "Rust codegen does not support namespaces, as Rust equates namespaces with `mod`s.

Requested namespace: {namespace}",
        );

        let mut output = CodeIndenter::new(String::new());
        let out = &mut output;

        print_file_header(out);

        out.newline();

        gen_and_print_imports(
            module,
            out,
            &reducer.params_for_generate.elements,
            // No need to skip any imports; we're not emitting a type that other modules can import.
            &[],
        );

        out.newline();

        let reducer_name = reducer.name.deref();
        let func_name = reducer_function_name(reducer);
        let args_type = reducer_args_type_name(&reducer.name);

        define_struct_for_product(module, out, &args_type, &reducer.params_for_generate.elements);

        out.newline();

        let callback_id = args_type.clone() + "CallbackId";

        // The reducer arguments as `ident: ty, ident: ty, ident: ty,`,
        // like an argument list.
        let mut arglist = String::new();
        write_arglist_no_delimiters(module, &mut arglist, &reducer.params_for_generate.elements, None).unwrap();

        // The reducer argument types as `&ty, &ty, &ty`,
        // for use as the params in a `FnMut` closure type.
        let mut arg_types_ref_list = String::new();
        // The reducer argument names as `ident, ident, ident`,
        // for passing to function call and struct literal expressions.
        let mut arg_names_list = String::new();
        // The reducer argument names as `&args.ident, &args.ident, &args.ident`,
        // for extracting from a structure named `args` by reference
        // and passing to a function call.
        let mut unboxed_arg_refs = String::new();
        for (arg_ident, arg_ty) in &reducer.params_for_generate.elements[..] {
            arg_types_ref_list += "&";
            write_type(module, &mut arg_types_ref_list, arg_ty).unwrap();
            arg_types_ref_list += ", ";

            let arg_name = arg_ident.deref().to_case(Case::Snake);
            arg_names_list += &arg_name;
            arg_names_list += ", ";

            unboxed_arg_refs += "&args.";
            unboxed_arg_refs += &arg_name;
            unboxed_arg_refs += ", ";
        }

        // TODO: check for lifecycle reducers and do not generate the invoke method.

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
        self.imp.call_reducer({reducer_name:?}, {args_type} {{ {arg_names_list} }})
    }}
    fn on_{func_name}(
        &self,
        mut callback: impl FnMut(&super::EventContext, {arg_types_ref_list}) + Send + 'static,
    ) -> {callback_id} {{
        {callback_id}(self.imp.on_reducer::<{args_type}>(
            {reducer_name:?},
            Box::new(move |ctx: &super::EventContext, args: &{args_type}| callback(ctx, {unboxed_arg_refs})),
        ))
    }}
    fn remove_on_{func_name}(&self, callback: {callback_id}) {{
        self.imp.remove_on_reducer::<{args_type}>({reducer_name:?}, callback.0)
    }}
}}
"
        );

        output.into_inner()
    }

    fn generate_globals(&self, module: &ModuleDef, namespace: &str) -> Vec<(String, String)> {
        assert!(
            namespace_is_acceptable(namespace),
            "Rust codegen does not support namespaces, as Rust equates namespaces with `mod`s.

Requested namespace: {namespace}",
        );

        let mut output = CodeIndenter::new(String::new());
        let out = &mut output;

        print_file_header(out);

        out.newline();

        // Declare `pub mod` for each of the files generated.
        print_module_decls(module, out);

        out.newline();

        // Re-export all the modules for the generated files.
        print_module_reexports(module, out);

        out.newline();

        // Define `enum Reducer`.
        print_reducer_enum_defn(module, out);

        out.newline();

        // Define `DbUpdate`.
        print_db_update_defn(module, out);

        out.newline();

        // Define `RemoteModule`, `DbConnection`, `EventContext`, `RemoteTables`, `RemoteReducers` and `SubscriptionHandle`.
        // Note that these do not change based on the module.
        print_const_db_context_types(out);

        vec![("mod.rs".to_string(), (output.into_inner()))]
    }
}

pub fn write_type<W: Write>(module: &ModuleDef, out: &mut W, ty: &AlgebraicTypeUse) -> fmt::Result {
    match ty {
        AlgebraicTypeUse::Unit => write!(out, "()")?,
        AlgebraicTypeUse::Never => write!(out, "std::convert::Infallible")?,
        AlgebraicTypeUse::Identity => write!(out, "__sdk::Identity")?,
        AlgebraicTypeUse::Address => write!(out, "__sdk::Address")?,
        AlgebraicTypeUse::ScheduleAt => write!(out, "__sdk::ScheduleAt")?,
        AlgebraicTypeUse::Option(inner_ty) => {
            write!(out, "Option::<")?;
            write_type(module, out, inner_ty)?;
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
            PrimitiveType::I256 => write!(out, "__sats::i256")?,
            PrimitiveType::U256 => write!(out, "__sats::u256")?,
            PrimitiveType::F32 => write!(out, "f32")?,
            PrimitiveType::F64 => write!(out, "f64")?,
        },
        AlgebraicTypeUse::String => write!(out, "String")?,
        AlgebraicTypeUse::Array(elem_ty) => {
            write!(out, "Vec::<")?;
            write_type(module, out, elem_ty)?;
            write!(out, ">")?;
        }
        AlgebraicTypeUse::Map { .. } => unimplemented!("AlgebraicType::Map is unsupported and will be removed"),
        AlgebraicTypeUse::Ref(r) => {
            write!(out, "{}", type_ref_name(module, *r))?;
        }
    }
    Ok(())
}

// This is (effectively) duplicated in [typescript.rs] as `typescript_typename` and in
// [csharp.rs] as `csharp_typename`, and should probably be lifted to a shared utils
// module.
fn type_ref_name(module: &ModuleDef, typeref: AlgebraicTypeRef) -> String {
    let (name, _def) = module.type_def_from_ref(typeref).unwrap();
    collect_case(Case::Pascal, name.name_segments())
}

pub fn type_name(module: &ModuleDef, ty: &AlgebraicTypeUse) -> String {
    let mut s = String::new();
    write_type(module, &mut s, ty).unwrap();
    s
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
    "\tlib as __lib,",
    "\tsats as __sats,",
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

const ENUM_DERIVES: &[&str] = &[
    "#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]",
    "#[sats(crate = __lib)]",
];

fn print_enum_derives(output: &mut Indenter) {
    print_lines(output, ENUM_DERIVES);
}

/// Generate a file which defines an `enum` corresponding to the `sum_type`.
pub fn define_enum_for_sum(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    variants: &[(Identifier, AlgebraicTypeUse)],
) {
    print_enum_derives(out);
    write!(out, "pub enum {name} ");

    out.delimited_block(
        "{",
        |out| {
            for (ident, ty) in variants {
                write_enum_variant(module, out, ident, ty);
                out.newline();
            }
        },
        "}\n",
    );

    out.newline()
}

fn write_enum_variant(module: &ModuleDef, out: &mut Indenter, ident: &Identifier, ty: &AlgebraicTypeUse) {
    let name = ident.deref().to_case(Case::Pascal);
    write!(out, "{name}");

    // If the contained type is the unit type, i.e. this variant has no members,
    // write it without parens or braces, like
    // ```
    // Foo,
    // ```
    if !matches!(ty, AlgebraicTypeUse::Unit) {
        // If the contained type is not a product, i.e. this variant has a single
        // member, write it tuple-style, with parens.
        write!(out, "(");
        write_type(module, out, ty).unwrap();
        write!(out, ")");
    }
    writeln!(out, ",");
}

fn write_struct_type_fields_in_braces(
    module: &ModuleDef,
    out: &mut Indenter,
    elements: &[(Identifier, AlgebraicTypeUse)],

    // Whether to print a `pub` qualifier on the fields. Necessary for `struct` defns,
    // disallowed for `enum` defns.
    pub_qualifier: bool,
) {
    out.delimited_block(
        "{",
        |out| write_arglist_no_delimiters(module, out, elements, pub_qualifier.then_some("pub")).unwrap(),
        "}",
    );
}

fn write_arglist_no_delimiters(
    module: &ModuleDef,
    out: &mut impl Write,
    elements: &[(Identifier, AlgebraicTypeUse)],

    // Written before each line. Useful for `pub`.
    prefix: Option<&str>,
) -> anyhow::Result<()> {
    for (ident, ty) in elements {
        if let Some(prefix) = prefix {
            write!(out, "{prefix} ")?;
        }

        let name = ident.deref().to_case(Case::Snake);

        write!(out, "{name}: ")?;
        write_type(module, out, ty)?;
        writeln!(out, ",")?;
    }

    Ok(())
}

// TODO: figure out if/when product types should derive:
// - Clone
// - Debug
// - Copy
// - PartialEq, Eq
// - Hash
//    - Complicated because `HashMap` is not `Hash`.
// - others?

const STRUCT_DERIVES: &[&str] = &[
    "#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]",
    "#[sats(crate = __lib)]",
];

fn print_struct_derives(output: &mut Indenter) {
    print_lines(output, STRUCT_DERIVES);
}

fn define_struct_for_product(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    elements: &[(Identifier, AlgebraicTypeUse)],
) {
    print_struct_derives(out);

    write!(out, "pub struct {name} ");

    // TODO: if elements is empty, define a unit struct with no brace-delimited list of fields.
    write_struct_type_fields_in_braces(
        module, out, elements, true, // `pub`-qualify fields.
    );

    out.newline();
}

fn type_ref_module_name(module: &ModuleDef, type_ref: AlgebraicTypeRef) -> String {
    let (name, _) = module.type_def_from_ref(type_ref).unwrap();
    type_module_name(name)
}

fn type_module_name(type_name: &ScopedTypeName) -> String {
    collect_case(Case::Snake, type_name.name_segments()) + "_type"
}

fn table_module_name(table_name: &Identifier) -> String {
    table_name.deref().to_case(Case::Snake) + "_table"
}

fn table_method_name(table_name: &Identifier) -> String {
    table_name.deref().to_case(Case::Snake)
}

fn reducer_args_type_name(reducer_name: &Identifier) -> String {
    reducer_name.deref().to_case(Case::Pascal)
}

fn reducer_variant_name(reducer_name: &Identifier) -> String {
    reducer_name.deref().to_case(Case::Pascal)
}

fn reducer_module_name(reducer_name: &Identifier) -> String {
    reducer_name.deref().to_case(Case::Snake) + "_reducer"
}

fn reducer_function_name(reducer: &ReducerDef) -> String {
    reducer.name.deref().to_case(Case::Snake)
}

/// Iterate over all of the Rust `mod`s for types, reducers and tables in the `module`.
fn iter_module_names(module: &ModuleDef) -> impl Iterator<Item = String> + '_ {
    itertools::chain!(
        module.types().map(|ty| type_module_name(&ty.name)).sorted(),
        module.reducers().map(|r| reducer_module_name(&r.name)).sorted(),
        module.tables().map(|tbl| table_module_name(&tbl.name)).sorted(),
    )
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

/// Iterate over all the [`ReducerDef`]s defined by the module, in alphabetical order by name.
///
/// Sorting is necessary to have deterministic reproducable codegen.
fn iter_reducers(module: &ModuleDef) -> impl Iterator<Item = &ReducerDef> {
    module.reducers().sorted_by_key(|reducer| &reducer.name)
}

/// Iterate over all the [`TableDef`]s defined by the module, in alphabetical order by name.
///
/// Sorting is necessary to have deterministic reproducable codegen.
fn iter_tables(module: &ModuleDef) -> impl Iterator<Item = &TableDef> {
    module.tables().sorted_by_key(|table| &table.name)
}

fn print_reducer_enum_defn(module: &ModuleDef, out: &mut Indenter) {
    print_enum_derives(out);
    out.delimited_block(
        "pub enum Reducer {",
        |out| {
            for reducer in iter_reducers(module) {
                writeln!(
                    out,
                    "{}({}::{}),",
                    reducer_variant_name(&reducer.name),
                    reducer_module_name(&reducer.name),
                    reducer_args_type_name(&reducer.name),
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
                            for reducer in iter_reducers(module) {
                                writeln!(
                                    out,
                                    "Reducer::{}(_) => {:?},",
                                    reducer_variant_name(&reducer.name),
                                    reducer.name.deref(),
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
                            for reducer in iter_reducers(module) {
                                writeln!(out, "Reducer::{}(args) => args,", reducer_variant_name(&reducer.name));
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
                                for reducer in iter_reducers(module) {
                                    writeln!(
                                        out,
                                        "{:?} => Ok(Reducer::{}(__sdk::spacetime_module::parse_reducer_args({:?}, &value.args)?)),",
                                        reducer.name.deref(),
                                        reducer_variant_name(&reducer.name),
                                        reducer.name.deref(),
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

fn print_db_update_defn(module: &ModuleDef, out: &mut Indenter) {
    writeln!(out, "#[derive(Default)]");
    writeln!(out, "#[allow(non_snake_case)]");
    out.delimited_block(
        "pub struct DbUpdate {",
        |out| {
            for table in iter_tables(module) {
                writeln!(
                    out,
                    "{}: __sdk::spacetime_module::TableUpdate<{}>,",
                    table_method_name(&table.name),
                    type_ref_name(module, table.product_type_ref),
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
            for table in iter_tables(module) {
                writeln!(
                    out,
                    "{:?} => db_update.{} = {}::parse_table_update(table_update.deletes, table_update.inserts)?,",
                    table.name.deref(),
                    table_method_name(&table.name),
                    table_module_name(&table.name),
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
                    for table in iter_tables(module) {
                        writeln!(
                            out,
                            "cache.apply_diff_to_table::<{}>({:?}, &self.{});",
                            type_ref_name(module, table.product_type_ref),
                            table.name.deref(),
                            table_method_name(&table.name),
                        );
                    }
                },
                "}\n",
            );

            out.delimited_block(
                "fn invoke_row_callbacks(&self, event: &EventContext, callbacks: &mut __sdk::callbacks::DbCallbacks<RemoteModule>) {",
                |out| {
                    for table in iter_tables(module) {
                        writeln!(
                            out,
                            "callbacks.invoke_table_row_callbacks::<{}>({:?}, &self.{}, event);",
                            type_ref_name(module, table.product_type_ref),
                            table.name.deref(),
                            table_method_name(&table.name),
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

    fn try_identity(&self) -> Option<__sdk::Identity> {{
        self.imp.try_identity()
    }}
    fn address(&self) -> __sdk::Address {{
        self.imp.address()
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

    fn try_identity(&self) -> Option<__sdk::Identity> {{
        self.imp.try_identity()
    }}
    fn address(&self) -> __sdk::Address {{
        self.imp.address()
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

pub trait RemoteDbContext: __sdk::DbContext<
    DbView = RemoteTables,
    Reducers = RemoteReducers,
    SubscriptionBuilder = __sdk::subscription::SubscriptionBuilder<RemoteModule>,
> {{}}
impl<Ctx: __sdk::DbContext<
    DbView = RemoteTables,
    Reducers = RemoteReducers,
    SubscriptionBuilder = __sdk::subscription::SubscriptionBuilder<RemoteModule>,
>> RemoteDbContext for Ctx {{}}
",
    );
}

/// Print `use super::` imports for each of the `imports`.
fn print_imports(module: &ModuleDef, out: &mut Indenter, imports: Imports) {
    for typeref in imports {
        let module_name = type_ref_module_name(module, typeref);
        let type_name = type_ref_name(module, typeref);
        writeln!(out, "use super::{module_name}::{type_name};");
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
    dont_import: &[AlgebraicTypeRef],
) {
    let mut imports = BTreeSet::new();

    for (_, ty) in roots {
        ty.for_each_ref(|r| {
            imports.insert(r);
        });
    }
    for skip in dont_import {
        imports.remove(skip);
    }

    print_imports(module, out, imports);
}
