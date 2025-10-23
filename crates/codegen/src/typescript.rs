use crate::util::{
    is_reducer_invokable, iter_reducers, iter_tables, iter_types, iter_unique_cols,
    print_auto_generated_version_comment,
};
use crate::{indent_scope, OutputFile};

use super::util::{collect_case, print_auto_generated_file_comment, type_ref_name};

use std::collections::BTreeSet;
use std::fmt::{self, Write};
use std::ops::Deref;

use convert_case::{Case, Casing};
use spacetimedb_lib::sats::layout::PrimitiveType;
use spacetimedb_lib::sats::AlgebraicTypeRef;
use spacetimedb_schema::def::{ModuleDef, ReducerDef, ScopedTypeName, TableDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::schema::{Schema, TableSchema};
use spacetimedb_schema::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse, ProductTypeDef};

use super::code_indenter::{CodeIndenter, Indenter};
use super::Lang;
use spacetimedb_lib::version::spacetimedb_lib_version;

type Imports = BTreeSet<AlgebraicTypeRef>;

const INDENT: &str = "  ";

pub struct TypeScript;

impl Lang for TypeScript {
    fn generate_type_files(&self, module: &ModuleDef, typ: &TypeDef) -> Vec<OutputFile> {
        let type_name = collect_case(Case::Pascal, typ.name.name_segments());

        let define_type_for_product = |product: &ProductTypeDef| {
            let mut output = CodeIndenter::new(String::new(), INDENT);
            let out = &mut output;

            print_file_header(out, false);
            gen_and_print_imports(module, out, &product.elements, &[typ.ty], None);
            writeln!(out);
            define_body_for_product(module, out, &type_name, &product.elements);
            out.newline();
            OutputFile {
                filename: type_module_name(&typ.name) + ".ts",
                code: output.into_inner(),
            }
        };

        let define_variants_for_sum = |variants: &[(Identifier, AlgebraicTypeUse)]| {
            let mut output = CodeIndenter::new(String::new(), INDENT);
            let out = &mut output;

            print_file_header(out, false);
            // Note that the current type is not included in dont_import below.
            gen_and_print_imports(module, out, variants, &[], Some("Type"));
            writeln!(out);
            write_variant_types(module, out, variants);
            out.newline();
            OutputFile {
                filename: variants_module_name(&typ.name) + ".ts",
                code: output.into_inner(),
            }
        };

        let define_type_for_sum = |variants: &[(Identifier, AlgebraicTypeUse)]| {
            let mut output = CodeIndenter::new(String::new(), INDENT);
            let out = &mut output;

            print_file_header(out, false);
            gen_and_print_imports(module, out, variants, &[typ.ty], None);
            writeln!(
                out,
                "import * as {}Variants from './{}'",
                type_name,
                variants_module_name(&typ.name)
            );
            writeln!(out);
            // For the purpose of bootstrapping AlgebraicType, if the name of the type
            // is `AlgebraicType`, we need to use an alias.
            define_body_for_sum(module, out, &type_name, variants);
            out.newline();
            OutputFile {
                filename: type_module_name(&typ.name) + ".ts",
                code: output.into_inner(),
            }
        };

        match &module.typespace_for_generate()[typ.ty] {
            AlgebraicTypeDef::Product(product) => {
                vec![define_type_for_product(product)]
            }
            AlgebraicTypeDef::Sum(sum) => {
                vec![
                    define_variants_for_sum(&sum.variants),
                    define_type_for_sum(&sum.variants),
                ]
            }
            AlgebraicTypeDef::PlainEnum(plain_enum) => {
                let variants = plain_enum
                    .variants
                    .iter()
                    .cloned()
                    .map(|var| (var, AlgebraicTypeUse::Unit))
                    .collect::<Vec<_>>();
                vec![define_variants_for_sum(&variants), define_type_for_sum(&variants)]
            }
        }
    }

    fn generate_table_file(&self, module: &ModuleDef, table: &TableDef) -> OutputFile {
        let schema = TableSchema::from_module_def(module, table, (), 0.into())
            .validated()
            .expect("Failed to generate table due to validation errors");

        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, false);

        let type_ref = table.product_type_ref;
        let row_type = type_ref_name(module, type_ref);
        let row_type_module = type_ref_module_name(module, type_ref);

        writeln!(out, "import {{ {row_type} }} from \"./{row_type_module}\";");

        let product_def = module.typespace_for_generate()[type_ref].as_product().unwrap();

        // Import the types of all fields.
        // We only need to import fields which have indices or unique constraints,
        // but it's easier to just import all of 'em, since we have `// @ts-nocheck` anyway.
        gen_and_print_imports(
            module,
            out,
            &product_def.elements,
            &[], // No need to skip any imports; we're not defining a type, so there's no chance of circular imports.
            None,
        );

        writeln!(
            out,
            "import {{ type EventContext, type Reducer, RemoteReducers, RemoteTables }} from \".\";"
        );

        // Mark potentially unused types
        writeln!(
            out,
            "declare type __keep = [EventContext, Reducer, RemoteReducers, RemoteTables];"
        );

        let table_name = table.name.deref();
        let table_name_pascalcase = table.name.deref().to_case(Case::Pascal);
        let table_handle = table_name_pascalcase.clone() + "TableHandle";
        let accessor_method = table_method_name(&table.name);

        writeln!(out);

        write!(
            out,
            "/**
 * Table handle for the table `{table_name}`.
 *
 * Obtain a handle from the [`{accessor_method}`] property on [`RemoteTables`],
 * like `ctx.db.{accessor_method}`.
 *
 * Users are encouraged not to explicitly reference this type,
 * but to directly chain method calls,
 * like `ctx.db.{accessor_method}.on_insert(...)`.
 */
export class {table_handle}<TableName extends string> implements __TableHandle<TableName> {{
"
        );
        out.indent(1);
        writeln!(out, "// phantom type to track the table name");
        writeln!(out, "readonly tableName!: TableName;");
        writeln!(out, "tableCache: __TableCache<{row_type}>;");
        writeln!(out);
        writeln!(out, "constructor(tableCache: __TableCache<{row_type}>) {{");
        out.with_indent(|out| writeln!(out, "this.tableCache = tableCache;"));
        writeln!(out, "}}");
        writeln!(out);
        writeln!(out, "count(): number {{");
        out.with_indent(|out| {
            writeln!(out, "return this.tableCache.count();");
        });
        writeln!(out, "}}");
        writeln!(out);
        writeln!(out, "iter(): Iterable<{row_type}> {{");
        out.with_indent(|out| {
            writeln!(out, "return this.tableCache.iter();");
        });
        writeln!(out, "}}");

        for (unique_field_ident, unique_field_type_use) in
            iter_unique_cols(module.typespace_for_generate(), &schema, product_def)
        {
            let unique_field_name = unique_field_ident.deref().to_case(Case::Camel);
            let unique_field_name_pascalcase = unique_field_name.to_case(Case::Pascal);

            let unique_constraint = table_name_pascalcase.clone() + &unique_field_name_pascalcase + "Unique";
            let unique_field_type = type_name(module, unique_field_type_use);

            writeln!(
                out,
                "/**
 * Access to the `{unique_field_name}` unique index on the table `{table_name}`,
 * which allows point queries on the field of the same name
 * via the [`{unique_constraint}.find`] method.
 *
 * Users are encouraged not to explicitly reference this type,
 * but to directly chain method calls,
 * like `ctx.db.{accessor_method}.{unique_field_name}().find(...)`.
 *
 * Get a handle on the `{unique_field_name}` unique index on the table `{table_name}`.
 */"
            );
            writeln!(out, "{unique_field_name} = {{");
            out.with_indent(|out| {
                writeln!(
                    out,
                    "// Find the subscribed row whose `{unique_field_name}` column value is equal to `col_val`,"
                );
                writeln!(out, "// if such a row is present in the client cache.");
                writeln!(
                    out,
                    "find: (col_val: {unique_field_type}): {row_type} | undefined => {{"
                );
                out.with_indent(|out| {
                    writeln!(out, "for (let row of this.tableCache.iter()) {{");
                    out.with_indent(|out| {
                        writeln!(out, "if (__deepEqual(row.{unique_field_name}, col_val)) {{");
                        out.with_indent(|out| {
                            writeln!(out, "return row;");
                        });
                        writeln!(out, "}}");
                    });
                    writeln!(out, "}}");
                });
                writeln!(out, "}},");
            });
            writeln!(out, "}};");
        }

        writeln!(out);

        // TODO: expose non-unique indices.

        writeln!(
            out,
            "onInsert = (cb: (ctx: EventContext, row: {row_type}) => void) => {{
{INDENT}return this.tableCache.onInsert(cb);
}}

removeOnInsert = (cb: (ctx: EventContext, row: {row_type}) => void) => {{
{INDENT}return this.tableCache.removeOnInsert(cb);
}}

onDelete = (cb: (ctx: EventContext, row: {row_type}) => void) => {{
{INDENT}return this.tableCache.onDelete(cb);
}}

removeOnDelete = (cb: (ctx: EventContext, row: {row_type}) => void) => {{
{INDENT}return this.tableCache.removeOnDelete(cb);
}}"
        );

        if schema.pk().is_some() {
            write!(
                out,
                "
// Updates are only defined for tables with primary keys.
onUpdate = (cb: (ctx: EventContext, oldRow: {row_type}, newRow: {row_type}) => void) => {{
{INDENT}return this.tableCache.onUpdate(cb);
}}

removeOnUpdate = (cb: (ctx: EventContext, onRow: {row_type}, newRow: {row_type}) => void) => {{
{INDENT}return this.tableCache.removeOnUpdate(cb);
}}"
            );
        }
        out.dedent(1);

        writeln!(out, "}}");
        OutputFile {
            filename: table_module_name(&table.name) + ".ts",
            code: output.into_inner(),
        }
    }

    fn generate_reducer_file(&self, module: &ModuleDef, reducer: &ReducerDef) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, false);

        out.newline();

        gen_and_print_imports(
            module,
            out,
            &reducer.params_for_generate.elements,
            // No need to skip any imports; we're not emitting a type that other modules can import.
            &[],
            None,
        );

        let args_type = reducer_args_type_name(&reducer.name);

        define_body_for_product(module, out, &args_type, &reducer.params_for_generate.elements);

        OutputFile {
            filename: reducer_module_name(&reducer.name) + ".ts",
            code: output.into_inner(),
        }
    }

    fn generate_global_files(&self, module: &ModuleDef) -> Vec<OutputFile> {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, true);

        out.newline();

        writeln!(out, "// Import and reexport all reducer arg types");
        for reducer in iter_reducers(module) {
            let reducer_name = &reducer.name;
            let reducer_module_name = reducer_module_name(reducer_name) + ".ts";
            let args_type = reducer_args_type_name(&reducer.name);
            writeln!(out, "import {{ {args_type} }} from \"./{reducer_module_name}\";");
            writeln!(out, "export {{ {args_type} }};");
        }

        writeln!(out);
        writeln!(out, "// Import and reexport all table handle types");
        for table in iter_tables(module) {
            let table_name = &table.name;
            let table_module_name = table_module_name(table_name) + ".ts";
            let table_name_pascalcase = table.name.deref().to_case(Case::Pascal);
            let table_handle = table_name_pascalcase.clone() + "TableHandle";
            writeln!(out, "import {{ {table_handle} }} from \"./{table_module_name}\";");
            writeln!(out, "export {{ {table_handle} }};");
        }

        writeln!(out);
        writeln!(out, "// Import and reexport all types");
        for ty in iter_types(module) {
            let type_name = collect_case(Case::Pascal, ty.name.name_segments());
            let type_module_name = type_module_name(&ty.name) + ".ts";
            writeln!(out, "import {{ {type_name} }} from \"./{type_module_name}\";");
            writeln!(out, "export {{ {type_name} }};");
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
            writeln!(out, "tableName: \"{}\" as const,", table.name);
            writeln!(out, "rowType: {row_type}.getTypeScriptAlgebraicType(),");
            if let Some(pk) = schema.pk() {
                // This is left here so we can release the codegen change before releasing a new
                // version of the SDK.
                //
                // Eventually we can remove this and only generate use the `primaryKeyInfo` field.
                writeln!(out, "primaryKey: \"{}\",", pk.col_name.to_string().to_case(Case::Camel));

                writeln!(out, "primaryKeyInfo: {{");
                out.indent(1);
                writeln!(out, "colName: \"{}\",", pk.col_name.to_string().to_case(Case::Camel));
                writeln!(
                    out,
                    "colType: ({row_type}.getTypeScriptAlgebraicType() as __AlgebraicTypeVariants.Product).value.elements[{}].algebraicType,",
                    pk.col_pos.0
                );
                out.dedent(1);
                writeln!(out, "}},");
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
                "argsType: {args_type}.getTypeScriptAlgebraicType(),",
                args_type = reducer_args_type_name(&reducer.name)
            );
            out.dedent(1);
            writeln!(out, "}},");
        }
        out.dedent(1);
        writeln!(out, "}},");
        writeln!(out, "versionInfo: {{");
        out.indent(1);
        writeln!(out, "cliVersion: \"{}\",", spacetimedb_lib_version());
        out.dedent(1);
        writeln!(out, "}},");
        writeln!(
            out,
            "// Constructors which are used by the DbConnectionImpl to
// extract type information from the generated RemoteModule.
//
// NOTE: This is not strictly necessary for `eventContextConstructor` because
// all we do is build a TypeScript object which we could have done inside the
// SDK, but if in the future we wanted to create a class this would be
// necessary because classes have methods, so we'll keep it.
eventContextConstructor: (imp: __DbConnectionImpl, event: __Event<Reducer>) => {{
  return {{
    ...(imp as DbConnection),
    event
  }}
}},
dbViewConstructor: (imp: __DbConnectionImpl) => {{
  return new RemoteTables(imp);
}},
reducersConstructor: (imp: __DbConnectionImpl, setReducerFlags: SetReducerFlags) => {{
  return new RemoteReducers(imp, setReducerFlags);
}},
setReducerFlagsConstructor: () => {{
  return new SetReducerFlags();
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

        print_set_reducer_flags(module, out);

        out.newline();

        print_remote_tables(module, out);

        out.newline();

        print_subscription_builder(module, out);

        out.newline();

        print_db_connection(module, out);

        out.newline();

        writeln!(
            out,
            "export type EventContext = __EventContextInterface<RemoteTables, RemoteReducers, SetReducerFlags, Reducer>;"
        );

        writeln!(
            out,
            "export type ReducerEventContext = __ReducerEventContextInterface<RemoteTables, RemoteReducers, SetReducerFlags, Reducer>;"
        );

        writeln!(
            out,
            "export type SubscriptionEventContext = __SubscriptionEventContextInterface<RemoteTables, RemoteReducers, SetReducerFlags>;"
        );

        writeln!(
            out,
            "export type ErrorContext = __ErrorContextInterface<RemoteTables, RemoteReducers, SetReducerFlags>;"
        );

        vec![OutputFile {
            filename: "index.ts".to_string(),
            code: output.into_inner(),
        }]
    }
}

fn print_remote_reducers(module: &ModuleDef, out: &mut Indenter) {
    writeln!(out, "export class RemoteReducers {{");
    out.indent(1);
    writeln!(
        out,
        "constructor(private connection: __DbConnectionImpl, private setCallReducerFlags: SetReducerFlags) {{}}"
    );
    out.newline();

    for reducer in iter_reducers(module) {
        // The reducer argument names and types as `ident: ty, ident: ty, ident: ty`,
        // and the argument names as `ident, ident, ident`
        // for passing to function call and struct literal expressions.
        let mut arg_list = "".to_string();
        let mut arg_name_list = "".to_string();
        for (arg_ident, arg_ty) in &reducer.params_for_generate.elements[..] {
            let arg_name = arg_ident.deref().to_case(Case::Camel);
            arg_name_list += &arg_name;
            arg_list += &arg_name;
            arg_list += ": ";
            write_type(module, &mut arg_list, arg_ty, None, None).unwrap();
            arg_list += ", ";
            arg_name_list += ", ";
        }
        let arg_list = arg_list.trim_end_matches(", ");
        let arg_name_list = arg_name_list.trim_end_matches(", ");

        let reducer_name = &reducer.name;

        if is_reducer_invokable(reducer) {
            let reducer_function_name = reducer_function_name(reducer);
            let reducer_variant = reducer_variant_name(&reducer.name);
            if reducer.params_for_generate.elements.is_empty() {
                writeln!(out, "{reducer_function_name}() {{");
                out.with_indent(|out| {
                    writeln!(
                        out,
                        "this.connection.callReducer(\"{reducer_name}\", new Uint8Array(0), this.setCallReducerFlags.{reducer_function_name}Flags);"
                    );
                });
            } else {
                writeln!(out, "{reducer_function_name}({arg_list}) {{");
                out.with_indent(|out| {
                    writeln!(out, "const __args = {{ {arg_name_list} }};");
                    writeln!(out, "let __writer = new __BinaryWriter(1024);");
                    writeln!(
                        out,
                        "{reducer_variant}.serialize(__writer, __args);"
                    );
                    writeln!(out, "let __argsBuffer = __writer.getBuffer();");
                    writeln!(out, "this.connection.callReducer(\"{reducer_name}\", __argsBuffer, this.setCallReducerFlags.{reducer_function_name}Flags);");
                });
            }
            writeln!(out, "}}");
            out.newline();
        }

        let arg_list_padded = if arg_list.is_empty() {
            String::new()
        } else {
            format!(", {arg_list}")
        };
        let reducer_name_pascal = reducer_name.deref().to_case(Case::Pascal);
        writeln!(
            out,
            "on{reducer_name_pascal}(callback: (ctx: ReducerEventContext{arg_list_padded}) => void) {{"
        );
        out.indent(1);
        writeln!(out, "this.connection.onReducer(\"{reducer_name}\", callback);");
        out.dedent(1);
        writeln!(out, "}}");
        out.newline();
        writeln!(
            out,
            "removeOn{reducer_name_pascal}(callback: (ctx: ReducerEventContext{arg_list_padded}) => void) {{"
        );
        out.indent(1);
        writeln!(out, "this.connection.offReducer(\"{reducer_name}\", callback);");
        out.dedent(1);
        writeln!(out, "}}");
        out.newline();
    }

    out.dedent(1);
    writeln!(out, "}}");
}

fn print_set_reducer_flags(module: &ModuleDef, out: &mut Indenter) {
    writeln!(out, "export class SetReducerFlags {{");
    out.indent(1);

    for reducer in iter_reducers(module).filter(|r| is_reducer_invokable(r)) {
        let reducer_function_name = reducer_function_name(reducer);
        writeln!(out, "{reducer_function_name}Flags: __CallReducerFlags = 'FullUpdate';");
        writeln!(out, "{reducer_function_name}(flags: __CallReducerFlags) {{");
        out.with_indent(|out| {
            writeln!(out, "this.{reducer_function_name}Flags = flags;");
        });
        writeln!(out, "}}");
        out.newline();
    }

    out.dedent(1);
    writeln!(out, "}}");
}

fn print_remote_tables(module: &ModuleDef, out: &mut Indenter) {
    writeln!(out, "export class RemoteTables {{");
    out.indent(1);
    writeln!(out, "constructor(private connection: __DbConnectionImpl) {{}}");

    for table in iter_tables(module) {
        writeln!(out);
        let table_name = table.name.deref();
        let table_name_pascalcase = table.name.deref().to_case(Case::Pascal);
        let table_name_camelcase = table.name.deref().to_case(Case::Camel);
        let table_handle = table_name_pascalcase.clone() + "TableHandle";
        let type_ref = table.product_type_ref;
        let row_type = type_ref_name(module, type_ref);
        writeln!(out, "get {table_name_camelcase}(): {table_handle}<'{table_name}'> {{");
        out.with_indent(|out| {
            writeln!(out, "// clientCache is a private property");
            writeln!(
                out,
                "return new {table_handle}((this.connection as unknown as {{ clientCache: __ClientCache }}).clientCache.getOrCreateTable<{row_type}>(REMOTE_MODULE.tables.{table_name}));"
            );
        });
        writeln!(out, "}}");
    }

    out.dedent(1);
    writeln!(out, "}}");
}

fn print_subscription_builder(_module: &ModuleDef, out: &mut Indenter) {
    writeln!(
        out,
        "export class SubscriptionBuilder extends __SubscriptionBuilderImpl<RemoteTables, RemoteReducers, SetReducerFlags> {{ }}"
    );
}

fn print_db_connection(_module: &ModuleDef, out: &mut Indenter) {
    writeln!(
        out,
        "export class DbConnection extends __DbConnectionImpl<RemoteTables, RemoteReducers, SetReducerFlags> {{"
    );
    out.indent(1);
    writeln!(
        out,
        "static builder = (): __DbConnectionBuilder<DbConnection, ErrorContext, SubscriptionEventContext> => {{"
    );
    out.indent(1);
    writeln!(
        out,
        "return new __DbConnectionBuilder<DbConnection, ErrorContext, SubscriptionEventContext>(REMOTE_MODULE, (imp: __DbConnectionImpl) => imp as DbConnection);"
    );
    out.dedent(1);
    writeln!(out, "}}");
    writeln!(out, "subscriptionBuilder = (): SubscriptionBuilder => {{");
    out.indent(1);
    writeln!(out, "return new SubscriptionBuilder(this);");
    out.dedent(1);
    writeln!(out, "}}");
    out.dedent(1);
    writeln!(out, "}}");
}

fn print_reducer_enum_defn(module: &ModuleDef, out: &mut Indenter) {
    writeln!(out, "// A type representing all the possible variants of a reducer.");
    writeln!(out, "export type Reducer = never");
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

fn print_spacetimedb_imports(out: &mut Indenter) {
    // All library imports are prefixed with `__` to avoid
    // clashing with the names of user generated types.
    let mut types = [
        "type AlgebraicType as __AlgebraicTypeType",
        "AlgebraicType as __AlgebraicTypeValue",
        "type AlgebraicTypeVariants as __AlgebraicTypeVariants",
        "Identity as __Identity",
        "ClientCache as __ClientCache",
        "ConnectionId as __ConnectionId",
        "Timestamp as __Timestamp",
        "TimeDuration as __TimeDuration",
        "DbConnectionBuilder as __DbConnectionBuilder",
        "TableCache as __TableCache",
        "BinaryWriter as __BinaryWriter",
        "type TableHandle as __TableHandle",
        "type CallReducerFlags as __CallReducerFlags",
        "type EventContextInterface as __EventContextInterface",
        "type ReducerEventContextInterface as __ReducerEventContextInterface",
        "type SubscriptionEventContextInterface as __SubscriptionEventContextInterface",
        "type ErrorContextInterface as __ErrorContextInterface",
        "SubscriptionBuilderImpl as __SubscriptionBuilderImpl",
        "BinaryReader as __BinaryReader",
        "DbConnectionImpl as __DbConnectionImpl",
        "type Event as __Event",
        "deepEqual as __deepEqual",
    ];
    types.sort();
    writeln!(out, "import {{");
    out.indent(1);
    for ty in &types {
        writeln!(out, "{ty},");
    }
    out.dedent(1);
    writeln!(out, "}} from \"spacetimedb\";");
}

fn print_file_header(output: &mut Indenter, include_version: bool) {
    print_auto_generated_file_comment(output);
    if include_version {
        print_auto_generated_version_comment(output);
    }
    print_lint_suppression(output);
    print_spacetimedb_imports(output);
}

fn print_lint_suppression(output: &mut Indenter) {
    writeln!(output, "/* eslint-disable */");
    writeln!(output, "/* tslint:disable */");
}

fn write_get_algebraic_type_for_product(
    module: &ModuleDef,
    out: &mut Indenter,
    type_cache_name: &str,
    elements: &[(Identifier, AlgebraicTypeUse)],
) {
    writeln!(
        out,
        "/**
* A function which returns this type represented as an AlgebraicType.
* This function is derived from the AlgebraicType used to generate this type.
*/"
    );
    writeln!(out, "getTypeScriptAlgebraicType(): __AlgebraicTypeType {{");
    {
        out.indent(1);
        writeln!(out, "if ({type_cache_name}) return {type_cache_name};");
        // initialization is split in two because of recursive types
        writeln!(
            out,
            "{type_cache_name} = __AlgebraicTypeValue.Product({{ elements: [] }});"
        );
        writeln!(out, "{type_cache_name}.value.elements.push(");
        out.indent(1);
        convert_product_type_elements(module, out, elements, "");
        out.dedent(1);
        writeln!(out, ");");
        writeln!(out, "return {type_cache_name};");
        out.dedent(1);
    }
    writeln!(out, "}},");
}

fn define_body_for_product(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    elements: &[(Identifier, AlgebraicTypeUse)],
) {
    write!(out, "export type {name} = {{");
    if elements.is_empty() {
        writeln!(out, "}};");
    } else {
        writeln!(out);
        out.with_indent(|out| write_arglist_no_delimiters(module, out, elements, None, true).unwrap());
        writeln!(out, "}};");
    }

    let type_cache_name = &*format!("_cached_{name}_type_value");
    writeln!(out, "let {type_cache_name}: __AlgebraicTypeType | null = null;");
    out.newline();

    writeln!(
        out,
        "/**
 * An object for generated helper functions.
 */"
    );
    writeln!(out, "export const {name} = {{");
    out.indent(1);
    write_get_algebraic_type_for_product(module, out, type_cache_name, elements);
    writeln!(out);

    writeln!(out, "serialize(writer: __BinaryWriter, value: {name}): void {{");
    out.indent(1);
    writeln!(
        out,
        "__AlgebraicTypeValue.serializeValue(writer, {name}.getTypeScriptAlgebraicType(), value);"
    );
    out.dedent(1);
    writeln!(out, "}},");
    writeln!(out);

    writeln!(out, "deserialize(reader: __BinaryReader): {name} {{");
    out.indent(1);
    writeln!(
        out,
        "return __AlgebraicTypeValue.deserializeValue(reader, {name}.getTypeScriptAlgebraicType());"
    );
    out.dedent(1);
    writeln!(out, "}},");
    writeln!(out);

    out.dedent(1);
    writeln!(out, "}}");

    out.newline();

    writeln!(out, "export default {name};");

    out.newline();
}

fn write_arglist_no_delimiters(
    module: &ModuleDef,
    out: &mut impl Write,
    elements: &[(Identifier, AlgebraicTypeUse)],
    prefix: Option<&str>,
    convert_case: bool,
) -> anyhow::Result<()> {
    for (ident, ty) in elements {
        if let Some(prefix) = prefix {
            write!(out, "{prefix} ")?;
        }

        let name = if convert_case {
            ident.deref().to_case(Case::Camel)
        } else {
            ident.deref().into()
        };

        write!(out, "{name}: ")?;
        write_type(module, out, ty, None, None)?;
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
    // { tag: "Bar", value: BarType }
    // { tag: "Bar", value: number }
    // { tag: "Bar", value: string }
    // ```
    // Note you could alternatively do:
    // ```
    // { tag: "Bar" } & BarType
    // ```
    // for non-primitive types but that doesn't extend to primitives.
    // Another alternative would be to name the value field the same as the tag field, but lowercased
    // ```
    // { tag: "Bar", bar: BarType }
    // { tag: "Bar", bar: number }
    // { tag: "Bar", bar: string }
    // ```
    // but this is a departure from our previous convention and is not much different.
    if !matches!(ty, AlgebraicTypeUse::Unit) {
        write!(out, ", value: ");
        write_type(module, out, ty, None, Some("Type")).unwrap();
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
            // Foo: { tag: "Foo" } = { tag: "Foo" } as const,
            // ```
            write!(out, "{ident}: {{ tag: \"{ident}\" }} as const,");
            writeln!(out);
            continue;
        }
        let variant_name = ident.deref().to_case(Case::Pascal);
        write!(out, "{variant_name}: (value: ");
        write_type(module, out, ty, None, None).unwrap();
        writeln!(
            out,
            "): {name}Variants.{variant_name} => ({{ tag: \"{variant_name}\", value }}),"
        );
    }
}

fn write_get_algebraic_type_for_sum(
    module: &ModuleDef,
    out: &mut Indenter,
    type_cache_name: &str,
    variants: &[(Identifier, AlgebraicTypeUse)],
) {
    writeln!(out, "getTypeScriptAlgebraicType(): __AlgebraicTypeType {{");
    {
        indent_scope!(out);
        writeln!(out, "if ({type_cache_name}) return {type_cache_name};");
        // initialization is split in two because of recursive types
        writeln!(out, "{type_cache_name} = __AlgebraicTypeValue.Sum({{ variants: [] }});");
        writeln!(out, "{type_cache_name}.value.variants.push(");
        out.indent(1);
        convert_sum_type_variants(module, &mut out, variants, "");
        out.dedent(1);
        writeln!(out, ");");
        writeln!(out, "return {type_cache_name};");
    }
    writeln!(out, "}},");
}

fn define_body_for_sum(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    variants: &[(Identifier, AlgebraicTypeUse)],
) {
    writeln!(out, "// The tagged union or sum type for the algebraic type `{name}`.");
    write!(out, "export type {name} = ");

    let names = variants
        .iter()
        .map(|(ident, _)| format!("{name}Variants.{}", ident.deref().to_case(Case::Pascal)))
        .collect::<Vec<String>>()
        .join(" |\n  ");

    if variants.is_empty() {
        writeln!(out, "never;");
    } else {
        writeln!(out, "{names};");
    }

    out.newline();

    let type_cache_name = &*format!("_cached_{name}_type_value");
    writeln!(out, "let {type_cache_name}: __AlgebraicTypeType | null = null;");
    out.newline();

    // Write the runtime value with helper functions
    writeln!(out, "// A value with helper functions to construct the type.");
    writeln!(out, "export const {name} = {{");
    out.indent(1);

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
    write_get_algebraic_type_for_sum(module, out, type_cache_name, variants);
    writeln!(out);

    writeln!(
        out,
        "serialize(writer: __BinaryWriter, value: {name}): void {{
    __AlgebraicTypeValue.serializeValue(writer, {name}.getTypeScriptAlgebraicType(), value);
}},"
    );
    writeln!(out);

    writeln!(
        out,
        "deserialize(reader: __BinaryReader): {name} {{
    return __AlgebraicTypeValue.deserializeValue(reader, {name}.getTypeScriptAlgebraicType());
}},"
    );
    writeln!(out);

    out.dedent(1);

    writeln!(out, "}}");
    out.newline();

    writeln!(out, "export default {name};");

    out.newline();
}

fn type_ref_module_name(module: &ModuleDef, type_ref: AlgebraicTypeRef) -> String {
    let (name, _) = module.type_def_from_ref(type_ref).unwrap();
    type_module_name(name)
}

fn type_module_name(type_name: &ScopedTypeName) -> String {
    collect_case(Case::Snake, type_name.name_segments()) + "_type"
}

fn variants_module_name(type_name: &ScopedTypeName) -> String {
    collect_case(Case::Snake, type_name.name_segments()) + "_variants"
}

fn table_module_name(table_name: &Identifier) -> String {
    table_name.deref().to_case(Case::Snake) + "_table"
}

fn table_method_name(table_name: &Identifier) -> String {
    table_name.deref().to_case(Case::Camel)
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
    write_type(module, &mut s, ty, None, None).unwrap();
    s
}

// This should return true if we should wrap the type in parentheses when it is the element type of
// an array. This is needed if the type has a `|` in it, e.g. `Option<T>` or `Foo | Bar`, since
// without parens, `Foo | Bar[]` would be parsed as `Foo | (Bar[])`.
fn needs_parens_within_array(ty: &AlgebraicTypeUse) -> bool {
    match ty {
        AlgebraicTypeUse::Unit
        | AlgebraicTypeUse::Never
        | AlgebraicTypeUse::Identity
        | AlgebraicTypeUse::ConnectionId
        | AlgebraicTypeUse::Timestamp
        | AlgebraicTypeUse::TimeDuration
        | AlgebraicTypeUse::Primitive(_)
        | AlgebraicTypeUse::Array(_)
        | AlgebraicTypeUse::Ref(_) // We use the type name for these.
        | AlgebraicTypeUse::String => {
            false
        }
        AlgebraicTypeUse::ScheduleAt | AlgebraicTypeUse::Option(_) => {
            true
        }
    }
}

pub fn write_type<W: Write>(
    module: &ModuleDef,
    out: &mut W,
    ty: &AlgebraicTypeUse,
    ref_prefix: Option<&str>,
    ref_suffix: Option<&str>,
) -> fmt::Result {
    match ty {
        AlgebraicTypeUse::Unit => write!(out, "void")?,
        AlgebraicTypeUse::Never => write!(out, "never")?,
        AlgebraicTypeUse::Identity => write!(out, "__Identity")?,
        AlgebraicTypeUse::ConnectionId => write!(out, "__ConnectionId")?,
        AlgebraicTypeUse::Timestamp => write!(out, "__Timestamp")?,
        AlgebraicTypeUse::TimeDuration => write!(out, "__TimeDuration")?,
        AlgebraicTypeUse::ScheduleAt => write!(
            out,
            "{{ tag: \"Interval\", value: __TimeDuration }} | {{ tag: \"Time\", value: __Timestamp }}"
        )?,
        AlgebraicTypeUse::Option(inner_ty) => {
            write_type(module, out, inner_ty, ref_prefix, ref_suffix)?;
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
            PrimitiveType::I64 => write!(out, "bigint")?,
            PrimitiveType::U64 => write!(out, "bigint")?,
            PrimitiveType::I128 => write!(out, "bigint")?,
            PrimitiveType::U128 => write!(out, "bigint")?,
            PrimitiveType::I256 => write!(out, "bigint")?,
            PrimitiveType::U256 => write!(out, "bigint")?,
            PrimitiveType::F32 => write!(out, "number")?,
            PrimitiveType::F64 => write!(out, "number")?,
        },
        AlgebraicTypeUse::String => write!(out, "string")?,
        AlgebraicTypeUse::Array(elem_ty) => {
            if matches!(&**elem_ty, AlgebraicTypeUse::Primitive(PrimitiveType::U8)) {
                return write!(out, "Uint8Array");
            }
            let needs_parens = needs_parens_within_array(elem_ty);
            // We wrap the inner type in parentheses to avoid ambiguity with the [] binding.
            if needs_parens {
                write!(out, "(")?;
            }
            write_type(module, out, elem_ty, ref_prefix, ref_suffix)?;
            if needs_parens {
                write!(out, ")")?;
            }
            write!(out, "[]")?;
        }
        AlgebraicTypeUse::Ref(r) => {
            if let Some(prefix) = ref_prefix {
                write!(out, "{prefix}")?;
            }
            write!(out, "{}", type_ref_name(module, *r))?;
            if let Some(suffix) = ref_suffix {
                write!(out, "{suffix}")?;
            }
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
        AlgebraicTypeUse::ScheduleAt => write!(out, "__AlgebraicTypeValue.createScheduleAtType()"),
        AlgebraicTypeUse::Identity => write!(out, "__AlgebraicTypeValue.createIdentityType()"),
        AlgebraicTypeUse::ConnectionId => write!(out, "__AlgebraicTypeValue.createConnectionIdType()"),
        AlgebraicTypeUse::Timestamp => write!(out, "__AlgebraicTypeValue.createTimestampType()"),
        AlgebraicTypeUse::TimeDuration => write!(out, "__AlgebraicTypeValue.createTimeDurationType()"),
        AlgebraicTypeUse::Option(inner_ty) => {
            write!(out, "__AlgebraicTypeValue.createOptionType(");
            convert_algebraic_type(module, out, inner_ty, ref_prefix);
            write!(out, ")");
        }
        AlgebraicTypeUse::Array(ty) => {
            write!(out, "__AlgebraicTypeValue.Array(");
            convert_algebraic_type(module, out, ty, ref_prefix);
            write!(out, ")");
        }
        AlgebraicTypeUse::Ref(r) => write!(
            out,
            "{ref_prefix}{}.getTypeScriptAlgebraicType()",
            type_ref_name(module, *r)
        ),
        AlgebraicTypeUse::Primitive(prim) => {
            write!(out, "__AlgebraicTypeValue.{prim:?}");
        }
        AlgebraicTypeUse::Unit => write!(out, "__AlgebraicTypeValue.Product({{ elements: [] }})"),
        AlgebraicTypeUse::Never => unimplemented!(),
        AlgebraicTypeUse::String => write!(out, "__AlgebraicTypeValue.String"),
    }
}

fn convert_sum_type_variants<'a>(
    module: &'a ModuleDef,
    out: &mut Indenter,
    variants: &'a [(Identifier, AlgebraicTypeUse)],
    ref_prefix: &'a str,
) {
    for (ident, ty) in variants {
        write!(out, "{{ name: \"{ident}\", algebraicType: ",);
        convert_algebraic_type(module, out, ty, ref_prefix);
        writeln!(out, " }},");
    }
}

fn convert_product_type_elements<'a>(
    module: &'a ModuleDef,
    out: &mut Indenter,
    elements: &'a [(Identifier, AlgebraicTypeUse)],
    ref_prefix: &'a str,
) {
    for (ident, ty) in elements {
        write!(
            out,
            "{{ name: \"{}\", algebraicType: ",
            ident.deref().to_case(Case::Camel)
        );
        convert_algebraic_type(module, out, ty, ref_prefix);
        writeln!(out, " }},");
    }
}

/// Print imports for each of the `imports`.
fn print_imports(module: &ModuleDef, out: &mut Indenter, imports: Imports, suffix: Option<&str>) {
    for typeref in imports {
        let module_name = type_ref_module_name(module, typeref);
        let type_name = type_ref_name(module, typeref);
        if let Some(suffix) = suffix {
            writeln!(
                out,
                "import {{ {type_name} as {type_name}{suffix} }} from \"./{module_name}\";"
            );
            writeln!(out, "// Mark import as potentially unused");
            writeln!(out, "declare type __keep_{type_name}{suffix} = {type_name}{suffix};");
        } else {
            writeln!(out, "import {{ {type_name} }} from \"./{module_name}\";");
            writeln!(out, "// Mark import as potentially unused");
            writeln!(out, "declare type __keep_{type_name} = {type_name};");
        }
    }
}

/// Use `search_function` on `roots` to detect required imports, then print them with `print_imports`.
///
/// `this_file` is passed and excluded for the case of recursive types:
/// without it, the definition for a type like `struct Foo { foos: Vec<Foo> }`
/// would attempt to include `import { Foo } from "./foo"`.
fn gen_and_print_imports(
    module: &ModuleDef,
    out: &mut Indenter,
    roots: &[(Identifier, AlgebraicTypeUse)],
    dont_import: &[AlgebraicTypeRef],
    suffix: Option<&str>,
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
    let len = imports.len();

    print_imports(module, out, imports, suffix);

    if len > 0 {
        out.newline();
    }
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
