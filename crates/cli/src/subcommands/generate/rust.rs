use super::code_indenter::{CodeIndenter, Indenter};
use super::util::{collect_case, iter_reducers, print_lines, type_ref_name};
use super::Lang;
use crate::detect::{has_rust_fmt, has_rust_up};
use crate::generate::util::{iter_tables, iter_types, iter_unique_cols, print_auto_generated_file_comment};
use anyhow::Context;
use convert_case::{Case, Casing};
use duct::cmd;
use spacetimedb_lib::sats::AlgebraicTypeRef;
use spacetimedb_schema::def::{ModuleDef, ReducerDef, ScopedTypeName, TableDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::schema::{Schema, TableSchema};
use spacetimedb_schema::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse, PrimitiveType};
use std::collections::BTreeSet;
use std::fmt::{self, Write};
use std::ops::Deref;
use std::path::PathBuf;

/// Pairs of (module_name, TypeName).
type Imports = BTreeSet<AlgebraicTypeRef>;

const INDENT: &str = "    ";

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

    fn format_files(&self, generated_files: BTreeSet<PathBuf>) -> anyhow::Result<()> {
        if !has_rust_fmt() {
            if has_rust_up() {
                cmd!("rustup", "component", "add", "rustfmt")
                    .run()
                    .context("Failed to install rustfmt with Rustup")?;
            } else {
                anyhow::bail!("rustfmt is not installed. Please install it.");
            }
        }
        cmd!("rustfmt", "--edition", "2021")
            .before_spawn(move |cmd| {
                cmd.args(&generated_files);
                Ok(())
            })
            .run()?;
        Ok(())
    }

    fn generate_type(&self, module: &ModuleDef, typ: &TypeDef) -> String {
        let type_name = collect_case(Case::Pascal, typ.name.name_segments());

        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out);
        out.newline();

        match &module.typespace_for_generate()[typ.ty] {
            AlgebraicTypeDef::Product(product) => {
                gen_and_print_imports(module, out, &product.elements, &[typ.ty]);
                out.newline();
                define_struct_for_product(module, out, &type_name, &product.elements, "pub");
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
impl __sdk::InModule for {type_name} {{
    type Module = super::RemoteModule;
}}
",
        );

        output.into_inner()
    }
    fn generate_table(&self, module: &ModuleDef, table: &TableDef) -> String {
        let schema = TableSchema::from_module_def(module, table, (), 0.into())
            .validated()
            .expect("Failed to generate table due to validation errors");

        let type_ref = table.product_type_ref;

        let mut output = CodeIndenter::new(String::new(), INDENT);
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
        let accessor_trait = table_access_trait_name(&table.name);
        let accessor_method = table_method_name(&table.name);

        write!(
            out,
            "
/// Table handle for the table `{table_name}`.
///
/// Obtain a handle from the [`{accessor_trait}::{accessor_method}`] method on [`super::RemoteTables`],
/// like `ctx.db.{accessor_method}()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.{accessor_method}().on_insert(...)`.
pub struct {table_handle}<'ctx> {{
    imp: __sdk::TableHandle<{row_type}>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `{table_name}`.
///
/// Implemented for [`super::RemoteTables`].
pub trait {accessor_trait} {{
    #[allow(non_snake_case)]
    /// Obtain a [`{table_handle}`], which mediates access to the table `{table_name}`.
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

pub struct {insert_callback_id}(__sdk::CallbackId);
pub struct {delete_callback_id}(__sdk::CallbackId);

impl<'ctx> __sdk::Table for {table_handle}<'ctx> {{
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

        out.delimited_block(
            "
#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {
",
            |out| {
                writeln!(out, "let _table = client_cache.get_or_make_table::<{row_type}>({table_name:?});");
                for (unique_field_ident, unique_field_type_use) in iter_unique_cols(&schema, product_def) {
                    let unique_field_name = unique_field_ident.deref().to_case(Case::Snake);
                    let unique_field_type = type_name(module, unique_field_type_use);
                    writeln!(
                        out,
                        "_table.add_unique_constraint::<{unique_field_type}>({unique_field_name:?}, |row| &row.{unique_field_name});",
                    );
                }
            },
            "}",
        );

        if schema.pk().is_some() {
            let update_callback_id = table_name_pascalcase.clone() + "UpdateCallbackId";
            write!(
                out,
                "
pub struct {update_callback_id}(__sdk::CallbackId);

impl<'ctx> __sdk::TableWithPrimaryKey for {table_handle}<'ctx> {{
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
"
            );
        }

        out.newline();

        write!(
            out,
            "
#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __sdk::Result<__sdk::TableUpdate<{row_type}>> {{
    __sdk::TableUpdate::parse_table_update(raw_updates).map_err(|e| {{
        __sdk::InternalError::failed_parse(
            \"TableUpdate<{row_type}>\",
            \"TableUpdate\",
        ).with_cause(e).into()
    }})
}}
"
        );

        for (unique_field_ident, unique_field_type_use) in iter_unique_cols(&schema, product_def) {
            let unique_field_name = unique_field_ident.deref().to_case(Case::Snake);
            let unique_field_name_pascalcase = unique_field_name.to_case(Case::Pascal);

            let unique_constraint = table_name_pascalcase.clone() + &unique_field_name_pascalcase + "Unique";
            let unique_field_type = type_name(module, unique_field_type_use);

            write!(
                out,
                "
        /// Access to the `{unique_field_name}` unique index on the table `{table_name}`,
        /// which allows point queries on the field of the same name
        /// via the [`{unique_constraint}::find`] method.
        ///
        /// Users are encouraged not to explicitly reference this type,
        /// but to directly chain method calls,
        /// like `ctx.db.{accessor_method}().{unique_field_name}().find(...)`.
        pub struct {unique_constraint}<'ctx> {{
            imp: __sdk::UniqueConstraintHandle<{row_type}, {unique_field_type}>,
            phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
        }}

        impl<'ctx> {table_handle}<'ctx> {{
            /// Get a handle on the `{unique_field_name}` unique index on the table `{table_name}`.
            pub fn {unique_field_name}(&self) -> {unique_constraint}<'ctx> {{
                {unique_constraint} {{
                    imp: self.imp.get_unique_constraint::<{unique_field_type}>({unique_field_name:?}),
                    phantom: std::marker::PhantomData,
                }}
            }}
        }}

        impl<'ctx> {unique_constraint}<'ctx> {{
            /// Find the subscribed row whose `{unique_field_name}` column value is equal to `col_val`,
            /// if such a row is present in the client cache.
            pub fn find(&self, col_val: &{unique_field_type}) -> Option<{row_type}> {{
                self.imp.find(col_val)
            }}
        }}
        "
            );
        }

        // TODO: expose non-unique indices.

        output.into_inner()
    }
    fn generate_reducer(&self, module: &ModuleDef, reducer: &ReducerDef) -> String {
        let mut output = CodeIndenter::new(String::new(), INDENT);
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
        let set_reducer_flags_trait = reducer_flags_trait_name(reducer);
        let args_type = reducer_args_type_name(&reducer.name);
        let enum_variant_name = reducer_variant_name(&reducer.name);

        // Define an "args struct" for the reducer.
        // This is not user-facing (note the `pub(super)` visibility);
        // it is an internal helper for serialization and deserialization.
        // We actually want to ser/de instances of `enum Reducer`, but:
        // - `Reducer` will have struct-like variants, which SATS ser/de does not support.
        // - The WS format does not contain a BSATN-serialized `Reducer` instance;
        //   it holds the reducer name or ID separately from the argument bytes.
        //   We could work up some magic with `DeserializeSeed`
        //   and/or custom `Serializer` and `Deserializer` types
        //   to account for this, but it's much easier to just use an intermediate struct per reducer.
        define_struct_for_product(
            module,
            out,
            &args_type,
            &reducer.params_for_generate.elements,
            "pub(super)",
        );

        out.newline();

        let callback_id = reducer_callback_id_name(&reducer.name);

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
        for (arg_ident, arg_ty) in &reducer.params_for_generate.elements[..] {
            arg_types_ref_list += "&";
            write_type(module, &mut arg_types_ref_list, arg_ty).unwrap();
            arg_types_ref_list += ", ";

            let arg_name = arg_ident.deref().to_case(Case::Snake);
            arg_names_list += &arg_name;
            arg_names_list += ", ";
        }

        write!(out, "impl From<{args_type}> for super::Reducer ");
        out.delimited_block(
            "{",
            |out| {
                write!(out, "fn from(args: {args_type}) -> Self ");
                out.delimited_block(
                    "{",
                    |out| {
                        write!(out, "Self::{enum_variant_name}");
                        if !reducer.params_for_generate.elements.is_empty() {
                            // We generate "struct variants" for reducers with arguments,
                            // but "unit variants" for reducers of no arguments.
                            // These use different constructor syntax.
                            out.delimited_block(
                                " {",
                                |out| {
                                    for (arg_ident, _ty) in &reducer.params_for_generate.elements[..] {
                                        let arg_name = arg_ident.deref().to_case(Case::Snake);
                                        writeln!(out, "{arg_name}: args.{arg_name},");
                                    }
                                },
                                "}",
                            );
                        }
                        out.newline();
                    },
                    "}\n",
                );
            },
            "}\n",
        );

        // TODO: check for lifecycle reducers and do not generate the invoke method.

        writeln!(
            out,
            "
impl __sdk::InModule for {args_type} {{
    type Module = super::RemoteModule;
}}

pub struct {callback_id}(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `{reducer_name}`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait {func_name} {{
    /// Request that the remote module invoke the reducer `{reducer_name}` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_{func_name}`] callbacks.
    fn {func_name}(&self, {arglist}) -> __sdk::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `{reducer_name}`.
    ///
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::ReducerEventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`{callback_id}`] can be passed to [`Self::remove_on_{func_name}`]
    /// to cancel the callback.
    fn on_{func_name}(&self, callback: impl FnMut(&super::ReducerEventContext, {arg_types_ref_list}) + Send + 'static) -> {callback_id};
    /// Cancel a callback previously registered by [`Self::on_{func_name}`],
    /// causing it not to run in the future.
    fn remove_on_{func_name}(&self, callback: {callback_id});
}}

impl {func_name} for super::RemoteReducers {{
    fn {func_name}(&self, {arglist}) -> __sdk::Result<()> {{
        self.imp.call_reducer({reducer_name:?}, {args_type} {{ {arg_names_list} }})
    }}
    fn on_{func_name}(
        &self,
        mut callback: impl FnMut(&super::ReducerEventContext, {arg_types_ref_list}) + Send + 'static,
    ) -> {callback_id} {{
        {callback_id}(self.imp.on_reducer(
            {reducer_name:?},
            Box::new(move |ctx: &super::ReducerEventContext| {{
                let super::ReducerEventContext {{
                    event: __sdk::ReducerEvent {{
                        reducer: super::Reducer::{enum_variant_name} {{
                            {arg_names_list}
                        }},
                        ..
                    }},
                    ..
                }} = ctx else {{ unreachable!() }};
                callback(ctx, {arg_names_list})
            }}),
        ))
    }}
    fn remove_on_{func_name}(&self, callback: {callback_id}) {{
        self.imp.remove_on_reducer({reducer_name:?}, callback.0)
    }}
}}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `{reducer_name}`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait {set_reducer_flags_trait} {{
    /// Set the call-reducer flags for the reducer `{reducer_name}` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn {func_name}(&self, flags: __ws::CallReducerFlags);
}}

impl {set_reducer_flags_trait} for super::SetReducerFlags {{
    fn {func_name}(&self, flags: __ws::CallReducerFlags) {{
        self.imp.set_call_reducer_flags({reducer_name:?}, flags);
    }}
}}
"
        );

        output.into_inner()
    }

    fn generate_globals(&self, module: &ModuleDef) -> Vec<(String, String)> {
        let mut output = CodeIndenter::new(String::new(), INDENT);
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

        // Define `AppliedDiff`.
        print_applied_diff_defn(module, out);

        out.newline();

        // Define `RemoteModule`, `DbConnection`, `EventContext`, `RemoteTables`, `RemoteReducers` and `SubscriptionHandle`.
        // Note that these do not change based on the module.
        print_const_db_context_types(out);

        out.newline();

        // Implement `SpacetimeModule` for `RemoteModule`.
        // This includes a method for initializing the tables in the client cache.
        print_impl_spacetime_module(module, out);

        vec![("mod.rs".to_string(), (output.into_inner()))]
    }

    fn clap_value() -> clap::builder::PossibleValue {
        clap::builder::PossibleValue::new("rust").aliases(["rs", "RS"])
    }
}

pub fn write_type<W: Write>(module: &ModuleDef, out: &mut W, ty: &AlgebraicTypeUse) -> fmt::Result {
    match ty {
        AlgebraicTypeUse::Unit => write!(out, "()")?,
        AlgebraicTypeUse::Never => write!(out, "std::convert::Infallible")?,
        AlgebraicTypeUse::Identity => write!(out, "__sdk::Identity")?,
        AlgebraicTypeUse::ConnectionId => write!(out, "__sdk::ConnectionId")?,
        AlgebraicTypeUse::Timestamp => write!(out, "__sdk::Timestamp")?,
        AlgebraicTypeUse::TimeDuration => write!(out, "__sdk::TimeDuration")?,
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
        AlgebraicTypeUse::Ref(r) => {
            write!(out, "{}", type_ref_name(module, *r))?;
        }
    }
    Ok(())
}

pub fn type_name(module: &ModuleDef, ty: &AlgebraicTypeUse) -> String {
    let mut s = String::new();
    write_type(module, &mut s, ty).unwrap();
    s
}

const ALLOW_LINTS: &str = "#![allow(unused, clippy::all)]";

const SPACETIMEDB_IMPORTS: &[&str] = &[
    "use spacetimedb_sdk::__codegen::{",
    "\tself as __sdk,",
    "\t__lib,",
    "\t__sats,",
    "\t__ws,",
    "};",
];

fn print_spacetimedb_imports(output: &mut Indenter) {
    print_lines(output, SPACETIMEDB_IMPORTS);
}

fn print_file_header(output: &mut Indenter) {
    print_auto_generated_file_comment(output);
    writeln!(output, "{ALLOW_LINTS}");
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
    vis: &str,
) {
    print_struct_derives(out);

    write!(out, "{vis} struct {name} ");

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

fn table_access_trait_name(table_name: &Identifier) -> String {
    table_name.deref().to_case(Case::Pascal) + "TableAccess"
}

fn reducer_args_type_name(reducer_name: &Identifier) -> String {
    reducer_name.deref().to_case(Case::Pascal) + "Args"
}

fn reducer_variant_name(reducer_name: &Identifier) -> String {
    reducer_name.deref().to_case(Case::Pascal)
}

fn reducer_callback_id_name(reducer_name: &Identifier) -> String {
    reducer_name.deref().to_case(Case::Pascal) + "CallbackId"
}

fn reducer_module_name(reducer_name: &Identifier) -> String {
    reducer_name.deref().to_case(Case::Snake) + "_reducer"
}

fn reducer_function_name(reducer: &ReducerDef) -> String {
    reducer.name.deref().to_case(Case::Snake)
}

fn reducer_flags_trait_name(reducer: &ReducerDef) -> String {
    format!("set_flags_for_{}", reducer_function_name(reducer))
}

/// Iterate over all of the Rust `mod`s for types, reducers and tables in the `module`.
fn iter_module_names(module: &ModuleDef) -> impl Iterator<Item = String> + '_ {
    itertools::chain!(
        iter_types(module).map(|ty| type_module_name(&ty.name)),
        iter_reducers(module).map(|r| reducer_module_name(&r.name)),
        iter_tables(module).map(|tbl| table_module_name(&tbl.name)),
    )
}

/// Print `pub mod` declarations for all the files that will be generated for `items`.
fn print_module_decls(module: &ModuleDef, out: &mut Indenter) {
    for module_name in iter_module_names(module) {
        writeln!(out, "pub mod {module_name};");
    }
}

/// Print appropriate reexports for all the files that will be generated for `items`.
fn print_module_reexports(module: &ModuleDef, out: &mut Indenter) {
    for ty in iter_types(module) {
        let mod_name = type_module_name(&ty.name);
        let type_name = collect_case(Case::Pascal, ty.name.name_segments());
        writeln!(out, "pub use {mod_name}::{type_name};")
    }
    for table in iter_tables(module) {
        let mod_name = table_module_name(&table.name);
        // TODO: More precise reexport: we want:
        // - The trait name.
        // - The insert, delete and possibly update callback ids.
        // We do not want:
        // - The table handle.
        writeln!(out, "pub use {mod_name}::*;");
    }
    for reducer in iter_reducers(module) {
        let mod_name = reducer_module_name(&reducer.name);
        let reducer_trait_name = reducer_function_name(reducer);
        let flags_trait_name = reducer_flags_trait_name(reducer);
        let callback_id_name = reducer_callback_id_name(&reducer.name);
        writeln!(
            out,
            "pub use {mod_name}::{{{reducer_trait_name}, {flags_trait_name}, {callback_id_name}}};"
        );
    }
}

fn print_reducer_enum_defn(module: &ModuleDef, out: &mut Indenter) {
    // Don't derive ser/de on this enum;
    // it's not a proper SATS enum and the derive will fail.
    writeln!(out, "#[derive(Clone, PartialEq, Debug)]");
    writeln!(
        out,
        "
/// One of the reducers defined by this module.
///
/// Contained within a [`__sdk::ReducerEvent`] in [`EventContext`]s for reducer events
/// to indicate which reducer caused the event.
",
    );
    out.delimited_block(
        "pub enum Reducer {",
        |out| {
            for reducer in iter_reducers(module) {
                write!(out, "{} ", reducer_variant_name(&reducer.name));
                if !reducer.params_for_generate.elements.is_empty() {
                    // If the reducer has any arguments, generate a "struct variant,"
                    // like `Foo { bar: Baz, }`.
                    // If it doesn't, generate a "unit variant" instead,
                    // like `Foo,`.
                    write_struct_type_fields_in_braces(module, out, &reducer.params_for_generate.elements, false);
                }
                writeln!(out, ",");
            }
        },
        "}\n",
    );
    out.newline();
    writeln!(
        out,
        "
impl __sdk::InModule for Reducer {{
    type Module = RemoteModule;
}}
",
    );

    out.delimited_block(
        "impl __sdk::Reducer for Reducer {",
        |out| {
            out.delimited_block(
                "fn reducer_name(&self) -> &'static str {",
                |out| {
                    out.delimited_block(
                        "match self {",
                        |out| {
                            for reducer in iter_reducers(module) {
                                write!(out, "Reducer::{}", reducer_variant_name(&reducer.name));
                                if !reducer.params_for_generate.elements.is_empty() {
                                    // Because we're emitting unit variants when the payload is empty,
                                    // we will emit different patterns for empty vs non-empty variants.
                                    // This is not strictly required;
                                    // Rust allows matching a struct-like pattern
                                    // against a unit-like enum variant,
                                    // but we prefer the clarity of not including the braces for unit variants.
                                    write!(out, " {{ .. }}");
                                }
                                writeln!(out, " => {:?},", reducer.name.deref());
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
        "impl TryFrom<__ws::ReducerCallInfo<__ws::BsatnFormat>> for Reducer {",
        |out| {
            writeln!(out, "type Error = __sdk::Error;");
            // We define an "args struct" for each reducer in `generate_reducer`.
            // This is not user-facing, and is not exported past the "root" `mod.rs`;
            // it is an internal helper for serialization and deserialization.
            // We actually want to ser/de instances of `enum Reducer`, but:
            //
            // - `Reducer` will have struct-like variants, which SATS ser/de does not support.
            // - The WS format does not contain a BSATN-serialized `Reducer` instance;
            //   it holds the reducer name or ID separately from the argument bytes.
            //   We could work up some magic with `DeserializeSeed`
            //   and/or custom `Serializer` and `Deserializer` types
            //   to account for this, but it's much easier to just use an intermediate struct per reducer.
            //
            // As such, we deserialize from the `value.args` bytes into that "args struct,"
            // then convert it into a `Reducer` variant via `Into::into`,
            // which we also implement in `generate_reducer`.
            out.delimited_block(
                "fn try_from(value: __ws::ReducerCallInfo<__ws::BsatnFormat>) -> __sdk::Result<Self> {",
                |out| {
                    out.delimited_block(
                        "match &value.reducer_name[..] {",
                        |out| {
                            for reducer in iter_reducers(module) {
                                writeln!(
                                    out,
                                    "{:?} => Ok(__sdk::parse_reducer_args::<{}::{}>({:?}, &value.args)?.into()),",
                                    reducer.name.deref(),
                                    reducer_module_name(&reducer.name),
                                    reducer_args_type_name(&reducer.name),
                                    reducer.name.deref(),
                                );
                            }
                            writeln!(
                                out,
                                "unknown => Err(__sdk::InternalError::unknown_name(\"reducer\", unknown, \"ReducerCallInfo\").into()),",
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
    writeln!(out, "#[doc(hidden)]");
    out.delimited_block(
        "pub struct DbUpdate {",
        |out| {
            for table in iter_tables(module) {
                writeln!(
                    out,
                    "{}: __sdk::TableUpdate<{}>,",
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
impl TryFrom<__ws::DatabaseUpdate<__ws::BsatnFormat>> for DbUpdate {
    type Error = __sdk::Error;
    fn try_from(raw: __ws::DatabaseUpdate<__ws::BsatnFormat>) -> Result<Self, Self::Error> {
        let mut db_update = DbUpdate::default();
        for table_update in raw.tables {
            match &table_update.table_name[..] {
",
        |out| {
            for table in iter_tables(module) {
                writeln!(
                    out,
                    "{:?} => db_update.{} = {}::parse_table_update(table_update)?,",
                    table.name.deref(),
                    table_method_name(&table.name),
                    table_module_name(&table.name),
                );
            }
        },
        "
                unknown => {
                    return Err(__sdk::InternalError::unknown_name(
                        \"table\",
                        unknown,
                        \"DatabaseUpdate\",
                    ).into());
                }
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
impl __sdk::InModule for DbUpdate {{
    type Module = RemoteModule;
}}
",
    );

    out.delimited_block(
        "impl __sdk::DbUpdate for DbUpdate {",
        |out| {
            out.delimited_block(
                "fn apply_to_client_cache(&self, cache: &mut __sdk::ClientCache<RemoteModule>) -> AppliedDiff<'_> {
                    let mut diff = AppliedDiff::default();
                ",
                |out| {
                    for table in iter_tables(module) {
                        let with_updates = table
                            .primary_key
                            .map(|col| {
                                let pk_field = table.get_column(col).unwrap().name.deref().to_case(Case::Snake);
                                format!(".with_updates_by_pk(|row| &row.{pk_field})")
                            })
                            .unwrap_or_default();

                        let field_name = table_method_name(&table.name);
                        writeln!(
                            out,
                            "diff.{field_name} = cache.apply_diff_to_table::<{}>({:?}, &self.{field_name}){with_updates};",
                            type_ref_name(module, table.product_type_ref),
                            table.name.deref(),
                        );
                    }
                },
                "
                    diff
                }\n",
            );
        },
        "}\n",
    );
}

fn print_applied_diff_defn(module: &ModuleDef, out: &mut Indenter) {
    writeln!(out, "#[derive(Default)]");
    writeln!(out, "#[allow(non_snake_case)]");
    writeln!(out, "#[doc(hidden)]");
    out.delimited_block(
        "pub struct AppliedDiff<'r> {",
        |out| {
            for table in iter_tables(module) {
                writeln!(
                    out,
                    "{}: __sdk::TableAppliedDiff<'r, {}>,",
                    table_method_name(&table.name),
                    type_ref_name(module, table.product_type_ref),
                );
            }
        },
        "}\n",
    );

    out.newline();

    writeln!(
        out,
        "
impl __sdk::InModule for AppliedDiff<'_> {{
    type Module = RemoteModule;
}}
",
    );

    out.delimited_block(
        "impl<'r> __sdk::AppliedDiff<'r> for AppliedDiff<'r> {",
        |out| {
            out.delimited_block(
                "fn invoke_row_callbacks(&self, event: &EventContext, callbacks: &mut __sdk::DbCallbacks<RemoteModule>) {",
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

fn print_impl_spacetime_module(module: &ModuleDef, out: &mut Indenter) {
    out.delimited_block(
        "impl __sdk::SpacetimeModule for RemoteModule {",
        |out| {
            writeln!(
                out,
                "
type DbConnection = DbConnection;
type EventContext = EventContext;
type ReducerEventContext = ReducerEventContext;
type SubscriptionEventContext = SubscriptionEventContext;
type ErrorContext = ErrorContext;
type Reducer = Reducer;
type DbView = RemoteTables;
type Reducers = RemoteReducers;
type SetReducerFlags = SetReducerFlags;
type DbUpdate = DbUpdate;
type AppliedDiff<'r> = AppliedDiff<'r>;
type SubscriptionHandle = SubscriptionHandle;
"
            );
            out.delimited_block(
                "fn register_tables(client_cache: &mut __sdk::ClientCache<Self>) {",
                |out| {
                    for table in iter_tables(module) {
                        writeln!(out, "{}::register_table(client_cache);", table_module_name(&table.name));
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
#[doc(hidden)]
pub struct RemoteModule;

impl __sdk::InModule for RemoteModule {{
    type Module = Self;
}}

/// The `reducers` field of [`EventContext`] and [`DbConnection`],
/// with methods provided by extension traits for each reducer defined by the module.
pub struct RemoteReducers {{
    imp: __sdk::DbContextImpl<RemoteModule>,
}}

impl __sdk::InModule for RemoteReducers {{
    type Module = RemoteModule;
}}

#[doc(hidden)]
/// The `set_reducer_flags` field of [`DbConnection`],
/// with methods provided by extension traits for each reducer defined by the module.
/// Each method sets the flags for the reducer with the same name.
///
/// This type is currently unstable and may be removed without a major version bump.
pub struct SetReducerFlags {{
    imp: __sdk::DbContextImpl<RemoteModule>,
}}

impl __sdk::InModule for SetReducerFlags {{
    type Module = RemoteModule;
}}

/// The `db` field of [`EventContext`] and [`DbConnection`],
/// with methods provided by extension traits for each table defined by the module.
pub struct RemoteTables {{
    imp: __sdk::DbContextImpl<RemoteModule>,
}}

impl __sdk::InModule for RemoteTables {{
    type Module = RemoteModule;
}}

/// A connection to a remote module, including a materialized view of a subset of the database.
///
/// Connect to a remote module by calling [`DbConnection::builder`]
/// and using the [`__sdk::DbConnectionBuilder`] builder-pattern constructor.
///
/// You must explicitly advance the connection by calling any one of:
///
/// - [`DbConnection::frame_tick`].
/// - [`DbConnection::run_threaded`].
/// - [`DbConnection::run_async`].
/// - [`DbConnection::advance_one_message`].
/// - [`DbConnection::advance_one_message_blocking`].
/// - [`DbConnection::advance_one_message_async`].
///
/// Which of these methods you should call depends on the specific needs of your application,
/// but you must call one of them, or else the connection will never progress.
pub struct DbConnection {{
    /// Access to tables defined by the module via extension traits implemented for [`RemoteTables`].
    pub db: RemoteTables,
    /// Access to reducers defined by the module via extension traits implemented for [`RemoteReducers`].
    pub reducers: RemoteReducers,
    #[doc(hidden)]
    /// Access to setting the call-flags of each reducer defined for each reducer defined by the module
    /// via extension traits implemented for [`SetReducerFlags`].
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    pub set_reducer_flags: SetReducerFlags,

    imp: __sdk::DbContextImpl<RemoteModule>,
}}

impl __sdk::InModule for DbConnection {{
    type Module = RemoteModule;
}}

impl __sdk::DbContext for DbConnection {{
    type DbView = RemoteTables;
    type Reducers = RemoteReducers;
    type SetReducerFlags = SetReducerFlags;

    fn db(&self) -> &Self::DbView {{
        &self.db
    }}
    fn reducers(&self) -> &Self::Reducers {{
        &self.reducers
    }}
    fn set_reducer_flags(&self) -> &Self::SetReducerFlags {{
        &self.set_reducer_flags
    }}

    fn is_active(&self) -> bool {{
        self.imp.is_active()
    }}

    fn disconnect(&self) -> __sdk::Result<()> {{
        self.imp.disconnect()
    }}

    type SubscriptionBuilder = __sdk::SubscriptionBuilder<RemoteModule>;

    fn subscription_builder(&self) -> Self::SubscriptionBuilder {{
        __sdk::SubscriptionBuilder::new(&self.imp)
    }}

    fn try_identity(&self) -> Option<__sdk::Identity> {{
        self.imp.try_identity()
    }}
    fn connection_id(&self) -> __sdk::ConnectionId {{
        self.imp.connection_id()
    }}
}}

impl DbConnection {{
    /// Builder-pattern constructor for a connection to a remote module.
    ///
    /// See [`__sdk::DbConnectionBuilder`] for required and optional configuration for the new connection.
    pub fn builder() -> __sdk::DbConnectionBuilder<RemoteModule> {{
        __sdk::DbConnectionBuilder::new()
    }}

    /// If any WebSocket messages are waiting, process one of them.
    ///
    /// Returns `true` if a message was processed, or `false` if the queue is empty.
    /// Callers should invoke this message in a loop until it returns `false`
    /// or for as much time is available to process messages.
    ///
    /// Returns an error if the connection is disconnected.
    /// If the disconnection in question was normal,
    ///  i.e. the result of a call to [`__sdk::DbContext::disconnect`],
    /// the returned error will be downcastable to [`__sdk::DisconnectedError`].
    ///
    /// This is a low-level primitive exposed for power users who need significant control over scheduling.
    /// Most applications should call [`Self::frame_tick`] each frame
    /// to fully exhaust the queue whenever time is available.
    pub fn advance_one_message(&self) -> __sdk::Result<bool> {{
        self.imp.advance_one_message()
    }}

    /// Process one WebSocket message, potentially blocking the current thread until one is received.
    ///
    /// Returns an error if the connection is disconnected.
    /// If the disconnection in question was normal,
    ///  i.e. the result of a call to [`__sdk::DbContext::disconnect`],
    /// the returned error will be downcastable to [`__sdk::DisconnectedError`].
    ///
    /// This is a low-level primitive exposed for power users who need significant control over scheduling.
    /// Most applications should call [`Self::run_threaded`] to spawn a thread
    /// which advances the connection automatically.
    pub fn advance_one_message_blocking(&self) -> __sdk::Result<()> {{
        self.imp.advance_one_message_blocking()
    }}

    /// Process one WebSocket message, `await`ing until one is received.
    ///
    /// Returns an error if the connection is disconnected.
    /// If the disconnection in question was normal,
    ///  i.e. the result of a call to [`__sdk::DbContext::disconnect`],
    /// the returned error will be downcastable to [`__sdk::DisconnectedError`].
    ///
    /// This is a low-level primitive exposed for power users who need significant control over scheduling.
    /// Most applications should call [`Self::run_async`] to run an `async` loop
    /// which advances the connection when polled.
    pub async fn advance_one_message_async(&self) -> __sdk::Result<()> {{
        self.imp.advance_one_message_async().await
    }}

    /// Process all WebSocket messages waiting in the queue,
    /// then return without `await`ing or blocking the current thread.
    pub fn frame_tick(&self) -> __sdk::Result<()> {{
        self.imp.frame_tick()
    }}

    /// Spawn a thread which processes WebSocket messages as they are received.
    pub fn run_threaded(&self) -> std::thread::JoinHandle<()> {{
        self.imp.run_threaded()
    }}

    /// Run an `async` loop which processes WebSocket messages when polled.
    pub async fn run_async(&self) -> __sdk::Result<()> {{
        self.imp.run_async().await
    }}
}}

impl __sdk::DbConnection for DbConnection {{
    fn new(imp: __sdk::DbContextImpl<RemoteModule>) -> Self {{
        Self {{
            db: RemoteTables {{ imp: imp.clone() }},
            reducers: RemoteReducers {{ imp: imp.clone() }},
            set_reducer_flags: SetReducerFlags {{ imp: imp.clone() }},
            imp,
        }}
    }}
}}

/// A handle on a subscribed query.
// TODO: Document this better after implementing the new subscription API.
#[derive(Clone)]
pub struct SubscriptionHandle {{
    imp: __sdk::SubscriptionHandleImpl<RemoteModule>,
}}

impl __sdk::InModule for SubscriptionHandle {{
    type Module = RemoteModule;
}}

impl __sdk::SubscriptionHandle for SubscriptionHandle {{
    fn new(imp: __sdk::SubscriptionHandleImpl<RemoteModule>) -> Self {{
        Self {{ imp }}
    }}

    /// Returns true if this subscription has been terminated due to an unsubscribe call or an error.
    fn is_ended(&self) -> bool {{
        self.imp.is_ended()
    }}

    /// Returns true if this subscription has been applied and has not yet been unsubscribed.
    fn is_active(&self) -> bool {{
        self.imp.is_active()
    }}

    /// Unsubscribe from the query controlled by this `SubscriptionHandle`,
    /// then run `on_end` when its rows are removed from the client cache.
    fn unsubscribe_then(self, on_end: __sdk::OnEndedCallback<RemoteModule>) -> __sdk::Result<()> {{
        self.imp.unsubscribe_then(Some(on_end))
    }}

    fn unsubscribe(self) -> __sdk::Result<()> {{
        self.imp.unsubscribe_then(None)
    }}

}}

/// Alias trait for a [`__sdk::DbContext`] connected to this module,
/// with that trait's associated types bounded to this module's concrete types.
///
/// Users can use this trait as a boundary on definitions which should accept
/// either a [`DbConnection`] or an [`EventContext`] and operate on either.
pub trait RemoteDbContext: __sdk::DbContext<
    DbView = RemoteTables,
    Reducers = RemoteReducers,
    SetReducerFlags = SetReducerFlags,
    SubscriptionBuilder = __sdk::SubscriptionBuilder<RemoteModule>,
> {{}}
impl<Ctx: __sdk::DbContext<
    DbView = RemoteTables,
    Reducers = RemoteReducers,
    SetReducerFlags = SetReducerFlags,
    SubscriptionBuilder = __sdk::SubscriptionBuilder<RemoteModule>,
>> RemoteDbContext for Ctx {{}}
",
    );

    define_event_context(
        out,
        "EventContext",
        Some("__sdk::Event<Reducer>"),
        "[`__sdk::Table::on_insert`], [`__sdk::Table::on_delete`] and [`__sdk::TableWithPrimaryKey::on_update`] callbacks",
        Some("[`__sdk::Event`]"),
    );

    define_event_context(
        out,
        "ReducerEventContext",
        Some("__sdk::ReducerEvent<Reducer>"),
        "on-reducer callbacks", // There's no single trait or method for reducer callbacks, so we can't usefully link to them.
        Some("[`__sdk::ReducerEvent`]"),
    );

    define_event_context(
        out,
        "SubscriptionEventContext",
        None, // SubscriptionEventContexts have no additional `event` info, so they don't even get that field.
        "[`__sdk::SubscriptionBuilder::on_applied`] and [`SubscriptionHandle::unsubscribe_then`] callbacks",
        None,
    );

    define_event_context(
        out,
        "ErrorContext",
        Some("Option<__sdk::Error>"),
        "[`__sdk::DbConnectionBuilder::on_disconnect`], [`__sdk::DbConnectionBuilder::on_connect_error`] and [`__sdk::SubscriptionBuilder::on_error`] callbacks",
        Some("[`__sdk::Error`]"),
    );
}

/// Define a type that implements `AbstractEventContext` and one of its concrete subtraits.
///
/// `struct_and_trait_name` should be the name of an event context trait,
/// and will also be used as the new struct's name.
///
/// `event_type`, if `Some`, should be a Rust type which will be the type of the new struct's `event` field.
/// If `None`, the new struct will not have such a field.
/// The `SubscriptionEventContext` will pass `None`, since there is no useful information to add.
///
/// `passed_to_callbacks_doc_link` should be a rustdoc-formatted phrase
/// which links to the callback-registering functions for the callbacks which accept this event context type.
/// It should be of the form "foo callbacks" or "foo, bar and baz callbacks",
/// with link formatting where appropriate, and no trailing punctuation.
///
/// If `event_type` is `Some`, `event_type_doc_link` should be as well.
/// It should be a rustdoc-formatted link (including square brackets and all) to the `event_type`.
/// This may differ (in the `strcmp` sense) from `event_type` because it should not inlcude generic parameters.
fn define_event_context(
    out: &mut Indenter,
    struct_and_trait_name: &str,
    event_type: Option<&str>,
    passed_to_callbacks_doc_link: &str,
    event_type_doc_link: Option<&str>,
) {
    if let (Some(event_type), Some(event_type_doc_link)) = (event_type, event_type_doc_link) {
        write!(
            out,
            "
/// An [`__sdk::DbContext`] augmented with a {event_type_doc_link},
/// passed to {passed_to_callbacks_doc_link}.
pub struct {struct_and_trait_name} {{
    /// Access to tables defined by the module via extension traits implemented for [`RemoteTables`].
    pub db: RemoteTables,
    /// Access to reducers defined by the module via extension traits implemented for [`RemoteReducers`].
    pub reducers: RemoteReducers,
    /// Access to setting the call-flags of each reducer defined for each reducer defined by the module
    /// via extension traits implemented for [`SetReducerFlags`].
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    pub set_reducer_flags: SetReducerFlags,
    /// The event which caused these callbacks to run.
    pub event: {event_type},
    imp: __sdk::DbContextImpl<RemoteModule>,
}}

impl __sdk::AbstractEventContext for {struct_and_trait_name} {{
    type Event = {event_type};
    fn event(&self) -> &Self::Event {{
        &self.event
    }}
    fn new(imp: __sdk::DbContextImpl<RemoteModule>, event: Self::Event) -> Self {{
        Self {{
            db: RemoteTables {{ imp: imp.clone() }},
            reducers: RemoteReducers {{ imp: imp.clone() }},
            set_reducer_flags: SetReducerFlags {{ imp: imp.clone() }},
            event,
            imp,
        }}
    }}
}}
",
        );
    } else {
        debug_assert!(event_type.is_none() && event_type_doc_link.is_none());
        write!(
            out,
            "
/// An [`__sdk::DbContext`] passed to {passed_to_callbacks_doc_link}.
pub struct {struct_and_trait_name} {{
    /// Access to tables defined by the module via extension traits implemented for [`RemoteTables`].
    pub db: RemoteTables,
    /// Access to reducers defined by the module via extension traits implemented for [`RemoteReducers`].
    pub reducers: RemoteReducers,
    /// Access to setting the call-flags of each reducer defined for each reducer defined by the module
    /// via extension traits implemented for [`SetReducerFlags`].
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    pub set_reducer_flags: SetReducerFlags,
    imp: __sdk::DbContextImpl<RemoteModule>,
}}

impl __sdk::AbstractEventContext for {struct_and_trait_name} {{
    type Event = ();
    fn event(&self) -> &Self::Event {{
        &()
    }}
    fn new(imp: __sdk::DbContextImpl<RemoteModule>, _event: Self::Event) -> Self {{
        Self {{
            db: RemoteTables {{ imp: imp.clone() }},
            reducers: RemoteReducers {{ imp: imp.clone() }},
            set_reducer_flags: SetReducerFlags {{ imp: imp.clone() }},
            imp,
        }}
    }}
}}
",
        );
    }

    write!(
        out,
        "
impl __sdk::InModule for {struct_and_trait_name} {{
    type Module = RemoteModule;
}}

impl __sdk::DbContext for {struct_and_trait_name} {{
    type DbView = RemoteTables;
    type Reducers = RemoteReducers;
    type SetReducerFlags = SetReducerFlags;

    fn db(&self) -> &Self::DbView {{
        &self.db
    }}
    fn reducers(&self) -> &Self::Reducers {{
        &self.reducers
    }}
    fn set_reducer_flags(&self) -> &Self::SetReducerFlags {{
        &self.set_reducer_flags
    }}

    fn is_active(&self) -> bool {{
        self.imp.is_active()
    }}

    fn disconnect(&self) -> __sdk::Result<()> {{
        self.imp.disconnect()
    }}

    type SubscriptionBuilder = __sdk::SubscriptionBuilder<RemoteModule>;

    fn subscription_builder(&self) -> Self::SubscriptionBuilder {{
        __sdk::SubscriptionBuilder::new(&self.imp)
    }}

    fn try_identity(&self) -> Option<__sdk::Identity> {{
        self.imp.try_identity()
    }}
    fn connection_id(&self) -> __sdk::ConnectionId {{
        self.imp.connection_id()
    }}
}}

impl __sdk::{struct_and_trait_name} for {struct_and_trait_name} {{}}
"
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
