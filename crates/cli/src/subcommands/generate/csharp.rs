// Note: the generated code depends on APIs and interfaces from crates/bindings-csharp/BSATN.Runtime.
use super::util::fmt_fn;

use std::fmt::{self, Write};
use std::ops::Deref;

use convert_case::{Case, Casing};
use spacetimedb_lib::sats::{AlgebraicType, AlgebraicTypeRef, ArrayType, ProductType, SumType};
use spacetimedb_lib::ReducerDef;
use spacetimedb_primitives::ColList;
use spacetimedb_schema::def::{BTreeAlgorithm, IndexAlgorithm};
use spacetimedb_schema::schema::TableSchema;

use super::code_indenter::CodeIndenter;
use super::{GenCtx, GenItem, TableDescHack};

const INDENT: &str = "\t";

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
        AlgebraicType::Ref(_) | AlgebraicType::Sum(_) | AlgebraicType::Product(_) | AlgebraicType::Array(_) => {
            return None
        }
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
        AlgebraicType::Array(ArrayType { elem_ty }) => {
            write!(
                f,
                "System.Collections.Generic.List<{}>",
                ty_fmt(ctx, elem_ty, namespace)
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
        // For product types and arrays, we can use the default constructor.
        AlgebraicType::Product(_) | AlgebraicType::Array(_) => Some("new()"),
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
        let mut output = CodeIndenter::new(String::new(), INDENT);

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
pub fn autogen_csharp_table(ctx: &GenCtx, table: &TableDescHack, namespace: &str) -> String {
    let tuple = ctx.typespace[table.data].as_product().unwrap();
    autogen_csharp_product_table_common(
        ctx,
        csharp_typename(ctx, table.data),
        tuple,
        Some(table.schema.clone()),
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
        &["System.Collections.Generic", "System.Runtime.Serialization"],
    );

    writeln!(output, "[SpacetimeDB.Type]");
    writeln!(output, "[DataContract]");
    write!(output, "public partial class {name}");
    if schema.is_some() {
        write!(output, " : IDatabaseRow");
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
    });

    output.into_inner()
}

fn indented_block<R>(output: &mut CodeIndenter<String>, f: impl FnOnce(&mut CodeIndenter<String>) -> R) -> R {
    writeln!(output, "{{");
    let res = f(&mut output.indented(1));
    writeln!(output, "}}");
    res
}

fn csharp_field_type(field_type: &AlgebraicType) -> Option<&str> {
    match field_type {
        AlgebraicType::Product(product) => {
            if product.is_identity() {
                Some("SpacetimeDB.Identity")
            } else if product.is_address() {
                Some("SpacetimeDB.Address")
            } else {
                None
            }
        }
        AlgebraicType::Sum(_) | AlgebraicType::Ref(_) => None,
        ty => match scalar_or_string_name(ty) {
            Some(ty) => Some(ty),
            _ => None,
        },
    }
}

fn autogen_csharp_access_funcs_for_struct(
    output: &mut CodeIndenter<String>,
    struct_name_pascal_case: &str,
    product_type: &ProductType,
    table_name: &str,
    schema: &TableSchema,
    btrees: &Vec<BTreeIndex>
) {
    let csharp_table_name = table_name.to_case(Case::Pascal);
    let constraints = schema.backcompat_column_constraints();
    for col in schema.columns() {
        if !constraints[&ColList::new(col.col_pos)].has_unique() {
            continue;
        }

        let field = &product_type.elements[col.col_pos.idx()];
        let field_name = field.name.as_ref().expect("autogen'd tuples should have field names");
        let field_type = &field.algebraic_type;
        let csharp_field_name_pascal = field_name.replace("r#", "").to_case(Case::Pascal);
        let csharp_field_type = match csharp_field_type(field_type) {
            None => continue,
            Some(x) => x,
        };
        writeln!(output, "public class {csharp_field_name_pascal}UniqueIndex");
        indented_block(output, |output| {
            write!(
                output,
                "internal readonly Dictionary<{csharp_field_type}, {struct_name_pascal_case}> Cache = new(16);"
            );
            writeln!(output);

            writeln!(
                output,
                "public {struct_name_pascal_case}? Find({csharp_field_type} value)"
            );
            indented_block(output, |output| {
                writeln!(output, "Cache.TryGetValue(value, out var r);");
                writeln!(output, "return r;");
            });
            writeln!(output);
        });
        writeln!(output);
        writeln!(
            output,
            "public {csharp_field_name_pascal}UniqueIndex {csharp_field_name_pascal} = new();"
        );
    }

    for index in btrees {
        let fields: Vec<BTreeColumn<'_>> = index.columns
            .iter()
            .map(|col| {
                let field = &product_type.elements[col.idx()];
                let field_type = &field.algebraic_type;
                match csharp_field_type(field_type) {
                    None => todo!(),
                    Some(x) => {
                        let field_name = field.name.as_ref().expect("autogen'd tuples should have field names");
                        let csharp_field_name_pascal = field_name.replace("r#", "").to_case(Case::Pascal);
                        let range = match field_type {
                            AlgebraicType::Product(pt) =>
                                if pt.elements[0].name == Some("__address_bytes".into()) { ("Address.MinValue", "Address.MaxValue") }
                                else if pt.elements[0].name == Some("__identity_bytes".into()) { ("Identity.MinValue", "Identity.MaxValue") }
                                else { todo!() },
                            AlgebraicType::String => ("\"\"", "\"\\uFFFF\\uFFFF\""),
                            AlgebraicType::Bool => ("false", "true"),
                            AlgebraicType::I8 => ("sbyte.MinValue", "sbyte.MaxValue"),
                            AlgebraicType::U8 => ("byte.MinValue", "byte.MaxValue"),
                            AlgebraicType::I16 => ("short.MinValue", "short.MaxValue"),
                            AlgebraicType::U16 => ("ushort.MinValue", "ushort.MaxValue"),
                            AlgebraicType::I32 => ("int.MinValue", "int.MaxValue"),
                            AlgebraicType::U32 => ("uint.MinValue", "uint.MaxValue"),
                            AlgebraicType::I64 => ("long.MinValue", "long.MaxValue"),
                            AlgebraicType::U64 => ("ulong.MinValue", "ulong.MaxValue"),
                            AlgebraicType::I128 => ("I128.MinValue", "I128.MaxValue"),
                            AlgebraicType::U128 => ("U128.MinValue", "U128.MaxValue"),
                            AlgebraicType::I256 => ("I256.MinValue", "I256.MaxValue"),
                            AlgebraicType::U256 => ("U256.MinValue", "U256.MaxValue"),
                            AlgebraicType::F32 => ("float.MinValue", "float.MaxValue"),
                            AlgebraicType::F64 => ("double.MinValue", "double.MaxValue"),
                            _ => todo!(),
                        };
                        BTreeColumn { field_type, csharp_field_name_pascal, csharp_field_type: x, range }
                    },
                }
            })
            .collect();

        writeln!(output);
        writeln!(output, "sealed class {0}Comparer : IComparer<{struct_name_pascal_case}>", index.csharp_index_name);
        indented_block(output, |output| {
            writeln!(output, "public int Compare({struct_name_pascal_case}? a, {struct_name_pascal_case}? b)");
            indented_block(output, |output| {
                writeln!(output, "if (a == null || b == null) return -1;");
                for field in &fields {
                    match field.field_type {
                        AlgebraicType::Product(_) => {
                            writeln!(output, "var {0} = a.{0}.CompareTo(b.{0});", field.csharp_field_name_pascal);
                            writeln!(output, "if ({0} != 0) return {0};", field.csharp_field_name_pascal);
                        },
                        AlgebraicType::String => {
                            writeln!(output, "var {0} = String.Compare(a.{0}, b.{0}, StringComparison.InvariantCulture);", field.csharp_field_name_pascal);
                            writeln!(output, "if ({0} != 0) return {0};", field.csharp_field_name_pascal);
                        },
                        _ => {
                            writeln!(output, "if (a.{0} < b.{0}) return -1;", field.csharp_field_name_pascal);
                            writeln!(output, "if (a.{0} > b.{0}) return 1;", field.csharp_field_name_pascal);
                        },
                    }
                }
                writeln!(output, "return 0;")
            });
        });
        writeln!(output, "SortedSet<{struct_name_pascal_case}> {0}_BTree = new(new {0}Comparer());", index.csharp_index_name);
        writeln!(output);
        
        writeln!(output, "public sealed class {0}Index", index.csharp_index_name);
        indented_block(output, |output| {
            writeln!(output, "{csharp_table_name}Handle Handle;");
            writeln!(
                output,
                "internal {0}Index({csharp_table_name}Handle handle) => Handle = handle;", index.csharp_index_name
            );
            writeln!(output);

            writeln!(output, "IEnumerable<{struct_name_pascal_case}> DoFilter() =>");
            {
                indent_scope!(output);
                writeln!(output, "Handle.{0}_BTree.GetViewBetween(Handle.__Min, Handle.__Max);", index.csharp_index_name);
            }

            for i in 0..fields.len() {
                writeln!(output);
                write!(output, "public IEnumerable<{struct_name_pascal_case}> Filter(");
                for j in 0..i + 1 {
                    if j != 0 {
                        write!(output, ", ");
                    }
                    write!(output, "{} {}", fields[j].csharp_field_type, fields[j].csharp_field_name_pascal);
                }
                writeln!(output, ")");
                indented_block(output, |output| {
                    for j in 0..i + 1 {
                        writeln!(output, "Handle.__Min.{0} = {0};", fields[j].csharp_field_name_pascal);
                        writeln!(output, "Handle.__Max.{0} = {0};", fields[j].csharp_field_name_pascal);
                    }
                    for j in i + 1..fields.len() {
                        writeln!(output, "Handle.__Min.{0} = {1};", fields[j].csharp_field_name_pascal, fields[j].range.0);
                        writeln!(output, "Handle.__Max.{0} = {1};", fields[j].csharp_field_name_pascal, fields[j].range.1);
                    }
                    writeln!(output, "return DoFilter();");
                });

                writeln!(output);
                write!(output, "public IEnumerable<{struct_name_pascal_case}> Filter(");
                for j in 0..i {
                    if j != 0 {
                        write!(output, ", ");
                    }
                    write!(output, "{} {}", fields[j].csharp_field_type, fields[j].csharp_field_name_pascal);
                }
                if i != 0 {
                    write!(output, ", ");
                }
                write!(output, "({0} min, {0} max) {1}", fields[i].csharp_field_type, fields[i].csharp_field_name_pascal);
                writeln!(output, ")");
                indented_block(output, |output| {
                    for j in 0..i {
                        writeln!(output, "Handle.__Min.{0} = {0};", fields[j].csharp_field_name_pascal);
                        writeln!(output, "Handle.__Max.{0} = {0};", fields[j].csharp_field_name_pascal);
                    }
                    writeln!(output, "Handle.__Min.{0} = {0}.min;",  fields[i].csharp_field_name_pascal);
                    writeln!(output, "Handle.__Max.{0} = {0}.max;",  fields[i].csharp_field_name_pascal);
                    for j in i + 1..fields.len() {
                        writeln!(output, "Handle.__Min.{0} = {1};", fields[j].csharp_field_name_pascal, fields[j].range.0);
                        writeln!(output, "Handle.__Max.{0} = {1};", fields[j].csharp_field_name_pascal, fields[j].range.1);
                    }
                    writeln!(output, "return DoFilter();");
                });
            }
        });
        writeln!(output);
        writeln!(
            output,
            "public {0}Index {0} {{ get; init; }}", index.csharp_index_name
        );
    }

    writeln!(output, "internal {csharp_table_name}Handle()");
    indented_block(output, |output| {
        for index in btrees {
            writeln!(output, "{0} = new(this);", index.csharp_index_name);
        }
    });

    if let Some(primary_col_index) = schema.pk() {
        writeln!(
            output,
            "public override object GetPrimaryKey(IDatabaseRow row) => (({struct_name_pascal_case})row).{col_name_pascal_case};",
            col_name_pascal_case = primary_col_index.col_name.replace("r#", "").to_case(Case::Pascal)
        );
    }
}

pub fn autogen_csharp_reducer(ctx: &GenCtx, reducer: &ReducerDef, namespace: &str) -> String {
    let func_name = &*reducer.name;
    let func_name_pascal_case = func_name.to_case(Case::Pascal);

    let mut output = CsharpAutogen::new(namespace, &[]);

    //Args struct
    writeln!(output, "[SpacetimeDB.Type]");
    writeln!(output, "public partial class {func_name_pascal_case} : IReducerArgs");
    indented_block(&mut output, |output| {
        writeln!(output, "string IReducerArgs.ReducerName => \"{func_name}\";");
        if !reducer.args.is_empty() {
            writeln!(output);
        }
        for arg in reducer.args.iter() {
            let name = arg
                .name
                .as_deref()
                .unwrap_or_else(|| panic!("reducer args should have names: {func_name}"));
            let arg_type_str = ty_fmt(ctx, &arg.algebraic_type, namespace);
            let field_name = name.to_case(Case::Pascal);

            write!(output, "public {arg_type_str} {field_name}");
            // Skip default initializer if it's the same as the implicit default.
            if let Some(default) = default_init(ctx, &arg.algebraic_type) {
                write!(output, " = {default}");
            }
            writeln!(output, ";");
        }
    });

    output.into_inner()
}

struct BTreeIndex {
    csharp_index_name: String,
    columns: ColList,
}

struct BTreeColumn<'a> {
    field_type: &'a AlgebraicType,
    csharp_field_name_pascal: String,
    csharp_field_type: &'a str,
    range: (&'static str, &'static str),
}

pub fn autogen_csharp_globals(ctx: &GenCtx, items: &[GenItem], namespace: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();

    let tables = items.iter().filter_map(|i| {
        if let GenItem::Table(table) = i {
            Some(table)
        } else {
            None
        }
    });

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

    let mut output = CsharpAutogen::new(namespace, &["SpacetimeDB.ClientApi", "System.Collections.Generic"]);

    writeln!(output, "public sealed class RemoteTables");
    indented_block(&mut output, |output| {
        for table in tables {
            let schema = &table.schema;
            let constraints = schema.backcompat_column_constraints();
            let table_name = &schema.table_name;
            let csharp_name = table_name.as_ref().to_case(Case::Pascal);
            let table_type = csharp_typename(ctx, table.data);

            let btrees: Vec<_> = schema.indexes
                .clone()
                .into_iter()
                .map(|i| match i.index_algorithm {
                    IndexAlgorithm::BTree(BTreeAlgorithm { columns }) => {
                        let col_pos = columns.head().unwrap().idx();
                        if constraints[&ColList::new(col_pos.into())].has_unique() {
                            None
                        }
                        else {
                            let csharp_index_name = (&i.index_name[table_name.len() + 7..]).to_case(Case::Pascal);
                            Some(BTreeIndex { csharp_index_name, columns: columns.clone() })
                       }
                    },
                    _ => None,
                })
                .filter(Option::is_some)
                .map(|x| x.unwrap())
                .collect();

            writeln!(
                output,
                "public class {csharp_name}Handle : RemoteTableHandle<EventContext, {table_type}>"
            );
            indented_block(output, |output| {
                // If this is a table, we want to generate event accessor and indexes
                let mut unique_indexes = Vec::new();

                // Declare custom index dictionaries
                for col in schema.columns() {
                    let field_name = col.col_name.replace("r#", "").to_case(Case::Pascal);
                    if !constraints[&ColList::new(col.col_pos)].has_unique() {
                        continue;
                    }
                    unique_indexes.push(field_name);
                }

                if !btrees.is_empty() {
                    writeln!(output, "{csharp_name} __Min = new();");
                    writeln!(output, "{csharp_name} __Max = new();");
                }

                if !unique_indexes.is_empty() || !btrees.is_empty() {
                    // OnInsert method for updating indexes
                    writeln!(output);
                    writeln!(
                        output,
                        "public override void InternalInvokeValueInserted(IDatabaseRow row)"
                    );
                    indented_block(output, |output| {
                        writeln!(output, "var value = ({table_type})row;");
                        for col in schema.columns() {
                            let field_name = col.col_name.replace("r#", "").to_case(Case::Pascal);
                            if !constraints[&ColList::new(col.col_pos)].has_unique() {
                                continue;
                            }
                            writeln!(output, "{field_name}.Cache[value.{field_name}] = value;");
                        }
                        for btree in &btrees {
                            writeln!(output, "{0}_BTree.Add(value);", btree.csharp_index_name);
                        }
                    });

                    // OnDelete method for updating indexes
                    writeln!(output);
                    writeln!(
                        output,
                        "public override void InternalInvokeValueDeleted(IDatabaseRow row)"
                    );
                    indented_block(output, |output| {
                        writeln!(output, "var value = ({table_type})row;");
                        for col in schema.columns() {
                            let field_name = col.col_name.replace("r#", "").to_case(Case::Pascal);
                            if !constraints[&ColList::new(col.col_pos)].has_unique() {
                                continue;
                            }
                            writeln!(output, "{field_name}.Cache.Remove((({table_type})row).{field_name});");
                        }
                    });
                }

                // If this is a table, we want to include functions for accessing the table data
                // Insert the funcs for accessing this struct
                let product_type = ctx.typespace[table.data].as_product().unwrap();
                autogen_csharp_access_funcs_for_struct(output, table_type, product_type, table_name, schema, &btrees);
            });
            writeln!(output);
            writeln!(output, "public readonly {csharp_name}Handle {csharp_name} = new();");
            writeln!(output);
        }
    });
    writeln!(output);

    writeln!(output, "public sealed class RemoteReducers : RemoteBase<DbConnection>");
    indented_block(&mut output, |output| {
        writeln!(
            output,
            "internal RemoteReducers(DbConnection conn, SetReducerFlags SetReducerFlags) : base(conn) {{ this.SetCallReducerFlags = SetReducerFlags; }}"
        );
        writeln!(output, "internal readonly SetReducerFlags SetCallReducerFlags;");

        for reducer in &reducers {
            let func_name = &*reducer.name;
            let func_name_pascal_case = func_name.to_case(Case::Pascal);
            let delegate_separator = if !reducer.args.is_empty() { ", " } else { "" };

            let mut func_params: String = String::new();
            let mut field_inits: String = String::new();

            for (arg_i, arg) in reducer.args.iter().enumerate() {
                if arg_i != 0 {
                    func_params.push_str(", ");
                    field_inits.push_str(", ");
                }

                let name = arg
                    .name
                    .as_deref()
                    .unwrap_or_else(|| panic!("reducer args should have names: {func_name}"));
                let arg_type_str = ty_fmt(ctx, &arg.algebraic_type, namespace);
                let arg_name = name.to_case(Case::Camel);
                let field_name = name.to_case(Case::Pascal);

                write!(func_params, "{arg_type_str} {arg_name}").unwrap();
                write!(field_inits, "{field_name} = {arg_name}").unwrap();
            }

            writeln!(
                output,
                "public delegate void {func_name_pascal_case}Handler(EventContext ctx{delegate_separator}{func_params});"
            );
            writeln!(
                output,
                "public event {func_name_pascal_case}Handler? On{func_name_pascal_case};"
            );
            writeln!(output);

            writeln!(output, "public void {func_name_pascal_case}({func_params})");
            indented_block(output, |output| {
                writeln!(
                    output,
                    "conn.InternalCallReducer(new {func_name_pascal_case} {{ {field_inits} }}, this.SetCallReducerFlags.{func_name_pascal_case}Flags);"
                );
            });
            writeln!(output);

            writeln!(
                output,
                "public bool Invoke{func_name_pascal_case}(EventContext ctx, {func_name_pascal_case} args)"
            );
            indented_block(output, |output| {
                writeln!(output, "if (On{func_name_pascal_case} == null) return false;");
                writeln!(output, "On{func_name_pascal_case}(");
                // Write out arguments one per line
                {
                    indent_scope!(output);
                    write!(output, "ctx");
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
        }
    });
    writeln!(output);

    writeln!(output, "public sealed class SetReducerFlags");
    indented_block(&mut output, |output| {
        writeln!(output, "internal SetReducerFlags() {{ }}");
        for reducer in &reducers {
            let func_name = &*reducer.name;
            let func_name_pascal_case = func_name.to_case(Case::Pascal);
            writeln!(output, "internal CallReducerFlags {func_name_pascal_case}Flags;");
            writeln!(output, "public void {func_name_pascal_case}(CallReducerFlags flags) {{ this.{func_name_pascal_case}Flags = flags; }}");
        }
    });
    writeln!(output);

    writeln!(
        output,
        "public partial record EventContext : DbContext<RemoteTables>, IEventContext"
    );
    indented_block(&mut output, |output| {
        writeln!(output, "public readonly RemoteReducers Reducers;");
        writeln!(output, "public readonly SetReducerFlags SetReducerFlags;");
        writeln!(output, "public readonly Event<Reducer> Event;");
        writeln!(output);
        writeln!(
            output,
            "internal EventContext(DbConnection conn, Event<Reducer> reducerEvent) : base(conn.Db)"
        );
        indented_block(output, |output| {
            writeln!(output, "Reducers = conn.Reducers;");
            writeln!(output, "SetReducerFlags = conn.SetReducerFlags;");
            writeln!(output, "Event = reducerEvent;");
        });
    });
    writeln!(output);

    writeln!(output, "[Type]");
    writeln!(output, "public partial record Reducer : TaggedEnum<(");
    {
        indent_scope!(output);
        for reducer_name in &reducer_names {
            writeln!(output, "{reducer_name} {reducer_name},");
        }
        writeln!(output, "Unit StdbNone,");
        writeln!(output, "Unit StdbIdentityConnected,");
        writeln!(output, "Unit StdbIdentityDisconnected");
    }
    writeln!(output, ")>;");
    writeln!(output);

    writeln!(
        output,
        "public class DbConnection : DbConnectionBase<DbConnection, Reducer>"
    );
    indented_block(&mut output, |output| {
        writeln!(output, "public readonly RemoteTables Db = new();");
        writeln!(output, "public readonly RemoteReducers Reducers;");
        writeln!(output, "public readonly SetReducerFlags SetReducerFlags;");
        writeln!(output);

        writeln!(output, "public DbConnection()");
        indented_block(output, |output| {
            writeln!(output, "SetReducerFlags = new();");
            writeln!(output, "Reducers = new(this, this.SetReducerFlags);");
            writeln!(output);

            for item in items {
                if let GenItem::Table(table) = item {
                    writeln!(
                        output,
                        "clientDB.AddTable<{table_type}>(\"{table_name}\", Db.{csharp_table_name});",
                        table_type = csharp_typename(ctx, table.data),
                        table_name = table.schema.table_name,
                        csharp_table_name = table.schema.table_name.as_ref().to_case(Case::Pascal)
                    );
                }
            }
        });
        writeln!(output);

        writeln!(output, "protected override Reducer ToReducer(TransactionUpdate update)");
        indented_block(output, |output| {
            writeln!(output, "var encodedArgs = update.ReducerCall.Args;");
            writeln!(output, "return update.ReducerCall.ReducerName switch {{");
            {
                indent_scope!(output);
                for (reducer, reducer_name) in std::iter::zip(&reducers, &reducer_names) {
                    let reducer_str_name = &reducer.name;
                    writeln!(
                        output,
                        "\"{reducer_str_name}\" => new Reducer.{reducer_name}(BSATNHelpers.Decode<{reducer_name}>(encodedArgs)),"
                    );
                }
                writeln!(output, "\"<none>\" => new Reducer.StdbNone(default),");
                writeln!(
                    output,
                    "\"__identity_connected__\" => new Reducer.StdbIdentityConnected(default),"
                );
                writeln!(
                    output,
                    "\"__identity_disconnected__\" => new Reducer.StdbIdentityDisconnected(default),"
                );
                writeln!(output, "\"\" => new Reducer.StdbNone(default),"); //Transaction from CLI command
                writeln!(
                    output,
                    r#"var reducer => throw new ArgumentOutOfRangeException("Reducer", $"Unknown reducer {{reducer}}")"#
                );
            }
            writeln!(output, "}};");
        });
        writeln!(output);

        writeln!(
            output,
            "protected override IEventContext ToEventContext(Event<Reducer> reducerEvent) =>"
        );
        writeln!(output, "new EventContext(this, reducerEvent);");
        writeln!(output);

        writeln!(
            output,
            "protected override bool Dispatch(IEventContext context, Reducer reducer)"
        );
        indented_block(output, |output| {
            writeln!(output, "var eventContext = (EventContext)context;");
            writeln!(output, "return reducer switch {{");
            {
                indent_scope!(output);
                for reducer_name in &reducer_names {
                    writeln!(
                        output,
                        "Reducer.{reducer_name}(var args) => Reducers.Invoke{reducer_name}(eventContext, args),"
                    );
                }
                writeln!(output, "Reducer.StdbNone or");
                writeln!(output, "Reducer.StdbIdentityConnected or");
                writeln!(output, "Reducer.StdbIdentityDisconnected => true,");
                writeln!(
                    output,
                    r#"_ => throw new ArgumentOutOfRangeException("Reducer", $"Unknown reducer {{reducer}}")"#
                );
            }
            writeln!(output, "}};");
        });
        writeln!(output);

        writeln!(
            output,
            "public SubscriptionBuilder<EventContext> SubscriptionBuilder() => new(this);"
        );
    });

    results.push(("_Globals/SpacetimeDBClient.cs".to_owned(), output.into_inner()));
    results
}
