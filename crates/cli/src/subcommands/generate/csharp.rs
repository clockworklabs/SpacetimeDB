// Note: the generated code depends on APIs and interfaces from crates/bindings-csharp/BSATN.Runtime.
use super::util::fmt_fn;

use std::fmt::{self, Write};
use std::ops::Deref;

use convert_case::{Case, Casing};
use spacetimedb_lib::sats::{AlgebraicType, AlgebraicTypeRef, ArrayType, ProductType, SumType};
use spacetimedb_lib::{ReducerDef, TableDesc};
use spacetimedb_primitives::ColList;
use spacetimedb_schema::schema::TableSchema;

use super::code_indenter::CodeIndenter;
use super::{GenCtx, GenItem};

fn scalar_or_string_name(b: &AlgebraicType) -> Option<&str> {
    Some(match b {
        AlgebraicType::Bool => "bool",
        AlgebraicType::I8 => "sbyte",
        AlgebraicType::U8 => "byte",
        AlgebraicType::I16 => "short",
        AlgebraicType::U16 => "ushort",
        AlgebraicType::I32 => "int",
        AlgebraicType::U32 => "uint",
        AlgebraicType::I64 => "long",
        AlgebraicType::U64 => "ulong",
        AlgebraicType::I128 => "I128",
        AlgebraicType::U128 => "U128",
        AlgebraicType::I256 => "I256",
        AlgebraicType::U256 => "U256",
        AlgebraicType::String => "string",
        AlgebraicType::F32 => "float",
        AlgebraicType::F64 => "double",
        AlgebraicType::Ref(_)
        | AlgebraicType::Sum(_)
        | AlgebraicType::Product(_)
        | AlgebraicType::Array(_)
        | AlgebraicType::Map(_) => return None,
    })
}

fn ty_fmt<'a>(ctx: &'a GenCtx, ty: &'a AlgebraicType, namespace: &'a str) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        ty if ty.is_identity() => f.write_str("SpacetimeDB.Identity"),
        ty if ty.is_address() => f.write_str("SpacetimeDB.Address"),
        ty if ty.is_schedule_at() => f.write_str("SpacetimeDB.ScheduleAt"),
        AlgebraicType::Sum(sum_type) => {
            // This better be an option type
            if let Some(inner_ty) = sum_type.as_option() {
                write!(f, "{}?", ty_fmt(ctx, inner_ty, namespace))
            } else {
                unimplemented!()
            }
        }
        // Arbitrary product types should fail.
        AlgebraicType::Product(_) => unimplemented!(),
        ty if ty.is_bytes() => f.write_str("byte[]"),
        AlgebraicType::Array(ArrayType { elem_ty }) => {
            write!(
                f,
                "System.Collections.Generic.List<{}>",
                ty_fmt(ctx, elem_ty, namespace)
            )
        }
        AlgebraicType::Map(ty) => {
            write!(
                f,
                "System.Collections.Generic.Dictionary<{}, {}>",
                ty_fmt(ctx, &ty.ty, namespace),
                ty_fmt(ctx, &ty.key_ty, namespace)
            )
        }
        AlgebraicType::Ref(r) => {
            let name = csharp_typename(ctx, *r);
            match &ctx.typespace[*r] {
                AlgebraicType::Sum(sum_type) => {
                    if sum_type.is_simple_enum() {
                        let parts: Vec<&str> = name.split('.').collect();
                        if parts.len() >= 2 {
                            let enum_namespace = parts[0];
                            let enum_name = parts[1];
                            write!(f, "{namespace}.{enum_namespace}.Types.{enum_name}")
                        } else {
                            write!(f, "{namespace}.{name}")
                        }
                    } else {
                        write!(f, "{namespace}.{name}")
                    }
                }
                _ => {
                    write!(f, "{namespace}.{name}")
                }
            }
        }
        ty => f.write_str(scalar_or_string_name(ty).expect("must be a scalar/string type at this point")),
    })
}

fn default_init(ctx: &GenCtx, ty: &AlgebraicType) -> Option<&'static str> {
    match ty {
        // Options have a default value of null which is fine for us, and simple enums have their own default.
        AlgebraicType::Sum(sum_type) if sum_type.is_option() || sum_type.is_simple_enum() => None,
        // TODO: generate some proper default here (what would it be for tagged enums?).
        AlgebraicType::Sum(_) => Some("null!"),
        // Byte arrays must be initialized to an empty array.
        ty if ty.is_bytes() => Some("Array.Empty<byte>()"),
        // For product types, arrays, and maps, we can use the default constructor.
        AlgebraicType::Product(_) | AlgebraicType::Array(_) | AlgebraicType::Map(_) => Some("new()"),
        // Strings must have explicit default value of "".
        AlgebraicType::String => Some(r#""""#),
        AlgebraicType::Ref(r) => default_init(ctx, &ctx.typespace[*r]),
        _ => {
            debug_assert!(ty.is_scalar());
            None
        }
    }
}

// can maybe do something fancy with this in the future
fn csharp_typename(ctx: &GenCtx, typeref: AlgebraicTypeRef) -> &str {
    ctx.names[typeref.idx()].as_deref().expect("tuples should have names")
}

macro_rules! indent_scope {
    ($x:ident) => {
        let mut $x = $x.indented(1);
    };
}

struct CsharpAutogen {
    output: CodeIndenter<String>,
}

impl std::ops::Deref for CsharpAutogen {
    type Target = CodeIndenter<String>;

    fn deref(&self) -> &Self::Target {
        &self.output
    }
}

impl std::ops::DerefMut for CsharpAutogen {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.output
    }
}

impl CsharpAutogen {
    pub fn new(namespace: &str, extra_usings: &[&str]) -> Self {
        let mut output = CodeIndenter::new(String::new());

        writeln!(
            output,
            "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE"
        );
        writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.");
        writeln!(output, "// <auto-generated />");
        writeln!(output);

        writeln!(output, "#nullable enable");
        writeln!(output);

        writeln!(output, "using System;");
        if namespace != "SpacetimeDB" {
            writeln!(output, "using SpacetimeDB;");
        }
        for extra_using in extra_usings {
            writeln!(output, "using {extra_using};");
        }
        writeln!(output);

        writeln!(output, "namespace {namespace}");
        writeln!(output, "{{");

        output.indent(1);
        Self { output }
    }

    pub fn into_inner(mut self) -> String {
        self.dedent(1);
        writeln!(self, "}}");
        self.output.into_inner()
    }
}

pub fn autogen_csharp_sum(ctx: &GenCtx, name: &str, sum_type: &SumType, namespace: &str) -> String {
    let mut output = CsharpAutogen::new(namespace, &[]);

    let mut sum_namespace = None;
    let mut sum_type_name = name.replace("r#", "").to_case(Case::Pascal);
    if sum_type_name.contains('.') {
        let split: Vec<&str> = sum_type_name.split('.').collect();
        if split.len() != 2 {
            panic!("Enum names cannot contain more than one namespace prefix. Example: MyNamespace.MyEnum");
        }

        sum_namespace = Some(split[0].to_string().to_case(Case::Pascal));
        sum_type_name = split[1].to_string().to_case(Case::Pascal);
    }

    if let Some(sum_namespace) = &sum_namespace {
        writeln!(output, "public partial class {sum_namespace}");
        writeln!(output, "{{");
        output.indent(1);

        writeln!(output, "public partial class Types");
        writeln!(output, "{{");
        output.indent(1);
    }

    writeln!(output, "[SpacetimeDB.Type]");

    if sum_type.is_simple_enum() {
        writeln!(output, "public enum {sum_type_name}");
        indented_block(&mut output, |output| {
            for variant in &*sum_type.variants {
                let variant_name = variant
                    .name
                    .as_ref()
                    .expect("All sum variants should have names!")
                    .replace("r#", "");
                writeln!(output, "{variant_name},");
            }
        });
    } else {
        write!(
            output,
            "public partial record {sum_type_name} : SpacetimeDB.TaggedEnum<("
        );
        {
            indent_scope!(output);
            for (i, variant) in sum_type.variants.iter().enumerate() {
                if i != 0 {
                    write!(output, ",");
                }
                writeln!(output);
                if variant.is_unit() {
                    write!(output, "SpacetimeDB.Unit");
                } else {
                    write!(output, "{}", ty_fmt(ctx, &variant.algebraic_type, namespace));
                }
                let variant_name = variant
                    .name
                    .as_ref()
                    .expect("All sum variants should have names!")
                    .replace("r#", "");
                write!(output, " {variant_name}");
            }
            // If we have less than 2 variants, we need to add some dummy variants to make the tuple work.
            match sum_type.variants.len() {
                0 => {
                    writeln!(output);
                    writeln!(output, "SpacetimeDB.Unit _Reserved1,");
                    write!(output, "SpacetimeDB.Unit _Reserved2");
                }
                1 => {
                    writeln!(output, ",");
                    write!(output, "SpacetimeDB.Unit _Reserved");
                }
                _ => {}
            }
        }
        writeln!(output);
        writeln!(output, ")>;");
    }

    if sum_namespace.is_some() {
        for _ in 0..2 {
            output.dedent(1);
            writeln!(output, "}}");
        }
    }

    output.into_inner()
}

pub fn autogen_csharp_tuple(ctx: &GenCtx, name: &str, tuple: &ProductType, namespace: &str) -> String {
    autogen_csharp_product_table_common(ctx, name, tuple, None, namespace)
}

#[allow(deprecated)]
pub fn autogen_csharp_table(ctx: &GenCtx, table: &TableDesc, namespace: &str) -> String {
    let tuple = ctx.typespace[table.data].as_product().unwrap();
    autogen_csharp_product_table_common(
        ctx,
        csharp_typename(ctx, table.data),
        tuple,
        Some(
            TableSchema::from_def(0.into(), table.schema.clone())
                .validated()
                .expect("Failed to generate table due to validation errors"),
        ),
        namespace,
    )
}

fn autogen_csharp_product_table_common(
    ctx: &GenCtx,
    name: &str,
    product_type: &ProductType,
    schema: Option<TableSchema>,
    namespace: &str,
) -> String {
    let mut output = CsharpAutogen::new(
        namespace,
        &[
            "System.Collections.Generic",
            "System.Linq",
            "System.Runtime.Serialization",
        ],
    );

    writeln!(output, "[SpacetimeDB.Type]");
    writeln!(output, "[DataContract]");
    write!(output, "public partial class {name}");
    if let Some(schema) = &schema {
        write!(
            output,
            " : SpacetimeDB.{parent}<{name}, {namespace}.ReducerEvent>",
            parent = if schema.pk().is_some() {
                "DatabaseTableWithPrimaryKey"
            } else {
                "DatabaseTable"
            }
        );
    }
    writeln!(output);
    indented_block(&mut output, |output| {
        let fields = product_type
            .elements
            .iter()
            .map(|field| {
                let orig_name = field
                    .name
                    .as_ref()
                    .expect("autogen'd tuples should have field names")
                    .replace("r#", "");

                writeln!(output, "[DataMember(Name = \"{orig_name}\")]");

                let field_name = orig_name.to_case(Case::Pascal);
                let ty = ty_fmt(ctx, &field.algebraic_type, namespace).to_string();

                writeln!(output, "public {ty} {field_name};");

                (field_name, ty)
            })
            .collect::<Vec<_>>();

        // Generate fully-parameterized constructor.
        writeln!(output);
        writeln!(output, "public {name}(");
        {
            indent_scope!(output);
            for (i, (field_name, ty)) in fields.iter().enumerate() {
                if i != 0 {
                    writeln!(output, ",");
                }
                write!(output, "{ty} {field_name}");
            }
        }
        writeln!(output);
        writeln!(output, ")");
        indented_block(output, |output| {
            for (field_name, _ty) in fields.iter() {
                writeln!(output, "this.{field_name} = {field_name};");
            }
        });
        writeln!(output);

        // Generate default constructor (if the one above is not already parameterless).
        if !fields.is_empty() {
            writeln!(output, "public {name}()");
            indented_block(output, |output| {
                for ((field_name, _ty), field) in fields.iter().zip(&*product_type.elements) {
                    if let Some(default) = default_init(ctx, &field.algebraic_type) {
                        writeln!(output, "this.{field_name} = {default};");
                    }
                }
            });
            writeln!(output);
        }

        // If this is a table, we want to generate event accessor and indexes
        if let Some(schema) = &schema {
            let constraints = schema.column_constraints();
            let mut unique_indexes = Vec::new();
            // Declare custom index dictionaries
            for col in schema.columns() {
                let field_name = col.col_name.replace("r#", "").to_case(Case::Pascal);
                if !constraints[&ColList::new(col.col_pos)].has_unique() {
                    continue;
                }
                let type_name = ty_fmt(ctx, &col.col_type, namespace);
                writeln!(
                    output,
                    "private static Dictionary<{type_name}, {name}> {field_name}_Index = new(16);"
                );
                unique_indexes.push(field_name);
            }
            if !unique_indexes.is_empty() {
                writeln!(output);
                // OnInsert method for updating indexes
                writeln!(output, "public override void InternalOnValueInserted()");
                indented_block(output, |output| {
                    for col in schema.columns() {
                        let field_name = col.col_name.replace("r#", "").to_case(Case::Pascal);
                        if !constraints[&ColList::new(col.col_pos)].has_unique() {
                            continue;
                        }
                        writeln!(output, "{field_name}_Index[{field_name}] = this;");
                    }
                });
                writeln!(output);
                // OnDelete method for updating indexes
                writeln!(output, "public override void InternalOnValueDeleted()");
                indented_block(output, |output| {
                    for col in schema.columns() {
                        let field_name = col.col_name.replace("r#", "").to_case(Case::Pascal);
                        if !constraints[&ColList::new(col.col_pos)].has_unique() {
                            continue;
                        }
                        writeln!(output, "{field_name}_Index.Remove({field_name});");
                    }
                });
                writeln!(output);
            }

            // If this is a table, we want to include functions for accessing the table data
            // Insert the funcs for accessing this struct
            autogen_csharp_access_funcs_for_struct(output, name, product_type, name, schema);
            writeln!(output);
        }
    });

    output.into_inner()
}

fn indented_block<R>(output: &mut CodeIndenter<String>, f: impl FnOnce(&mut CodeIndenter<String>) -> R) -> R {
    writeln!(output, "{{");
    let res = f(&mut output.indented(1));
    writeln!(output, "}}");
    res
}

fn autogen_csharp_access_funcs_for_struct(
    output: &mut CodeIndenter<String>,
    struct_name_pascal_case: &str,
    product_type: &ProductType,
    _table_name: &str,
    schema: &TableSchema,
) {
    let constraints = schema.column_constraints();
    for col in schema.columns() {
        let is_unique = constraints[&ColList::new(col.col_pos)].has_unique();

        let field = &product_type.elements[col.col_pos.idx()];
        let field_name = field.name.as_ref().expect("autogen'd tuples should have field names");
        let field_type = &field.algebraic_type;
        let csharp_field_name_pascal = field_name.replace("r#", "").to_case(Case::Pascal);

        let csharp_field_type = match field_type {
            AlgebraicType::Product(product) => {
                if product.is_identity() {
                    "SpacetimeDB.Identity"
                } else if product.is_address() {
                    "SpacetimeDB.Address"
                } else {
                    continue;
                }
            }
            AlgebraicType::Sum(_) | AlgebraicType::Ref(_) => continue,
            ty => match scalar_or_string_name(ty) {
                Some(ty) => ty,
                _ => continue,
            },
        };

        if is_unique {
            writeln!(
                output,
                "public static {struct_name_pascal_case}? FindBy{csharp_field_name_pascal}({csharp_field_type} value)"
            );
            indented_block(output, |output| {
                writeln!(
                    output,
                    "{csharp_field_name_pascal}_Index.TryGetValue(value, out var r);"
                );
                writeln!(output, "return r;");
            });
            writeln!(output);
        }

        writeln!(
            output,
            "public static IEnumerable<{struct_name_pascal_case}> FilterBy{csharp_field_name_pascal}({csharp_field_type} value)"
        );
        indented_block(output, |output| {
            if is_unique {
                // Yield a single item iff `FindBy` returns a non-null record.
                writeln!(output, "if (FindBy{csharp_field_name_pascal}(value) is {{}} found)");
                indented_block(output, |output| {
                    writeln!(output, "yield return found;");
                });
            } else {
                writeln!(output, "return Query(x => x.{csharp_field_name_pascal} == value);");
            }
        });
        writeln!(output);
    }

    if let Some(primary_col_index) = schema.pk() {
        writeln!(
            output,
            "public override object GetPrimaryKeyValue() => {col_name_pascal_case};",
            col_name_pascal_case = primary_col_index.col_name.replace("r#", "").to_case(Case::Pascal)
        );
    }
}

pub fn autogen_csharp_reducer(ctx: &GenCtx, reducer: &ReducerDef, namespace: &str) -> String {
    let func_name = &*reducer.name;
    // let reducer_pascal_name = func_name.to_case(Case::Pascal);
    let func_name_pascal_case = func_name.to_case(Case::Pascal);

    let mut output = CsharpAutogen::new(namespace, &[]);

    //Args struct
    writeln!(output, "[SpacetimeDB.Type]");
    writeln!(
        output,
        "public partial class {func_name_pascal_case}ArgsStruct : IReducerArgs"
    );

    let mut func_params: String = String::new();
    let mut field_inits: String = String::new();

    indented_block(&mut output, |output| {
        writeln!(
            output,
            "ReducerType IReducerArgs.ReducerType => ReducerType.{func_name_pascal_case};"
        );
        writeln!(output, "string IReducerArgsBase.ReducerName => \"{func_name}\";");
        writeln!(output, "bool IReducerArgs.InvokeHandler(ReducerEvent reducerEvent) => Reducer.On{func_name_pascal_case}(reducerEvent, this);");
        if !reducer.args.is_empty() {
            writeln!(output);
        }
        for (arg_i, arg) in reducer.args.iter().enumerate() {
            let name = arg
                .name
                .as_deref()
                .unwrap_or_else(|| panic!("reducer args should have names: {func_name}"));
            let arg_name = name.to_case(Case::Camel);
            let field_name = name.to_case(Case::Pascal);
            let arg_type_str = ty_fmt(ctx, &arg.algebraic_type, namespace);

            if arg_i != 0 {
                func_params.push_str(", ");
                field_inits.push_str(", ");
            }
            write!(output, "public {arg_type_str} {field_name}");
            // Skip default initializer if it's the same as the implicit default.
            if let Some(default) = default_init(ctx, &arg.algebraic_type) {
                write!(output, " = {default}");
            }
            writeln!(output, ";");
            write!(func_params, "{arg_type_str} {arg_name}").unwrap();
            write!(field_inits, "{field_name} = {arg_name}").unwrap();
        }
    });

    writeln!(output);

    writeln!(output, "public static partial class Reducer");
    indented_block(&mut output, |output| {
        let delegate_separator = if !reducer.args.is_empty() { ", " } else { "" };
        writeln!(
            output,
            "public delegate void {func_name_pascal_case}Handler(ReducerEvent reducerEvent{delegate_separator}{func_params});"
        );
        writeln!(
            output,
            "public static event {func_name_pascal_case}Handler? On{func_name_pascal_case}Event;"
        );

        writeln!(output);

        writeln!(output, "public static void {func_name_pascal_case}({func_params})");
        indented_block(output, |output| {
            writeln!(
                output,
                "SpacetimeDBClient.instance.InternalCallReducer(new {func_name_pascal_case}ArgsStruct {{ {field_inits} }});"
            );
        });
        writeln!(output);

        writeln!(
            output,
            "public static bool On{func_name_pascal_case}(ReducerEvent reducerEvent, {func_name_pascal_case}ArgsStruct args)"
        );
        indented_block(output, |output| {
            writeln!(output, "if (On{func_name_pascal_case}Event == null) return false;");
            writeln!(output, "On{func_name_pascal_case}Event(");
            // Write out arguments one per line
            {
                indent_scope!(output);
                write!(output, "reducerEvent");
                for (i, arg) in reducer.args.iter().enumerate() {
                    writeln!(output, ",");
                    let arg_name = arg
                        .name
                        .as_deref()
                        .map_or_else(|| format!("Arg{i}"), |name| name.to_case(Case::Pascal));
                    write!(output, "args.{arg_name}");
                }
                writeln!(output);
            }
            writeln!(output, ");");
            writeln!(output, "return true;");
        });
    });
    writeln!(output);

    output.into_inner()
}

pub fn autogen_csharp_globals(ctx: &GenCtx, items: &[GenItem], namespace: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();

    let reducers: Vec<&ReducerDef> = items
        .iter()
        .filter_map(|i| {
            if let GenItem::Reducer(reducer) = i {
                Some(reducer)
            } else {
                None
            }
        })
        .collect();
    let reducer_names: Vec<String> = reducers
        .iter()
        .map(|reducer| reducer.name.deref().to_case(Case::Pascal))
        .collect();

    let mut output = CsharpAutogen::new(namespace, &["SpacetimeDB.ClientApi"]);

    writeln!(output, "public enum ReducerType");
    indented_block(&mut output, |output| {
        writeln!(output, "None,");
        for reducer_name in &reducer_names {
            writeln!(output, "{reducer_name},");
        }
    });
    writeln!(output);

    writeln!(output, "public interface IReducerArgs : IReducerArgsBase");
    indented_block(&mut output, |output| {
        writeln!(output, "ReducerType ReducerType {{ get; }}");
        writeln!(output, "bool InvokeHandler(ReducerEvent reducerEvent);");
    });
    writeln!(output);

    writeln!(output, "public partial class ReducerEvent : ReducerEventBase");
    indented_block(&mut output, |output| {
        writeln!(output, "public IReducerArgs? Args {{ get; }}");
        writeln!(output);
        writeln!(output, "public string ReducerName => Args?.ReducerName ?? \"<none>\";");
        writeln!(output);
        writeln!(
            output,
            r#"[Obsolete("ReducerType is deprecated, please match directly on type of .Args instead.")]"#
        );
        writeln!(
            output,
            "public ReducerType Reducer => Args?.ReducerType ?? ReducerType.None;"
        );
        writeln!(output);
        writeln!(
            output,
            "public ReducerEvent(IReducerArgs? args) : base() => Args = args;"
        );
        writeln!(
            output,
            "public ReducerEvent(TransactionUpdate update, IReducerArgs? args) : base(update) => Args = args;"
        );
        writeln!(output);
        // Properties for reducer args
        for reducer_name in &reducer_names {
            writeln!(
                output,
                r#"[Obsolete("Accessors that implicitly cast `Args` are deprecated, please match `Args` against the desired type explicitly instead.")]"#
            );
            writeln!(
                output,
                "public {reducer_name}ArgsStruct {reducer_name}Args => ({reducer_name}ArgsStruct)Args!;"
            );
        }
        writeln!(output);
        // Event handlers.
        writeln!(
            output,
            "public override bool InvokeHandler() => Args?.InvokeHandler(this) ?? false;"
        );
    });
    writeln!(output);

    writeln!(
        output,
        "public class SpacetimeDBClient : SpacetimeDBClientBase<ReducerEvent>"
    );
    indented_block(&mut output, |output| {
        writeln!(output, "protected SpacetimeDBClient()");
        indented_block(output, |output| {
            for item in items {
                if let GenItem::Table(table) = item {
                    writeln!(
                        output,
                        "clientDB.AddTable<{table_name}>();",
                        table_name = csharp_typename(ctx, table.data)
                    );
                }
            }
        });
        writeln!(output);

        writeln!(output, "public static readonly SpacetimeDBClient instance = new();");
        writeln!(output);

        writeln!(
            output,
            "protected override ReducerEvent ReducerEventFromDbEvent(TransactionUpdate update)"
        );
        indented_block(output, |output| {
            writeln!(output, "var encodedArgs = update.ReducerCall.Args;");
            writeln!(output, "IReducerArgs? args = update.ReducerCall.ReducerName switch {{");
            {
                indent_scope!(output);
                for (reducer, reducer_name) in std::iter::zip(&reducers, &reducer_names) {
                    let reducer_str_name = &reducer.name;
                    writeln!(
                        output,
                        "\"{reducer_str_name}\" => BSATNHelpers.Decode<{reducer_name}ArgsStruct>(encodedArgs),"
                    );
                }
                writeln!(output, "\"<none>\" => null,");
                writeln!(output, "\"__identity_connected__\" => null,");
                writeln!(output, "\"__identity_disconnected__\" => null,");
                writeln!(output, "\"\" => null,"); //Transaction from CLI command
                writeln!(
                    output,
                    r#"var reducer => throw new ArgumentOutOfRangeException("Reducer", $"Unknown reducer {{reducer}}")"#
                );
            }
            writeln!(output, "}};");
            writeln!(output, "return new ReducerEvent(update, args);");
        });
    });

    results.push(("_Globals/SpacetimeDBClient.cs".to_owned(), output.into_inner()));

    // Note: Unity requires script classes to have the same name as the file they are in.
    // That's why we're generating a separate file for Unity-specific code.

    let mut output = CsharpAutogen::new(namespace, &[]);

    writeln!(output, "// This class is only used in Unity projects.");
    writeln!(
        output,
        "// Attach this to a gameobject in your scene to use SpacetimeDB."
    );
    writeln!(output, "#if UNITY_5_3_OR_NEWER");
    writeln!(output, "public class UnityNetworkManager : UnityEngine.MonoBehaviour");
    indented_block(&mut output, |output| {
        writeln!(
            output,
            "private void OnDestroy() => SpacetimeDBClient.instance.Close();"
        );
        writeln!(output, "private void Update() => SpacetimeDBClient.instance.Update();");
    });
    writeln!(output, "#endif");

    results.push(("_Globals/UnityNetworkManager.cs".to_owned(), output.into_inner()));

    results
}
