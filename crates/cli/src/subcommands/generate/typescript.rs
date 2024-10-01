use crate::generate::util::namespace_is_empty_or_default;
use crate::indent_scope;

use super::util::{collect_case, print_auto_generated_file_comment, type_ref_name};

use std::collections::BTreeSet;
use std::fmt::{self, Write};
use std::ops::Deref;

use convert_case::{Case, Casing};
use itertools::Itertools;
use spacetimedb_lib::sats::AlgebraicTypeRef;
use spacetimedb_primitives::ColList;
use spacetimedb_schema::def::{ModuleDef, ReducerDef, ScopedTypeName, TableDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::schema::{Schema, TableSchema};
use spacetimedb_schema::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse, PrimitiveType};

use super::code_indenter::{CodeIndenter, Indenter};
use super::Lang;

type Imports = BTreeSet<AlgebraicTypeRef>;

pub struct TypeScript;

impl Lang for TypeScript {
    fn table_filename(
        &self,
        _module: &spacetimedb_schema::def::ModuleDef,
        table: &spacetimedb_schema::def::TableDef,
    ) -> String {
        table_module_name(&table.name) + ".ts"
    }

    fn type_filename(&self, type_name: &ScopedTypeName) -> String {
        type_module_name(type_name) + ".ts"
    }

    fn reducer_filename(&self, reducer_name: &Identifier) -> String {
        reducer_module_name(reducer_name) + ".ts"
    }

    fn generate_type(&self, module: &ModuleDef, namespace: &str, typ: &TypeDef) -> String {
        // TODO(cloutiertyler): I do think TypeScript does support namespaces:
        // https://www.typescriptlang.org/docs/handbook/namespaces.html
        assert!(
            namespace_is_empty_or_default(namespace),
            "TypeScript codegen does not support namespaces, as TypeScript equates namespaces with files.

Requested namespace: {namespace}",
        );
        let type_name = collect_case(Case::Pascal, typ.name.name_segments());

        let mut output = CodeIndenter::new(String::new());
        let out = &mut output;

        print_file_header(out);

        match &module.typespace_for_generate()[typ.ty] {
            AlgebraicTypeDef::Product(product) => {
                gen_and_print_imports(module, out, &product.elements, &[typ.ty]);
                out.newline();
                define_namespace_and_object_type_for_product(module, out, &type_name, &product.elements);
            }
            AlgebraicTypeDef::Sum(sum) => {
                gen_and_print_imports(module, out, &sum.variants, &[typ.ty]);
                out.newline();
                define_namespace_and_types_for_sum(module, out, &type_name, &sum.variants);
            }
            AlgebraicTypeDef::PlainEnum(plain_enum) => {
                let variants = plain_enum
                    .variants
                    .iter()
                    .cloned()
                    .map(|var| (var, AlgebraicTypeUse::Unit))
                    .collect::<Vec<_>>();
                out.newline();
                define_namespace_and_types_for_sum(module, out, &type_name, &variants);
            }
        }
        out.newline();

        output.into_inner()
    }

    fn generate_table(&self, module: &ModuleDef, namespace: &str, table: &TableDef) -> String {
        assert!(
            namespace_is_empty_or_default(namespace),
            "TypeScript codegen does not support namespaces, as TypeScript equates namespaces with files.

Requested namespace: {namespace}",
        );

        let schema = TableSchema::from_module_def(module, table, (), 0.into())
            .validated()
            .expect("Failed to generate table due to validation errors");

        let mut output = CodeIndenter::new(String::new());
        let out = &mut output;

        print_file_header(out);

        let type_ref = table.product_type_ref;
        let row_type = type_ref_name(module, type_ref);
        let row_type_module = type_ref_module_name(module, type_ref);

        writeln!(out, "import {{ {row_type} }} from \"./{row_type_module}\";");

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

        writeln!(out, "import {{ Reducer, RemoteReducers, RemoteTables }} from \".\";");

        let table_name = table.name.deref();
        let table_name_pascalcase = table.name.deref().to_case(Case::Pascal);
        let table_handle = table_name_pascalcase.clone() + "TableHandle";
        let accessor_method = table_method_name(&table.name);

        write!(
            out,
            "
// Table handle for the table `{table_name}`.
//
// Obtain a handle from the [TODO] method on [`super::RemoteTables`],
// like `ctx.db.TODO()`.
//
// Users are encouraged not to explicitly reference this type,
// but to directly chain method calls,
// like `ctx.db.TODO().on_insert(...)`.
export class {table_handle} {{
    tableCache: TableCache<{row_type}>;

    constructor(tableCache: TableCache<{row_type}>) {{
        this.tableCache = tableCache;
    }}

    count(): number {{
        return this.tableCache.count();
    }}

    iter(): Iterable<{row_type}> {{
        return this.tableCache.iter();
    }}
"
        );

        let constraints = schema.backcompat_column_constraints();

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
    // Access to the `{unique_field_name}` unique index on the table `{table_name}`,
    // which allows point queries on the field of the same name
    // via the [`{unique_constraint}::find`] method.
    //
    // Users are encouraged not to explicitly reference this type,
    // but to directly chain method calls,
    // like `ctx.db.{accessor_method}().{unique_field_name}().find(...)`.
    //
    // Get a handle on the `{unique_field_name}` unique index on the table `{table_name}`.
    {unique_field_name} = {{
        /// Find the subscribed row whose `{unique_field_name}` column value is equal to `col_val`,
        /// if such a row is present in the client cache.
        find: (col_val: {unique_field_type}): {row_type} | undefined => {{
            for (let row of this.tableCache.iter()) {{
                if (row.{unique_field_name} === col_val) {{
                    return row;
                }}
            }}
        }}
    }}
"
                );
            }
        }

        writeln!(
            out,
            "
    onInsert = (cb: (ctx: EventContext<RemoteTables, RemoteReducers, Reducer>, row: {row_type}) => void) => {{
        return this.tableCache.onInsert(cb);
    }}

    removeOnInsert = (cb: (ctx: EventContext<RemoteTables, RemoteReducers, Reducer>, row: {row_type}) => void) => {{
        return this.tableCache.removeOnInsert(cb);
    }}

    onDelete = (cb: (ctx: EventContext<RemoteTables, RemoteReducers, Reducer>, row: {row_type}) => void) => {{
        return this.tableCache.onDelete(cb);
    }}

    removeOnDelete = (cb: (ctx: EventContext<RemoteTables, RemoteReducers, Reducer>, row: {row_type}) => void) => {{
        return this.tableCache.removeOnDelete(cb);
    }}
"
        );

        if schema.pk().is_some() {
            write!(
                out,
    "   // Updates are only defined for tables with primary keys.
    onUpdate = (cb: (ctx: EventContext<RemoteTables, RemoteReducers, Reducer>, oldRow: {row_type}, newRow: {row_type}) => void) => {{
         return this.tableCache.onUpdate(cb);
    }}
"
            );
        }

        // TODO: expose non-unique indices.

        writeln!(out, "}}");
        output.into_inner()
    }

    fn generate_reducer(&self, module: &ModuleDef, namespace: &str, reducer: &ReducerDef) -> String {
        assert!(
            namespace_is_empty_or_default(namespace),
            "TypeScript codegen does not support namespaces, as TypeScript equates namespaces with files.

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

        let args_type = reducer_args_type_name(&reducer.name);

        define_namespace_and_object_type_for_product(module, out, &args_type, &reducer.params_for_generate.elements);

        output.into_inner()
    }

    fn generate_globals(&self, module: &ModuleDef, namespace: &str) -> Vec<(String, String)> {
        assert!(
            namespace_is_empty_or_default(namespace),
            "TypeScript codegen does not support namespaces, as TypeScript equates namespaces with files.

Requested namespace: {namespace}",
        );

        let mut output = CodeIndenter::new(String::new());
        let out = &mut output;

        print_file_header(out);

        out.newline();

        writeln!(out, "// Import all reducer arg types");
        for reducer in iter_reducers(module) {
            let reducer_name = &reducer.name;
            let reducer_module_name = reducer_module_name(reducer_name) + ".ts";
            let args_type = reducer_args_type_name(&reducer.name);
            writeln!(out, "import {{ {args_type} }} from \"./{reducer_module_name}\";");
        }

        writeln!(out);
        writeln!(out, "// Import all table handle types");
        for table in iter_tables(module) {
            let table_name = &table.name;
            let table_module_name = table_module_name(table_name) + ".ts";
            let table_name_pascalcase = table.name.deref().to_case(Case::Pascal);
            let table_handle = table_name_pascalcase.clone() + "TableHandle";
            writeln!(out, "import {{ {table_handle} }} from \"./{table_module_name}\";");
        }

        writeln!(out);
        writeln!(out, "// Import all types");
        for ty in iter_types(module) {
            let type_name = collect_case(Case::Pascal, ty.name.name_segments());
            let type_module_name = type_module_name(&ty.name) + ".ts";
            writeln!(out, "import {{ {type_name} }} from \"./{type_module_name}\";");
        }

        out.newline();

        // Define SpacetimeModule
        writeln!(out, "const REMOTE_MODULE = {{");
        out.indent(1);
        writeln!(out, "tables: {{");
        out.indent(1);
        for table in iter_tables(module) {
            let type_ref = table.product_type_ref;
            let row_type = type_ref_name(module, type_ref);
            let schema = TableSchema::from_module_def(module, table, (), 0.into())
                .validated()
                .expect("Failed to generate table due to validation errors");
            writeln!(out, "{}: {{", table.name);
            out.indent(1);
            writeln!(out, "tableName: \"{}\",", table.name);
            writeln!(out, "rowType: {row_type}.getAlgebraicType(),");
            if let Some(pk) = schema.pk() {
                writeln!(out, "primaryKey: \"{}\",", pk.col_name);
            }
            out.dedent(1);
            writeln!(out, "}},");
        }
        out.dedent(1);
        writeln!(out, "}},");
        writeln!(out, "reducers: {{");
        out.indent(1);
        for reducer in iter_reducers(module) {
            writeln!(out, "{}: {{", reducer.name);
            out.indent(1);
            writeln!(out, "reducerName: \"{}\",", reducer.name);
            writeln!(
                out,
                "argsType: {args_type}.getAlgebraicType(),",
                args_type = reducer_args_type_name(&reducer.name)
            );
            out.dedent(1);
            writeln!(out, "}},");
        }
        out.dedent(1);
        writeln!(out, "}},");
        writeln!(
            out,
            "// Constructors which are used by the DBConnectionImpl to
// extract type information from the generated RemoteModule.
eventContextConstructor: (imp: DBConnectionImpl, event: Event<Reducer>) => {{
  return {{
    ...(imp as DBConnection),
    event
  }}
}},
dbViewConstructor: (imp: DBConnectionImpl) => {{
  return new RemoteTables(imp);
}},
reducersConstructor: (imp: DBConnectionImpl) => {{
  return new RemoteReducers(imp);
}}"
        );
        out.dedent(1);
        writeln!(out, "}}");

        // Define `type Reducer` enum.
        writeln!(out);
        print_reducer_enum_defn(module, out);

        out.newline();

        print_remote_reducers(module, out);

        out.newline();

        print_remote_tables(module, out);

        out.newline();

        print_db_connection(module, out);
        // // Define `RemoteModule`, `DbConnection`, `EventContext`, `RemoteTables`, `RemoteReducers` and `SubscriptionHandle`.
        // // Note that these do not change based on the module.
        // print_const_db_context_types(out);

        vec![("index.ts".to_string(), (output.into_inner()))]
    }
}

fn print_remote_reducers(module: &ModuleDef, out: &mut Indenter) {
    writeln!(out, "export class RemoteReducers {{");
    out.indent(1);
    writeln!(out, "constructor(private connection: DBConnectionImpl) {{}}");
    out.newline();

    for reducer in iter_reducers(module) {
        // // The reducer argument types as `ty, ty, ty`,
        // // for use as the params in a `FnMut` closure type.
        // let mut arg_types_ref_list = String::new();

        // // The reducer argument names as `ident, ident, ident`,
        // // for passing to function call and struct literal expressions.
        // let mut arg_names_list = String::new();

        // The reducer argument names as `ident: ty, ident: ty, ident: ty`,
        // for passing to function call and struct literal expressions.
        let mut arg_list = ", ".to_string();

        for (arg_ident, arg_ty) in &reducer.params_for_generate.elements[..] {
            arg_list += "";
            let arg_name = arg_ident.deref().to_case(Case::Camel);
            arg_list += &arg_name;
            arg_list += ": ";
            write_type(module, &mut arg_list, arg_ty, None).unwrap();
            arg_list += ", ";
        }

        // Remove the trailing comma and space.
        if arg_list.is_empty() {
            arg_list = arg_list[2..arg_list.len()].to_string();
        } else {
            arg_list = arg_list[..arg_list.len() - 2].to_string();
        }

        let reducer_name = &reducer.name;
        let reducer_name_pascal = reducer_name.deref().to_case(Case::Pascal);
        let reducer_function_name = reducer_function_name(reducer);
        let args_type = reducer_args_type_name(&reducer.name);
        let reducer_variant = reducer_variant_name(&reducer.name);
        writeln!(out, "{reducer_function_name}(args: {args_type}) {{");
        out.indent(1);
        writeln!(out, "let writer = new BinaryWriter(1024);");
        writeln!(out, "{reducer_variant}.getAlgebraicType().serialize(writer, args);");
        writeln!(out, "let argsBuffer = writer.getBuffer();");
        writeln!(out, "this.connection.callReducer(\"{reducer_name}\", argsBuffer);");
        out.dedent(1);
        writeln!(out, "}}");
        out.newline();

        writeln!(out, "on{reducer_name_pascal}(callback: (ctx: EventContext<RemoteTables, RemoteReducers, Reducer>{arg_list}) => void) {{");
        out.indent(1);
        writeln!(out, "this.connection.onReducer(\"{reducer_name}\", callback);");
        out.dedent(1);
        writeln!(out, "}}");
        out.newline();
    }

    out.dedent(1);
    writeln!(out, "}}");
}

fn print_remote_tables(module: &ModuleDef, out: &mut Indenter) {
    writeln!(out, "export class RemoteTables {{");
    out.indent(1);
    writeln!(out, "constructor(private connection: DBConnectionImpl) {{}}");
    out.newline();

    for table in iter_tables(module) {
        let table_name = table.name.deref();
        let table_name_pascalcase = table.name.deref().to_case(Case::Pascal);
        let table_name_camelcase = table.name.deref().to_case(Case::Camel);
        let table_handle = table_name_pascalcase.clone() + "TableHandle";
        let type_ref = table.product_type_ref;
        let row_type = type_ref_name(module, type_ref);
        writeln!(out, "#{table_name_camelcase} = this.connection.clientCache.getOrCreateTable<{row_type}>(REMOTE_MODULE.tables.{table_name});");
        writeln!(out, "get {table_name_camelcase}(): {table_handle} {{");
        out.indent(1);
        writeln!(out, "return new {table_handle}(this.#{table_name_camelcase});");
        out.dedent(1);
        writeln!(out, "}}");
    }

    out.dedent(1);
    writeln!(out, "}}");
}

fn print_db_connection(_module: &ModuleDef, out: &mut Indenter) {
    writeln!(
        out,
        "export class DBConnection extends DBConnectionImpl<RemoteTables, RemoteReducers>  {{"
    );
    out.indent(1);
    writeln!(out, "static builder = () => {{");
    out.indent(1);
    writeln!(
        out,
        "return new DBConnectionBuilder<DBConnection>(REMOTE_MODULE, (imp: DBConnectionImpl) => imp as DBConnection);"
    );
    out.dedent(1);
    writeln!(out, "}}");
    out.dedent(1);
    writeln!(out, "}}");
}

fn print_reducer_enum_defn(module: &ModuleDef, out: &mut Indenter) {
    writeln!(out, "// A type representing all the possible variants of a reducer.");
    writeln!(out, "export type Reducer = ");
    for reducer in iter_reducers(module) {
        writeln!(
            out,
            "| {{ name: \"{}\", args: {} }}",
            reducer_variant_name(&reducer.name),
            reducer_args_type_name(&reducer.name)
        );
    }
    writeln!(out, ";");
}

fn print_spacetimedb_imports(output: &mut Indenter) {
    writeln!(
        output,
        "import {{
    // @ts-ignore
    AlgebraicType,
    // @ts-ignore
    ProductType,
    // @ts-ignore
    ProductTypeElement,
    // @ts-ignore
    SumType,
    // @ts-ignore
    SumTypeVariant,
    // @ts-ignore
    AlgebraicValue,
    // @ts-ignore
    Identity,
    // @ts-ignore
    Address,
    // @ts-ignore
    DBConnectionBuilder,
    // @ts-ignore
    TableCache,
    // @ts-ignore
    BinaryWriter,
    // @ts-ignore
    EventContext,
    // @ts-ignore
    BinaryReader,
    // @ts-ignore
    DBConnectionImpl,
    // @ts-ignore
    DBContext,
    // @ts-ignore
    Event,
}} from \"@clockworklabs/spacetimedb-sdk\";"
    );
}

fn print_file_header(output: &mut Indenter) {
    print_auto_generated_file_comment(output);
    print_spacetimedb_imports(output);
}

fn write_get_algebraic_type_for_product(
    module: &ModuleDef,
    out: &mut Indenter,
    elements: &[(Identifier, AlgebraicTypeUse)],
) {
    writeln!(
        out,
        "// A function which returns this type represented as an AlgebraicType.
// This function is derived from the AlgebraicType used to generate this type."
    );
    writeln!(out, "export function getAlgebraicType(): AlgebraicType {{");
    {
        out.indent(1);
        write!(out, "return ");
        convert_product_type(module, out, elements, "__");
        writeln!(out, ";");
        out.dedent(1);
    }
    writeln!(out, "}}");
}

fn define_namespace_and_object_type_for_product(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    elements: &[(Identifier, AlgebraicTypeUse)],
) {
    write!(out, "export type {name} = ");

    out.delimited_block(
        "{",
        |out| write_arglist_no_delimiters(module, out, elements, None).unwrap(),
        "}",
    );
    writeln!(out, ";");

    out.newline();

    writeln!(out, "// A namespace for generated helper functions.");
    writeln!(out, "export namespace {name} {{");
    out.indent(1);
    write_get_algebraic_type_for_product(module, out, elements);
    writeln!(out);

    writeln!(
        out,
        "export function serialize(writer: BinaryWriter, value: {name}): void {{
    {name}.getAlgebraicType().serialize(writer, value);
}}"
    );
    writeln!(out);

    writeln!(
        out,
        "export function deserialize(reader: BinaryReader): {name} {{
    return {name}.getAlgebraicType().deserialize(reader);
}}"
    );

    out.dedent(1);
    writeln!(out, "}}");

    out.newline();
}

fn write_arglist_no_delimiters(
    module: &ModuleDef,
    out: &mut impl Write,
    elements: &[(Identifier, AlgebraicTypeUse)],

    prefix: Option<&str>,
) -> anyhow::Result<()> {
    for (ident, ty) in elements {
        if let Some(prefix) = prefix {
            write!(out, "{prefix} ")?;
        }

        let name = ident.deref().to_case(Case::Snake);

        write!(out, "{name}: ")?;
        write_type(module, out, ty, Some("__"))?;
        writeln!(out, ",")?;
    }

    Ok(())
}

fn write_sum_variant_type(module: &ModuleDef, out: &mut Indenter, ident: &Identifier, ty: &AlgebraicTypeUse) {
    let name = ident.deref().to_case(Case::Pascal);
    write!(out, "export type {name} = ");

    // If the contained type is the unit type, i.e. this variant has no members,
    // write only the tag.
    // ```
    // { tag: "Foo" }
    // ```
    write!(out, "{{ ");
    write!(out, "tag: \"{name}\"");

    // If the contained type is not the unit type, write the tag and the value.
    // ```
    // { tag: "Bar", value: Bar }
    // { tag: "Bar", value: number }
    // { tag: "Bar", value: string }
    // ```
    // Note you could alternatively do:
    // ```
    // { tag: "Bar" } & Bar
    // ```
    // for non-primitive types but that doesn't extend to primitives.
    // Another alternative would be to name the value field the same as the tag field, but lowercased
    // ```
    // { tag: "Bar", bar: Bar }
    // { tag: "Bar", bar: number }
    // { tag: "Bar", bar: string }
    // ```
    // but this is a departure from our previous convention and is not much different.
    if !matches!(ty, AlgebraicTypeUse::Unit) {
        write!(out, ", value: ");
        write_type(module, out, ty, Some("__")).unwrap();
    }

    writeln!(out, " }};");
}

fn write_variant_types(module: &ModuleDef, out: &mut Indenter, variants: &[(Identifier, AlgebraicTypeUse)]) {
    // Write all the variant types.
    for (ident, ty) in variants {
        write_sum_variant_type(module, out, ident, ty);
    }
}

fn write_variant_constructors(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    variants: &[(Identifier, AlgebraicTypeUse)],
) {
    // Write all the variant constructors.
    // Write all of the variant constructors.
    for (ident, ty) in variants {
        if matches!(ty, AlgebraicTypeUse::Unit) {
            // If the variant has no members, we can export a simple object.
            // ```
            // export const Foo = { tag: "Foo" };
            // ```
            write!(out, "export const {ident} = ");
            writeln!(out, "{{ tag: \"{ident}\" }};");
            continue;
        }
        let variant_name = ident.deref().to_case(Case::Pascal);
        write!(out, "export const {variant_name} = (value: ");
        write_type(module, out, ty, Some("__")).unwrap();
        writeln!(out, "): {name} => ({{ tag: \"{variant_name}\", value }});");
    }
}

fn write_get_algebraic_type_for_sum(
    module: &ModuleDef,
    out: &mut Indenter,
    variants: &[(Identifier, AlgebraicTypeUse)],
) {
    writeln!(out, "export function getAlgebraicType(): AlgebraicType {{");
    {
        indent_scope!(out);
        write!(out, "return ");
        convert_sum_type(module, &mut out, variants, "__");
        writeln!(out, ";");
    }
    writeln!(out, "}}");
}

fn define_namespace_and_types_for_sum(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    variants: &[(Identifier, AlgebraicTypeUse)],
) {
    writeln!(out, "// A namespace for generated variants and helper functions.");
    writeln!(out, "export namespace {name} {{");
    out.indent(1);

    // Write all of the variant types.
    writeln!(
        out,
        "// These are the generated variant types for each variant of the tagged union.
// One type is generated per variant and will be used in the `value` field of
// the tagged union."
    );
    write_variant_types(module, out, variants);
    writeln!(out);

    // Write all of the variant constructors.
    writeln!(
        out,
        "// Helper functions for constructing each variant of the tagged union.
// ```
// const foo = Foo.A(42);
// assert!(foo.tag === \"A\");
// assert!(foo.value === 42);
// ```"
    );
    write_variant_constructors(module, out, name, variants);
    writeln!(out);

    // Write the function that generates the algebraic type.
    write_get_algebraic_type_for_sum(module, out, variants);
    writeln!(out);

    writeln!(
        out,
        "export function serialize(writer: BinaryWriter, value: {name}): void {{
    {name}.getAlgebraicType().serialize(writer, value);
}}"
    );
    writeln!(out);

    writeln!(
        out,
        "export function deserialize(reader: BinaryReader): {name} {{
    return {name}.getAlgebraicType().deserialize(reader);
}}"
    );
    writeln!(out);

    out.dedent(1);

    writeln!(out, "}}");
    out.newline();

    writeln!(out, "// The tagged union or sum type for the algebraic type `{name}`.");
    write!(out, "export type {name} = ");

    let names = variants
        .iter()
        .map(|(ident, _)| format!("{name}.{}", ident.deref().to_case(Case::Pascal)))
        .collect::<Vec<String>>()
        .join(" | ");

    writeln!(out, "{names};");
    out.newline();

    writeln!(out, "export default {name};");
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
    reducer.name.deref().to_case(Case::Camel)
}

pub fn type_name(module: &ModuleDef, ty: &AlgebraicTypeUse) -> String {
    let mut s = String::new();
    write_type(module, &mut s, ty, None).unwrap();
    s
}

pub fn write_type<W: Write>(
    module: &ModuleDef,
    out: &mut W,
    ty: &AlgebraicTypeUse,
    ref_prefix: Option<&str>,
) -> fmt::Result {
    match ty {
        AlgebraicTypeUse::Unit => write!(out, "void")?,
        AlgebraicTypeUse::Never => write!(out, "never")?,
        AlgebraicTypeUse::Identity => write!(out, "Identity")?,
        AlgebraicTypeUse::Address => write!(out, "Address")?,
        AlgebraicTypeUse::ScheduleAt => write!(
            out,
            "{{ tag: \"Interval\", value: BigInt }} | {{ tag: \"Time\", value: BigInt }}"
        )?,
        AlgebraicTypeUse::Option(inner_ty) => {
            write_type(module, out, inner_ty, ref_prefix)?;
            write!(out, " | undefined")?;
        }
        AlgebraicTypeUse::Primitive(prim) => match prim {
            PrimitiveType::Bool => write!(out, "boolean")?,
            PrimitiveType::I8 => write!(out, "number")?,
            PrimitiveType::U8 => write!(out, "number")?,
            PrimitiveType::I16 => write!(out, "number")?,
            PrimitiveType::U16 => write!(out, "number")?,
            PrimitiveType::I32 => write!(out, "number")?,
            PrimitiveType::U32 => write!(out, "number")?,
            PrimitiveType::I64 => write!(out, "BigInt")?,
            PrimitiveType::U64 => write!(out, "BigInt")?,
            PrimitiveType::I128 => write!(out, "BigInt")?,
            PrimitiveType::U128 => write!(out, "BigInt")?,
            PrimitiveType::I256 => write!(out, "BigInt")?,
            PrimitiveType::U256 => write!(out, "BigInt")?,
            PrimitiveType::F32 => write!(out, "number")?,
            PrimitiveType::F64 => write!(out, "number")?,
        },
        AlgebraicTypeUse::String => write!(out, "string")?,
        AlgebraicTypeUse::Array(elem_ty) => {
            write_type(module, out, elem_ty, ref_prefix)?;
            write!(out, "[]")?;
        }
        AlgebraicTypeUse::Map { .. } => unimplemented!("AlgebraicType::Map is unsupported and will be removed"),
        AlgebraicTypeUse::Ref(r) => {
            if let Some(prefix) = ref_prefix {
                write!(out, "{prefix}")?;
            }
            write!(out, "{}", type_ref_name(module, *r))?;
        }
    }
    Ok(())
}

fn convert_algebraic_type<'a>(
    module: &'a ModuleDef,
    out: &mut Indenter,
    ty: &'a AlgebraicTypeUse,
    ref_prefix: &'a str,
) {
    match ty {
        AlgebraicTypeUse::ScheduleAt => write!(out, "AlgebraicType.createScheduleAtType()"),
        AlgebraicTypeUse::Identity => write!(out, "AlgebraicType.createIdentityType()"),
        AlgebraicTypeUse::Address => write!(out, "AlgebraicType.createAddressType()"),
        AlgebraicTypeUse::Option(inner_ty) => {
            write!(out, "AlgebraicType.createOptionType(");
            convert_algebraic_type(module, out, inner_ty, ref_prefix);
            write!(out, ")");
        }
        AlgebraicTypeUse::Array(ty) => {
            write!(out, "AlgebraicType.createArrayType(");
            convert_algebraic_type(module, out, ty, ref_prefix);
            write!(out, ")");
        }
        AlgebraicTypeUse::Ref(r) => write!(out, "{ref_prefix}{}.getAlgebraicType()", type_ref_name(module, *r)),
        AlgebraicTypeUse::Primitive(prim) => {
            write!(out, "AlgebraicType.create{prim:?}Type()");
        }
        AlgebraicTypeUse::Map { .. } => unimplemented!(),
        AlgebraicTypeUse::Unit => write!(out, "AlgebraicType.createProductType([])"),
        AlgebraicTypeUse::Never => unimplemented!(),
        AlgebraicTypeUse::String => write!(out, "AlgebraicType.createStringType()"),
    }
}

fn convert_sum_type<'a>(
    module: &'a ModuleDef,
    out: &mut Indenter,
    variants: &'a [(Identifier, AlgebraicTypeUse)],
    ref_prefix: &'a str,
) {
    writeln!(out, "AlgebraicType.createSumType([");
    out.indent(1);
    for (ident, ty) in variants {
        write!(out, "new SumTypeVariant(\"{ident}\", ",);
        convert_algebraic_type(module, out, ty, ref_prefix);
        writeln!(out, "),");
    }
    out.dedent(1);
    write!(out, "])")
}

fn convert_product_type<'a>(
    module: &'a ModuleDef,
    out: &mut Indenter,
    elements: &'a [(Identifier, AlgebraicTypeUse)],
    ref_prefix: &'a str,
) {
    writeln!(out, "AlgebraicType.createProductType([");
    out.indent(1);
    for (ident, ty) in elements {
        write!(
            out,
            "new ProductTypeElement(\"{}\", ",
            ident.deref().to_case(Case::Camel),
        );
        convert_algebraic_type(module, out, ty, ref_prefix);
        writeln!(out, "),");
    }
    out.dedent(1);
    write!(out, "])")
}

/// Print imports for each of the `imports`.
fn print_imports(module: &ModuleDef, out: &mut Indenter, imports: Imports) {
    for typeref in imports {
        let module_name = type_ref_module_name(module, typeref);
        let type_name = type_ref_name(module, typeref);
        writeln!(out, "// @ts-ignore");
        writeln!(
            out,
            "import {{ {type_name} as __{type_name} }} from \"./{module_name}\";"
        );
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

/// Iterate over all the [`TypeDef`]s defined by the module, in alphabetical order by name.
///
/// Sorting is necessary to have deterministic reproducable codegen.
fn iter_types(module: &ModuleDef) -> impl Iterator<Item = &TypeDef> {
    module.types().sorted_by_key(|table| &table.name)
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

// const RESERVED_KEYWORDS: [&str; 36] = [
//     "break",
//     "case",
//     "catch",
//     "class",
//     "const",
//     "continue",
//     "debugger",
//     "default",
//     "delete",
//     "do",
//     "else",
//     "enum",
//     "export",
//     "extends",
//     "false",
//     "finally",
//     "for",
//     "function",
//     "if",
//     "import",
//     "in",
//     "instanceof",
//     "new",
//     "null",
//     "return",
//     "super",
//     "switch",
//     "this",
//     "throw",
//     "true",
//     "try",
//     "typeof",
//     "var",
//     "void",
//     "while",
//     "with",
// ];

// fn typescript_field_name(field_name: String) -> String {
//     if RESERVED_KEYWORDS
//         .into_iter()
//         .map(String::from)
//         .collect::<Vec<String>>()
//         .contains(&field_name)
//     {
//         return format!("_{field_name}");
//     }

//     field_name
// }
