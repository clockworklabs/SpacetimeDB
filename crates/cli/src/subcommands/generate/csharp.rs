use super::util::fmt_fn;

use std::fmt::{self, Write};
use std::ops::Deref;

use convert_case::{Case, Casing};
use spacetimedb_lib::sats::{AlgebraicType, AlgebraicTypeRef, ArrayType, ProductType, SumType};
use spacetimedb_lib::{ColumnIndexAttribute, ProductTypeElement, ReducerDef, TableDef};

use super::code_indenter::CodeIndenter;
use super::{GenCtx, GenItem, INDENT};

fn ty_fmt<'a>(ctx: &'a GenCtx, ty: &'a AlgebraicType, namespace: &'a str) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicType::Sum(sum_type) => {
            // This better be an option type
            let Some(inner_ty) = sum_type.as_option() else {
                unimplemented!();
            };

            write!(f, "{}", ty_fmt(ctx, inner_ty, namespace))?;
            if inner_ty.is_scalar() {
                // This has to be a nullable type.
                f.write_char('?')?;
            }
            Ok(())
        }
        AlgebraicType::Product(prod) => {
            // The only type that is allowed here is the identity type. All other types should fail.
            if prod.is_identity() {
                write!(f, "SpacetimeDB.Identity")
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Array(ArrayType { elem_ty }) if **elem_ty == AlgebraicType::U8 => f.write_str("byte[]"),
        AlgebraicType::Array(ArrayType { elem_ty }) => {
            write!(
                f,
                "System.Collections.Generic.List<{}>",
                ty_fmt(ctx, elem_ty, namespace)
            )
        }
        AlgebraicType::Map(ty) => write!(
            f,
            "System.Collections.Generic.Dictionary<{}, {}>",
            ty_fmt(ctx, &ty.ty, namespace),
            ty_fmt(ctx, &ty.key_ty, namespace)
        ),
        AlgebraicType::Bool => f.write_str("bool"),
        AlgebraicType::I8 => f.write_str("sbyte"),
        AlgebraicType::U8 => f.write_str("byte"),
        AlgebraicType::I16 => f.write_str("short"),
        AlgebraicType::U16 => f.write_str("ushort"),
        AlgebraicType::I32 => f.write_str("int"),
        AlgebraicType::U32 => f.write_str("uint"),
        AlgebraicType::I64 => f.write_str("long"),
        AlgebraicType::U64 => f.write_str("ulong"),
        AlgebraicType::I128 => panic!("i128 not supported for csharp"),
        AlgebraicType::U128 => panic!("i128 not supported for csharp"),
        AlgebraicType::String => f.write_str("string"),
        AlgebraicType::F32 => f.write_str("float"),
        AlgebraicType::F64 => f.write_str("double"),
        AlgebraicType::Ref(r) => {
            let name = csharp_typename(ctx, *r);
            match &ctx.typespace[*r] {
                AlgebraicType::Sum(sum_type) if sum_type.is_simple_enum() => {
                    let mut ps = name.split('.');
                    if let Some((e_ns, e_name)) = ps.next().zip(ps.next()) {
                        write!(f, "{namespace}.{e_ns}.Types.{e_name}")
                    } else {
                        write!(f, "{}.{}", namespace, name)
                    }
                }
                _ => {
                    write!(f, "{}.{}", namespace, name)
                }
            }
        }
    })
}

fn convert_type<'a>(
    ctx: &'a GenCtx,
    vecnest: usize,
    ty: &'a AlgebraicType,
    value: impl fmt::Display + 'a,
    namespace: &'a str,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        ty if ty.is_identity() => write!(
            f,
            "SpacetimeDB.Identity.From({}.AsProductValue().elements[0].AsBytes())",
            value
        ),
        ty if ty.is_bytes() => write!(f, "{value}.AsBytes()"),
        AlgebraicType::Array(ArrayType { elem_ty }) => {
            let csharp_type = ty_fmt(ctx, elem_ty, namespace);
            writeln!(
                f,
                "((System.Func<System.Collections.Generic.List<{csharp_type}>>)(() => {{"
            )?;
            writeln!(
                f,
                "\tvar vec{vecnest} = new System.Collections.Generic.List<{}>();",
                csharp_type
            )?;
            writeln!(f, "\tvar vec{vecnest}_source = {value}.AsArray();",)?;
            writeln!(f, "\tforeach(var entry in vec{vecnest}_source!)")?;
            writeln!(f, "\t{{")?;
            writeln!(
                f,
                "\t\tvec{vecnest}.Add({});",
                convert_type(ctx, vecnest + 1, elem_ty, "entry", namespace)
            )?;
            writeln!(f, "\t}}")?;
            writeln!(f, "\treturn vec{vecnest};")?;
            write!(f, "}}))()")
        }
        AlgebraicType::Product(_) | AlgebraicType::Map(_) => todo!(),
        AlgebraicType::Sum(sum_type) => {
            if let Some(inner_ty) = sum_type.as_option() {
                match inner_ty {
                    ty if ty.is_scalar() => write!(
                        f,
                        "{}.AsSumValue().tag == 1 ? null : new {}?({}.AsSumValue().value{})",
                        value,
                        ty_fmt(ctx, inner_ty, namespace),
                        value,
                        &convert_type(ctx, vecnest, inner_ty, "", namespace),
                    ),
                    _ => fmt::Display::fmt(
                        &convert_type(
                            ctx,
                            vecnest,
                            inner_ty,
                            format_args!("{}.AsSumValue().tag == 1 ? null : {}.AsSumValue().value", value, value),
                            namespace,
                        ),
                        f,
                    ),
                }
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Ref(r) => {
            let name = csharp_typename(ctx, *r);
            match &ctx.typespace[*r] {
                AlgebraicType::Sum(sum) => {
                    if sum.is_simple_enum() {
                        let split: Vec<&str> = name.split('.').collect();
                        if split.len() >= 2 {
                            assert_eq!(
                                split.len(),
                                2,
                                "Enum namespaces can only be in the form Namespace.EnumName, invalid value={}",
                                name
                            );
                            let enum_namespace = split[0];
                            let enum_name = split[1];
                            write!(f, "{namespace}.{enum_namespace}.Into{enum_name}({value})")
                        } else {
                            writeln!(
                                f,
                                "({name})Enum.Parse(typeof({name}), {value}.AsSumValue().tag.ToString())"
                            )
                        }
                    } else {
                        unimplemented!()
                    }
                }
                _ => {
                    write!(f, "({namespace}.{name})({value})",)
                }
            }
        }
        ty if ty.is_scalar() => write!(f, "{value}.As{ty:?}()"),
        AlgebraicType::String => write!(f, "{value}.AsString()"),
        _ => unreachable!(),
    })
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

fn convert_algebraic_type<'a>(ctx: &'a GenCtx, ty: &'a AlgebraicType, namespace: &'a str) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicType::Product(product_type) => write!(f, "{}", convert_product_type(ctx, product_type, namespace)),
        AlgebraicType::Sum(sum_type) => write!(f, "{}", convert_sum_type(ctx, sum_type, namespace)),
        AlgebraicType::Array(ArrayType { elem_ty }) => write!(
            f,
            "SpacetimeDB.SATS.AlgebraicType.CreateArrayType({})",
            convert_algebraic_type(ctx, elem_ty, namespace)
        ),
        AlgebraicType::Map(_) => todo!(),
        ty if ty.is_scalar_or_string() => write!(f, "SpacetimeDB.SATS.AlgebraicType.Type.{ty:?}",),
        AlgebraicType::Ref(r) => {
            let name = csharp_typename(ctx, *r);
            match &ctx.typespace[*r] {
                AlgebraicType::Sum(sum_type) if sum_type.is_simple_enum() => {
                    let mut ps = name.split('.');
                    if let Some((e_ns, e_name)) = ps.next().zip(ps.next()) {
                        write!(f, "{namespace}.{e_ns}.GetAlgebraicTypeFor{e_name}()")
                    } else {
                        write!(f, "{}", convert_sum_type(ctx, sum_type, namespace))
                    }
                }
                AlgebraicType::Sum(_) => unimplemented!(),
                _ => write!(f, "{}.{}.GetAlgebraicType()", namespace, name),
            }
        }
        _ => unreachable!(),
    })
}

fn convert_product_type<'a>(
    ctx: &'a GenCtx,
    product_type: &'a ProductType,
    namespace: &'a str,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| {
        writeln!(
            f,
            "SpacetimeDB.SATS.AlgebraicType.CreateProductType(new SpacetimeDB.SATS.ProductTypeElement[]"
        )?;
        writeln!(f, "{{")?;
        for (_, elem) in product_type.elements.iter().enumerate() {
            writeln!(
                f,
                "{INDENT}new SpacetimeDB.SATS.ProductTypeElement({}, {}),",
                elem.name
                    .to_owned()
                    .map(|s| format!("\"{}\"", s))
                    .unwrap_or("null".into()),
                convert_algebraic_type(ctx, &elem.algebraic_type, namespace)
            )?;
        }
        write!(f, "}})")
    })
}

fn convert_sum_type<'a>(ctx: &'a GenCtx, sum_type: &'a SumType, namespace: &'a str) -> impl fmt::Display + 'a {
    fmt_fn(move |f| {
        writeln!(
            f,
            "SpacetimeDB.SATS.AlgebraicType.CreateSumType(new System.Collections.Generic.List<SpacetimeDB.SATS.SumTypeVariant>"
        )?;
        writeln!(f, "{{")?;
        for (_, elem) in sum_type.variants.iter().enumerate() {
            writeln!(
                f,
                "\tnew SpacetimeDB.SATS.SumTypeVariant({}, {}),",
                elem.name
                    .to_owned()
                    .map(|s| format!("\"{}\"", s))
                    .unwrap_or("null".into()),
                convert_algebraic_type(ctx, &elem.algebraic_type, namespace)
            )?;
        }
        write!(f, "}})")
    })
}

pub fn autogen_csharp_sum(ctx: &GenCtx, name: &str, sum_type: &SumType, namespace: &str) -> String {
    if sum_type.is_simple_enum() {
        autogen_csharp_enum(ctx, name, sum_type, namespace)
    } else {
        unimplemented!();
    }
}

pub fn autogen_csharp_enum(ctx: &GenCtx, name: &str, sum_type: &SumType, namespace: &str) -> String {
    let mut output = CodeIndenter::new(String::new());

    let mut sum_namespace = None;
    let mut sum_type_name = name.replace("r#", "").to_case(Case::Pascal);
    let mut sum_full_enum_type_name = sum_type_name.clone();
    if sum_type_name.contains('.') {
        let split: Vec<&str> = sum_type_name.split('.').collect();
        if split.len() != 2 {
            panic!("Enum names cannot contain more than one namespace prefix. Example: MyNamespace.MyEnum");
        }

        sum_namespace = Some(split[0].to_case(Case::Pascal));
        sum_type_name = split[1].to_case(Case::Pascal);
        sum_full_enum_type_name = format!("{}.Types.{}", sum_namespace.clone().unwrap(), sum_type_name);
    }

    writeln!(
        output,
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE",
    )
    .unwrap();
    writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "using System;").unwrap();
    if namespace != "SpacetimeDB" {
        writeln!(output, "using SpacetimeDB;").unwrap();
    }

    writeln!(output).unwrap();

    writeln!(output, "namespace {namespace}").unwrap();
    writeln!(output, "{{").unwrap();
    {
        indent_scope!(output);
        writeln!(
            output,
            "public {}",
            match sum_namespace.clone() {
                None => format!("enum {}", sum_type_name),
                Some(namespace) => format!("partial class {}", namespace),
            },
        )
        .unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);
            match sum_namespace {
                Some(_) => {
                    writeln!(output, "public partial class Types").unwrap();
                    writeln!(output, "{{").unwrap();
                    {
                        indent_scope!(output);
                        writeln!(output, "public enum {}", sum_type_name).unwrap();
                        writeln!(output, "{{").unwrap();
                        {
                            indent_scope!(output);
                            for variant in &*sum_type.variants {
                                let variant_name = variant
                                    .name
                                    .as_ref()
                                    .expect("All sum variants should have names!")
                                    .replace("r#", "");
                                writeln!(output, "{},", variant_name).unwrap();
                            }
                        }
                        writeln!(output, "}}").unwrap();
                    }
                    writeln!(output, "}}").unwrap();

                    writeln!(
                        output,
                        "public static SpacetimeDB.SATS.AlgebraicType GetAlgebraicTypeFor{sum_type_name}()"
                    )
                    .unwrap();
                    writeln!(output, "{{").unwrap();
                    {
                        indent_scope!(output);
                        writeln!(output, "return {};", convert_sum_type(ctx, sum_type, namespace)).unwrap();
                    }
                    writeln!(output, "}}").unwrap();

                    write!(
                        output,
                        "{}",
                        autogen_csharp_enum_value_to_struct(sum_type_name, sum_full_enum_type_name, sum_type)
                    )
                    .unwrap();
                }
                None => {
                    for variant in &*sum_type.variants {
                        let variant_name = variant
                            .name
                            .as_ref()
                            .expect("All sum variants should have names!")
                            .replace("r#", "");
                        writeln!(output, "{},", variant_name).unwrap();
                    }
                }
            }
        }

        // End either enum or class def
        writeln!(output, "}}").unwrap();
    }
    writeln!(output, "}}").unwrap();

    output.into_inner()
}

fn autogen_csharp_enum_value_to_struct(sum_name: String, sum_full_enum_name: String, sum_type: &SumType) -> String {
    let mut output: String = String::new();

    writeln!(
        output,
        "public static {sum_full_enum_name} Into{sum_name}(SpacetimeDB.SATS.AlgebraicValue value)",
    )
    .unwrap();
    writeln!(output, "{{").unwrap();
    writeln!(output, "\tvar sumValue = value.AsSumValue();").unwrap();
    writeln!(output, "\tswitch(sumValue.tag)").unwrap();
    writeln!(output, "\t{{").unwrap();

    for (idx, variant) in sum_type.variants.iter().enumerate() {
        let field_name = variant
            .name
            .as_ref()
            .expect("autogen'd product types should have field names");
        let csharp_variant_name = field_name.to_string().replace("r#", "").to_case(Case::Pascal);
        writeln!(output, "\t\tcase {}:", idx).unwrap();
        writeln!(output, "\t\t\treturn {sum_full_enum_name}.{csharp_variant_name};").unwrap();
    }

    // End Switch
    writeln!(output, "\t}}").unwrap();
    writeln!(output).unwrap();
    writeln!(output, "\treturn default;").unwrap();
    // End Func
    writeln!(output, "}}").unwrap();

    output
}

pub fn autogen_csharp_tuple(ctx: &GenCtx, name: &str, tuple: &ProductType, namespace: &str) -> String {
    autogen_csharp_product_table_common(ctx, name, tuple, None, namespace)
}

pub fn autogen_csharp_table(ctx: &GenCtx, table: &TableDef, namespace: &str) -> String {
    let tuple = ctx.typespace[table.data].as_product().unwrap();
    autogen_csharp_product_table_common(ctx, &table.name, tuple, Some(&table.column_attrs), namespace)
}

fn autogen_csharp_product_table_common(
    ctx: &GenCtx,
    name: &str,
    product_type: &ProductType,
    column_attrs: Option<&[ColumnIndexAttribute]>,
    namespace: &str,
) -> String {
    let mut output = CodeIndenter::new(String::new());

    writeln!(
        output,
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE",
    )
    .unwrap();
    writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "using System;").unwrap();
    writeln!(output, "using System.Collections.Generic;").unwrap();
    if namespace != "SpacetimeDB" {
        writeln!(output, "using SpacetimeDB;").unwrap();
    }

    writeln!(output).unwrap();

    writeln!(output, "namespace {namespace}").unwrap();
    writeln!(output, "{{").unwrap();
    {
        indent_scope!(output);
        writeln!(output, "public partial class {name} : IDatabaseTable").unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);

            for field in &*product_type.elements {
                let field_name = field
                    .name
                    .as_ref()
                    .expect("autogen'd tuples should have field names")
                    .replace("r#", "");
                writeln!(output, "[Newtonsoft.Json.JsonProperty(\"{field_name}\")]").unwrap();
                match &field.algebraic_type {
                    ty if ty.is_bytes() => {
                        writeln!(
                            output,
                            "[Newtonsoft.Json.JsonConverter(typeof(SpacetimeDB.ByteArrayConverter))]"
                        )
                        .unwrap();
                    }
                    AlgebraicType::Sum(sum) => {
                        if sum.as_option().is_some() {
                            writeln!(output, "[SpacetimeDB.Some]").unwrap();
                        } else {
                            unimplemented!()
                        }
                    }
                    AlgebraicType::Ref(type_ref) => {
                        if let AlgebraicType::Sum(sum_type) = &ctx.typespace[*type_ref] {
                            if sum_type.is_simple_enum() {
                                writeln!(output, "[SpacetimeDB.Enum]").unwrap();
                            } else {
                                unimplemented!()
                            }
                        }
                    }
                    _ => {}
                }

                writeln!(
                    output,
                    "public {} {};",
                    ty_fmt(ctx, &field.algebraic_type, namespace),
                    field_name.to_case(Case::Pascal)
                )
                .unwrap();
            }

            writeln!(output).unwrap();

            // If this is a table, we want to generate indexes
            if let Some(column_attrs) = column_attrs {
                let indexed_fields: Vec<(&ProductTypeElement, String)> = column_attrs
                    .iter()
                    .enumerate()
                    .filter(|a| a.1.is_unique() || a.1.is_primary())
                    .map(|a| &product_type.elements[a.0])
                    .map(|f| (f, f.name.as_ref().unwrap().replace("r#", "").to_case(Case::Pascal)))
                    .collect();
                // Declare custom index dictionaries
                for (field, field_name) in &indexed_fields {
                    let type_name = ty_fmt(ctx, &field.algebraic_type, namespace);
                    let comparer = if format!("{}", type_name) == "byte[]" {
                        ", new SpacetimeDB.ByteArrayComparer()"
                    } else {
                        ""
                    };
                    writeln!(
                        output,
                        "private static Dictionary<{type_name}, {name}> {field_name}_Index = new Dictionary<{type_name}, {name}>(16{comparer});"
                    )
                        .unwrap();
                }
                writeln!(output).unwrap();
                // OnInsert method for updating indexes
                writeln!(
                    output,
                    "private static void InternalOnValueInserted(object insertedValue)"
                )
                .unwrap();
                writeln!(output, "{{").unwrap();
                {
                    indent_scope!(output);
                    writeln!(output, "var val = ({name})insertedValue;").unwrap();
                    for (_, field_name) in &indexed_fields {
                        writeln!(output, "{field_name}_Index[val.{field_name}] = val;").unwrap();
                    }
                }
                writeln!(output, "}}").unwrap();
                writeln!(output).unwrap();
                // OnDelete method for updating indexes
                writeln!(
                    output,
                    "private static void InternalOnValueDeleted(object deletedValue)"
                )
                .unwrap();
                writeln!(output, "{{").unwrap();
                {
                    indent_scope!(output);
                    writeln!(output, "var val = ({name})deletedValue;").unwrap();
                    for (_, field_name) in &indexed_fields {
                        writeln!(output, "{field_name}_Index.Remove(val.{field_name});").unwrap();
                    }
                }
                writeln!(output, "}}").unwrap();
                writeln!(output).unwrap();
            } // End indexes

            writeln!(
                output,
                "public static SpacetimeDB.SATS.AlgebraicType GetAlgebraicType()"
            )
            .unwrap();
            writeln!(output, "{{").unwrap();
            {
                indent_scope!(output);
                writeln!(output, "return {};", convert_product_type(ctx, product_type, namespace)).unwrap();
            }
            writeln!(output, "}}").unwrap();
            writeln!(output).unwrap();

            write!(
                output,
                "{}",
                autogen_csharp_product_value_to_struct(ctx, name, product_type, namespace)
            )
            .unwrap();

            writeln!(output).unwrap();

            // If this is a table, we want to include functions for accessing the table data
            if let Some(column_attrs) = column_attrs {
                // Insert the funcs for accessing this struct
                let has_primary_key =
                    autogen_csharp_access_funcs_for_struct(&mut output, name, product_type, name, column_attrs);

                writeln!(output).unwrap();

                writeln!(
                    output,
                    "public delegate void InsertEventHandler({name} insertedValue, {namespace}.ReducerEvent dbEvent);"
                )
                .unwrap();
                if has_primary_key {
                    writeln!(output, "public delegate void UpdateEventHandler({name} oldValue, {name} newValue, {namespace}.ReducerEvent dbEvent);").unwrap();
                }
                writeln!(
                    output,
                    "public delegate void DeleteEventHandler({name} deletedValue, {namespace}.ReducerEvent dbEvent);"
                )
                .unwrap();
                writeln!(output, "public delegate void RowUpdateEventHandler(SpacetimeDBClient.TableOp op, {name} oldValue, {name} newValue, {namespace}.ReducerEvent dbEvent);").unwrap();
                writeln!(output, "public static event InsertEventHandler OnInsert;").unwrap();
                if has_primary_key {
                    writeln!(output, "public static event UpdateEventHandler OnUpdate;").unwrap();
                }
                writeln!(output, "public static event DeleteEventHandler OnBeforeDelete;").unwrap();
                writeln!(output, "public static event DeleteEventHandler OnDelete;").unwrap();

                writeln!(output, "public static event RowUpdateEventHandler OnRowUpdate;").unwrap();

                writeln!(output).unwrap();

                writeln!(
                    output,
                    "public static void OnInsertEvent(object newValue, ClientApi.Event dbEvent)"
                )
                .unwrap();
                writeln!(output, "{{").unwrap();
                {
                    indent_scope!(output);
                    writeln!(
                        output,
                        "OnInsert?.Invoke(({name})newValue,(ReducerEvent)dbEvent?.FunctionCall.CallInfo);"
                    )
                    .unwrap();
                }
                writeln!(output, "}}").unwrap();
                writeln!(output).unwrap();

                if has_primary_key {
                    writeln!(
                        output,
                        "public static void OnUpdateEvent(object oldValue, object newValue, ClientApi.Event dbEvent)"
                    )
                    .unwrap();
                    writeln!(output, "{{").unwrap();
                    {
                        indent_scope!(output);
                        writeln!(
                            output,
                            "OnUpdate?.Invoke(({name})oldValue,({name})newValue,(ReducerEvent)dbEvent?.FunctionCall.CallInfo);"
                        )
                            .unwrap();
                    }
                    writeln!(output, "}}").unwrap();
                    writeln!(output).unwrap();
                }

                writeln!(
                    output,
                    "public static void OnBeforeDeleteEvent(object oldValue, ClientApi.Event dbEvent)"
                )
                .unwrap();
                writeln!(output, "{{").unwrap();
                {
                    indent_scope!(output);
                    writeln!(
                        output,
                        "OnBeforeDelete?.Invoke(({name})oldValue,(ReducerEvent)dbEvent?.FunctionCall.CallInfo);"
                    )
                    .unwrap();
                }
                writeln!(output, "}}").unwrap();
                writeln!(output).unwrap();

                writeln!(
                    output,
                    "public static void OnDeleteEvent(object oldValue, ClientApi.Event dbEvent)"
                )
                .unwrap();
                writeln!(output, "{{").unwrap();
                {
                    indent_scope!(output);
                    writeln!(
                        output,
                        "OnDelete?.Invoke(({name})oldValue,(ReducerEvent)dbEvent?.FunctionCall.CallInfo);"
                    )
                    .unwrap();
                }
                writeln!(output, "}}").unwrap();
                writeln!(output).unwrap();

                writeln!(
                    output,
                    "public static void OnRowUpdateEvent(SpacetimeDBClient.TableOp op, object oldValue, object newValue, ClientApi.Event dbEvent)"
                )
                    .unwrap();
                writeln!(output, "{{").unwrap();
                {
                    indent_scope!(output);
                    writeln!(
                        output,
                        "OnRowUpdate?.Invoke(op, ({name})oldValue,({name})newValue,(ReducerEvent)dbEvent?.FunctionCall.CallInfo);"
                    )
                        .unwrap();
                }
                writeln!(output, "}}").unwrap();
            }
        }
        writeln!(output, "}}").unwrap();
    }
    writeln!(output, "}}").unwrap();

    output.into_inner()
}

fn autogen_csharp_product_value_to_struct(
    ctx: &GenCtx,
    struct_name_pascal_case: &str,
    product_type: &ProductType,
    namespace: &str,
) -> String {
    let mut output_contents_header: String = String::new();
    let mut output_contents_return: String = String::new();

    writeln!(
        output_contents_header,
        "public static explicit operator {struct_name_pascal_case}(SpacetimeDB.SATS.AlgebraicValue value)",
    )
    .unwrap();
    writeln!(output_contents_header, "{{").unwrap();

    writeln!(output_contents_header, "\tif (value == null) {{").unwrap();
    writeln!(output_contents_header, "\t\treturn null;").unwrap();
    writeln!(output_contents_header, "\t}}").unwrap();

    writeln!(output_contents_header, "\tvar productValue = value.AsProductValue();").unwrap();

    // vec conversion go here
    writeln!(output_contents_return, "\treturn new {}", struct_name_pascal_case).unwrap();
    writeln!(output_contents_return, "\t{{").unwrap();

    for (idx, field) in product_type.elements.iter().enumerate() {
        let field_name = field
            .name
            .as_ref()
            .expect("autogen'd product types should have field names");
        let field_type = &field.algebraic_type;
        let csharp_field_name = field_name.to_string().replace("r#", "").to_case(Case::Pascal);

        writeln!(
            output_contents_return,
            "\t\t{csharp_field_name} = {},",
            convert_type(
                ctx,
                0,
                field_type,
                format_args!("productValue.elements[{idx}]"),
                namespace,
            )
        )
        .unwrap();
    }

    // End Struct
    writeln!(output_contents_return, "\t}};").unwrap();
    // End Func
    writeln!(output_contents_return, "}}").unwrap();

    output_contents_header + &output_contents_return
}

fn indented_block<R>(output: &mut CodeIndenter<String>, f: impl FnOnce(&mut CodeIndenter<String>) -> R) -> R {
    writeln!(output, "{{").unwrap();
    let res = f(&mut output.indented(1));
    writeln!(output, "}}").unwrap();
    res
}

fn autogen_csharp_access_funcs_for_struct(
    output: &mut CodeIndenter<String>,
    struct_name_pascal_case: &str,
    product_type: &ProductType,
    table_name: &str,
    column_attrs: &[ColumnIndexAttribute],
) -> bool {
    let (unique, nonunique) = column_attrs
        .iter()
        .copied()
        .enumerate()
        .partition::<Vec<_>, _>(|(_, attr)| attr.is_unique());
    let unique_it = unique.into_iter().chain(nonunique);
    writeln!(
        output,
        "public static System.Collections.Generic.IEnumerable<{struct_name_pascal_case}> Iter()"
    )
    .unwrap();
    indented_block(output, |output| {
        writeln!(
            output,
            "foreach(var entry in SpacetimeDBClient.clientDB.GetEntries(\"{table_name}\"))",
        )
        .unwrap();
        indented_block(output, |output| {
            // TODO: best way to handle this?
            writeln!(output, "yield return ({struct_name_pascal_case})entry.Item2;").unwrap();
        });
    });

    writeln!(output, "public static int Count()").unwrap();
    indented_block(output, |output| {
        writeln!(output, "return SpacetimeDBClient.clientDB.Count(\"{table_name}\");",).unwrap();
    });
    let mut primary_col_idx = None;

    for (col_i, attr) in unique_it {
        let is_unique = attr.is_unique();
        let is_primary = attr.is_primary();
        if is_primary {
            if primary_col_idx.is_some() {
                panic!("Multiple primary columns defined for table: {}", table_name);
            } else {
                primary_col_idx = Some(col_i);
            }
        }
        let field = &product_type.elements[col_i];
        let field_name = field.name.as_ref().expect("autogen'd tuples should have field names");
        let field_type = &field.algebraic_type;
        let csharp_field_name_pascal = field_name.replace("r#", "").to_case(Case::Pascal);

        let (field_type, csharp_field_type) = match field_type {
            ty if ty.is_identity() => ("Identity", "SpacetimeDB.Identity"),
            // Do allow filtering for byte arrays.
            ty if ty.is_bytes() => ("Bytes", "byte[]"),
            AlgebraicType::Product(_)
            | AlgebraicType::Ref(_)
            | AlgebraicType::Sum(_)
            | AlgebraicType::Array(_)
            | AlgebraicType::Map(_) => {
                // TODO: We don't allow filtering on enums, tuples, arrays, or maps right now;
                //       it's possible we may consider it for the future.
                //       For maps, it would be nice to be able to say,
                //       give me all entries where this vec contains this value,
                //       which we can do.
                continue;
            }
            AlgebraicType::Bool => ("Bool", "bool"),
            AlgebraicType::I8 => ("I8", "sbyte"),
            AlgebraicType::U8 => ("U8", "byte"),
            AlgebraicType::I16 => ("I16", "short"),
            AlgebraicType::U16 => ("U16", "ushort"),
            AlgebraicType::I32 => ("I32", "int"),
            AlgebraicType::U32 => ("U32", "uint"),
            AlgebraicType::I64 => ("I64", "long"),
            AlgebraicType::U64 => ("U64", "ulong"),
            AlgebraicType::I128 => panic!("i128 not supported for csharp"),
            AlgebraicType::U128 => panic!("i128 not supported for csharp"),
            AlgebraicType::String => ("String", "string"),
            AlgebraicType::F32 => ("F32", "float"),
            AlgebraicType::F64 => ("F64", "double"),
        };

        let filter_return_type = fmt_fn(|f| {
            if is_unique {
                f.write_str(struct_name_pascal_case)
            } else {
                write!(f, "System.Collections.Generic.IEnumerable<{}>", struct_name_pascal_case)
            }
        });

        writeln!(
            output,
            "public static {filter_return_type} FilterBy{}({} value)",
            csharp_field_name_pascal, csharp_field_type
        )
        .unwrap();

        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);
            if is_unique || is_primary {
                writeln!(
                    output,
                    "{csharp_field_name_pascal}_Index.TryGetValue(value, out var r);"
                )
                .unwrap();
                writeln!(output, "return r;").unwrap();
            } else {
                writeln!(
                    output,
                    "foreach(var entry in SpacetimeDBClient.clientDB.GetEntries(\"{}\"))",
                    table_name
                )
                .unwrap();
                writeln!(output, "{{").unwrap();
                {
                    indent_scope!(output);
                    writeln!(output, "var productValue = entry.Item1.AsProductValue();").unwrap();
                    if field_type == "Identity" {
                        writeln!(
                            output,
                            "var compareValue = Identity.From(productValue.elements[{}].AsProductValue().elements[0].AsBytes());",
                            col_i
                        )
                        .unwrap();
                    } else {
                        writeln!(
                            output,
                            "var compareValue = ({})productValue.elements[{}].As{}();",
                            csharp_field_type, col_i, field_type
                        )
                        .unwrap();
                    }
                    if csharp_field_type == "byte[]" {
                        writeln!(
                            output,
                            "static bool ByteArrayCompare(byte[] a1, byte[] a2)
    {{
        if (a1.Length != a2.Length)
            return false;

        for (int i=0; i<a1.Length; i++)
            if (a1[i]!=a2[i])
                return false;

        return true;
    }}"
                        )
                        .unwrap();
                        writeln!(output).unwrap();
                        writeln!(output, "if (ByteArrayCompare(compareValue, value)) {{").unwrap();
                        {
                            indent_scope!(output);
                            if is_unique {
                                writeln!(output, "return ({struct_name_pascal_case})entry.Item2;").unwrap();
                            } else {
                                writeln!(output, "yield return ({struct_name_pascal_case})entry.Item2;").unwrap();
                            }
                        }
                        writeln!(output, "}}").unwrap();
                    } else {
                        writeln!(output, "if (compareValue == value) {{").unwrap();
                        {
                            indent_scope!(output);
                            if is_unique {
                                writeln!(output, "return ({struct_name_pascal_case})entry.Item2;").unwrap();
                            } else {
                                writeln!(output, "yield return ({struct_name_pascal_case})entry.Item2;").unwrap();
                            }
                        }
                        writeln!(output, "}}").unwrap();
                    }
                }
                // End foreach
                writeln!(output, "}}").unwrap();

                if is_unique {
                    writeln!(output, "return null;").unwrap();
                }
            }
        }
        // End Func
        writeln!(output, "}}").unwrap();
        writeln!(output).unwrap();
    }

    if let Some(primary_col_index) = primary_col_idx {
        writeln!(
            output,
            "public static bool ComparePrimaryKey(SpacetimeDB.SATS.AlgebraicType t, SpacetimeDB.SATS.AlgebraicValue v1, SpacetimeDB.SATS.AlgebraicValue v2)"
        )
            .unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);
            writeln!(
                output,
                "var primaryColumnValue1 = v1.AsProductValue().elements[{}];",
                primary_col_index
            )
            .unwrap();
            writeln!(
                output,
                "var primaryColumnValue2 = v2.AsProductValue().elements[{}];",
                primary_col_index
            )
            .unwrap();
            writeln!(
                output,
                "return SpacetimeDB.SATS.AlgebraicValue.Compare(t.product.elements[0].algebraicType, primaryColumnValue1, primaryColumnValue2);"
            )
                .unwrap();
        }
        writeln!(output, "}}").unwrap();
        writeln!(output).unwrap();

        writeln!(
            output,
            "public static SpacetimeDB.SATS.AlgebraicValue GetPrimaryKeyValue(SpacetimeDB.SATS.AlgebraicValue v)"
        )
        .unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);
            writeln!(output, "return v.AsProductValue().elements[{}];", primary_col_index).unwrap();
        }
        writeln!(output, "}}").unwrap();
        writeln!(output).unwrap();

        writeln!(
            output,
            "public static SpacetimeDB.SATS.AlgebraicType GetPrimaryKeyType(SpacetimeDB.SATS.AlgebraicType t)"
        )
        .unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);
            writeln!(
                output,
                "return t.product.elements[{}].algebraicType;",
                primary_col_index
            )
            .unwrap();
        }
        writeln!(output, "}}").unwrap();
    } else {
        writeln!(
            output,
            "public static bool ComparePrimaryKey(SpacetimeDB.SATS.AlgebraicType t, SpacetimeDB.SATS.AlgebraicValue _v1, SpacetimeDB.SATS.AlgebraicValue _v2)"
        )
            .unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);
            writeln!(output, "return false;").unwrap();
        }
        writeln!(output, "}}").unwrap();
    }

    primary_col_idx.is_some()
}

// fn convert_enumdef(tuple: &SumType) -> impl fmt::Display + '_ {
//     fmt_fn(move |f| {
//         writeln!(f, "AlgebraicType.Tuple(new ProductTypeElement[]")?;
//         writeln!(f, "{{")?;
//         for (i, elem) in tuple.elements.iter().enumerate() {
//             let comma = if i == tuple.elements.len() - 1 { "" } else { "," };
//             writeln!(f, "{INDENT}{}{}", convert_elementdef(elem), comma)?;
//         }
//         writeln!(f, "}}")
//     })
// }

pub fn autogen_csharp_reducer(ctx: &GenCtx, reducer: &ReducerDef, namespace: &str) -> String {
    let func_name = &*reducer.name;
    // let reducer_pascal_name = func_name.to_case(Case::Pascal);
    let use_namespace = true;
    let func_name_pascal_case = func_name.to_case(Case::Pascal);

    let mut output = CodeIndenter::new(String::new());

    let mut func_arguments: String = String::new();
    let mut arg_types: String = String::new();

    writeln!(
        output,
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE"
    )
    .unwrap();
    writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "using System;").unwrap();
    writeln!(output, "using ClientApi;").unwrap();
    writeln!(output, "using Newtonsoft.Json.Linq;").unwrap();
    if namespace != "SpacetimeDB" {
        writeln!(output, "using SpacetimeDB;").unwrap();
    }

    writeln!(output).unwrap();

    if use_namespace {
        writeln!(output, "namespace {}", namespace).unwrap();
        writeln!(output, "{{").unwrap();
        output.indent(1);
    }

    writeln!(output, "public static partial class Reducer").unwrap();
    writeln!(output, "{{").unwrap();

    {
        indent_scope!(output);
        let mut json_args = String::new();
        for (arg_i, arg) in reducer.args.iter().enumerate() {
            let name = arg
                .name
                .as_deref()
                .unwrap_or_else(|| panic!("reducer args should have names: {}", func_name));
            let arg_name = name.to_case(Case::Camel);
            let arg_type_str = ty_fmt(ctx, &arg.algebraic_type, namespace);

            if !json_args.is_empty() {
                json_args.push_str(", ");
            }

            match &arg.algebraic_type {
                AlgebraicType::Sum(sum_type) => {
                    if sum_type.as_option().is_some() {
                        json_args.push_str(&format!("new SpacetimeDB.SomeWrapper<{}>({})", arg_type_str, arg_name));
                    } else {
                        json_args.push_str(&arg_name);
                    }
                }
                ty if ty.is_product() || ty.is_array() || ty.is_scalar_or_string() => {
                    json_args.push_str(&arg_name);
                }
                AlgebraicType::Map(_) => todo!(),
                AlgebraicType::Ref(type_ref) => match &ctx.typespace[*type_ref] {
                    AlgebraicType::Sum(sum) if sum.is_simple_enum() => {
                        json_args
                            .push_str(format!("new SpacetimeDB.EnumWrapper<{}>({})", arg_type_str, arg_name).as_str());
                    }
                    AlgebraicType::Sum(_) => unimplemented!(),
                    _ => json_args.push_str(&arg_name),
                },
                _ => unreachable!(),
            }

            if arg_i > 0 {
                func_arguments.push_str(", ");
            }
            arg_types.push_str(", ");

            write!(func_arguments, "{} {}", arg_type_str, arg_name).unwrap();
            write!(arg_types, "{}", arg_type_str).unwrap();
        }

        let delegate_args = if !reducer.args.is_empty() {
            format!(", {}", func_arguments.clone())
        } else {
            func_arguments.clone()
        };
        writeln!(
            output,
            "public delegate void {func_name_pascal_case}Handler(ReducerEvent reducerEvent{delegate_args});"
        )
        .unwrap();
        writeln!(
            output,
            "public static event {func_name_pascal_case}Handler On{func_name_pascal_case}Event;"
        )
        .unwrap();

        writeln!(output).unwrap();

        writeln!(output, "public static void {func_name_pascal_case}({func_arguments})").unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);

            // Tell the network manager to send this message
            writeln!(output, "var _argArray = new object[] {{{}}};", json_args).unwrap();
            writeln!(output, "var _message = new SpacetimeDBClient.ReducerCallRequest {{").unwrap();
            {
                indent_scope!(output);
                writeln!(output, "fn = \"{}\",", reducer.name).unwrap();
                writeln!(output, "args = _argArray,").unwrap();
            }
            writeln!(output, "}};").unwrap();

            writeln!(
                output,
                "SpacetimeDBClient.instance.InternalCallReducer(Newtonsoft.Json.JsonConvert.SerializeObject(_message, _settings));"
            )
                .unwrap();
        }
        // Closing brace for reducer
        writeln!(output, "}}").unwrap();
        writeln!(output).unwrap();

        writeln!(output, "[ReducerCallback(FunctionName = \"{func_name}\")]").unwrap();
        writeln!(
            output,
            "public static bool On{func_name_pascal_case}(ClientApi.Event dbEvent)"
        )
        .unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);

            writeln!(output, "if(On{func_name_pascal_case}Event != null)").unwrap();
            writeln!(output, "{{").unwrap();
            {
                indent_scope!(output);
                writeln!(
                    output,
                    "var args = ((ReducerEvent)dbEvent.FunctionCall.CallInfo).{func_name_pascal_case}Args;"
                )
                .unwrap();
                writeln!(
                    output,
                    "On{func_name_pascal_case}Event((ReducerEvent)dbEvent.FunctionCall.CallInfo"
                )
                .unwrap();
                // Write out arguments one per line
                {
                    indent_scope!(output);
                    for (i, arg) in reducer.args.iter().enumerate() {
                        let arg_name = arg
                            .name
                            .as_deref()
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| format!("arg_{}", i))
                            .to_case(Case::Pascal);
                        let arg_type_str = ty_fmt(ctx, &arg.algebraic_type, namespace);
                        writeln!(output, ",({arg_type_str})args.{arg_name}").unwrap();
                    }
                }
                writeln!(output, ");").unwrap();
                writeln!(output, "return true;").unwrap();
            }
            // Closing brace for if event is registered
            writeln!(output, "}}").unwrap();
            writeln!(output, "return false;").unwrap();
        }
        // Closing brace for Event parsing function
        writeln!(output, "}}").unwrap();
        writeln!(output).unwrap();

        writeln!(output, "[DeserializeEvent(FunctionName = \"{func_name}\")]").unwrap();
        writeln!(
            output,
            "public static void {func_name_pascal_case}DeserializeEventArgs(ClientApi.Event dbEvent)"
        )
        .unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);

            writeln!(output, "var args = new {func_name_pascal_case}ArgsStruct();").unwrap();
            writeln!(output, "var bsatnBytes = dbEvent.FunctionCall.ArgBytes;").unwrap();
            writeln!(output, "using var ms = new System.IO.MemoryStream();").unwrap();
            writeln!(output, "ms.SetLength(bsatnBytes.Length);").unwrap();
            writeln!(output, "bsatnBytes.CopyTo(ms.GetBuffer(), 0);").unwrap();
            writeln!(output, "ms.Position = 0;").unwrap();
            writeln!(output, "using var reader = new System.IO.BinaryReader(ms);").unwrap();
            for (i, arg) in reducer.args.iter().enumerate() {
                let arg_name = arg
                    .name
                    .as_deref()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| format!("arg_{}", i))
                    .to_case(Case::Pascal);
                let algebraic_type = convert_algebraic_type(ctx, &arg.algebraic_type, namespace);
                writeln!(
                    output,
                    "var args_{i}_value = SpacetimeDB.SATS.AlgebraicValue.Deserialize({algebraic_type}, reader);"
                )
                .unwrap();
                let convert = convert_type(ctx, 0, &arg.algebraic_type, format!("args_{i}_value"), namespace);
                writeln!(output, "args.{arg_name} = {convert};").unwrap();
            }

            writeln!(output, "dbEvent.FunctionCall.CallInfo = new ReducerEvent(ReducerType.{func_name_pascal_case}, \"{func_name}\", dbEvent.Timestamp, Identity.From(dbEvent.CallerIdentity.ToByteArray()), dbEvent.Message, dbEvent.Status, args);").unwrap();
        }

        // Closing brace for Event parsing function
        writeln!(output, "}}").unwrap();
    }
    // Closing brace for class
    writeln!(output, "}}").unwrap();
    writeln!(output).unwrap();

    //Args struct
    writeln!(output, "public partial class {func_name_pascal_case}ArgsStruct").unwrap();
    writeln!(output, "{{").unwrap();
    {
        indent_scope!(output);
        for (i, arg) in reducer.args.iter().enumerate() {
            let arg_name = arg
                .name
                .as_deref()
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("arg_{}", i))
                .to_case(Case::Pascal);
            let cs_type = ty_fmt(ctx, &arg.algebraic_type, namespace);
            writeln!(output, "public {cs_type} {arg_name};").unwrap();
        }
    }
    // Closing brace for struct ReducerArgs
    writeln!(output, "}}").unwrap();
    writeln!(output).unwrap();

    if use_namespace {
        output.dedent(1);
        writeln!(output, "}}").unwrap();
    }

    output.into_inner()
}

pub fn autogen_csharp_globals(items: &[GenItem], namespace: &str) -> Vec<Vec<(String, String)>> {
    let reducers: Vec<&ReducerDef> = items
        .iter()
        .map(|i| {
            if let GenItem::Reducer(reducer) = i {
                Some(reducer)
            } else {
                None
            }
        })
        .filter(|r| r.is_some())
        .flatten()
        .filter(|r| &*r.name != "__init__")
        .collect();
    let reducer_names: Vec<String> = reducers
        .iter()
        .map(|reducer| reducer.name.deref().to_case(Case::Pascal))
        .collect();

    let use_namespace = true;
    let mut output = CodeIndenter::new(String::new());

    writeln!(
        output,
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE"
    )
    .unwrap();
    writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "using System;").unwrap();
    writeln!(output, "using ClientApi;").unwrap();
    writeln!(output, "using Newtonsoft.Json.Linq;").unwrap();
    if namespace != "SpacetimeDB" {
        writeln!(output, "using SpacetimeDB;").unwrap();
    }

    writeln!(output).unwrap();

    if use_namespace {
        writeln!(output, "namespace {}", namespace).unwrap();
        writeln!(output, "{{").unwrap();
        output.indent(1);
    }

    writeln!(output, "public enum ReducerType").unwrap();
    writeln!(output, "{{").unwrap();
    {
        indent_scope!(output);
        writeln!(output, "None,").unwrap();
        for reducer in reducer_names {
            writeln!(output, "{reducer},").unwrap();
        }
    }
    // Closing brace for ReducerType
    writeln!(output, "}}").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "public partial class ReducerEvent : ReducerEventBase").unwrap();
    writeln!(output, "{{").unwrap();
    {
        indent_scope!(output);
        writeln!(output, "public ReducerType Reducer {{ get; private set; }}").unwrap();
        writeln!(output).unwrap();
        writeln!(output, "public ReducerEvent(ReducerType reducer, string reducerName, ulong timestamp, SpacetimeDB.Identity identity, string errMessage, ClientApi.Event.Types.Status status, object args)").unwrap();
        {
            indent_scope!(output);
            writeln!(
                output,
                ": base(reducerName, timestamp, identity, errMessage, status, args)"
            )
            .unwrap();
        }
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);
            writeln!(output, "Reducer = reducer;").unwrap();
        }
        // Closing brace for ctor
        writeln!(output, "}}").unwrap();
        writeln!(output).unwrap();
        // Properties for reducer args
        for reducer in &reducers {
            let reducer_name = reducer.name.deref().to_case(Case::Pascal);
            writeln!(output, "public {reducer_name}ArgsStruct {reducer_name}Args").unwrap();
            writeln!(output, "{{").unwrap();
            {
                indent_scope!(output);
                writeln!(output, "get").unwrap();
                writeln!(output, "{{").unwrap();
                {
                    indent_scope!(output);
                    writeln!(output, "if (Reducer != ReducerType.{reducer_name}) throw new SpacetimeDB.ReducerMismatchException(Reducer.ToString(), \"{reducer_name}\");").unwrap();
                    writeln!(output, "return ({reducer_name}ArgsStruct)Args;").unwrap();
                }
                // Closing brace for struct ReducerArgs
                writeln!(output, "}}").unwrap();
            }
            // Closing brace for struct ReducerArgs
            writeln!(output, "}}").unwrap();
        }
        writeln!(output).unwrap();
        writeln!(output, "public object[] GetArgsAsObjectArray()").unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);
            writeln!(output, "switch (Reducer)").unwrap();
            writeln!(output, "{{").unwrap();
            {
                indent_scope!(output);
                for reducer in &reducers {
                    let reducer_name = reducer.name.deref().to_case(Case::Pascal);
                    writeln!(output, "case ReducerType.{reducer_name}:").unwrap();
                    writeln!(output, "{{").unwrap();
                    {
                        indent_scope!(output);
                        writeln!(output, "var args = {reducer_name}Args;").unwrap();
                        writeln!(output, "return new object[] {{").unwrap();
                        {
                            indent_scope!(output);
                            for (i, arg) in reducer.args.iter().enumerate() {
                                let arg_name = arg
                                    .name
                                    .as_deref()
                                    .map(|n| n.to_string())
                                    .unwrap_or_else(|| format!("arg_{}", i))
                                    .to_case(Case::Pascal);
                                writeln!(output, "args.{arg_name},").unwrap();
                            }
                        }
                        writeln!(output, "}};").unwrap();
                    }
                    // Closing brace for switch
                    writeln!(output, "}}").unwrap();
                }
                writeln!(output, "default: throw new System.Exception($\"Unhandled reducer case: {{Reducer}}. Please run SpacetimeDB code generator\");").unwrap();
            }
            // Closing brace for switch
            writeln!(output, "}}").unwrap();
        }
        // Closing brace for ctor
        writeln!(output, "}}").unwrap();
    }
    // Closing brace for ReducerEvent
    writeln!(output, "}}").unwrap();

    if use_namespace {
        output.dedent(1);
        writeln!(output, "}}").unwrap();
    }

    let mut result = vec![vec![("ReducerEvent.cs".to_string(), output.into_inner())]];

    let mut output = CodeIndenter::new(String::new());

    writeln!(output, "using SpacetimeDB;").unwrap();

    writeln!(output).unwrap();

    if use_namespace {
        writeln!(output, "namespace {}", namespace).unwrap();
        writeln!(output, "{{").unwrap();
        output.indent(1);
    }

    writeln!(output, "public partial class Reducer").unwrap();
    writeln!(output, "{{").unwrap();
    {
        indent_scope!(output);
        writeln!(
            output,
            "private static Newtonsoft.Json.JsonSerializerSettings _settings = new Newtonsoft.Json.JsonSerializerSettings"
        )
        .unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);
            writeln!(
                output,
                "Converters = {{ new SpacetimeDB.SomeWrapperConverter(), new SpacetimeDB.EnumWrapperConverter() }},"
            )
            .unwrap();
            writeln!(output, "ContractResolver = new SpacetimeDB.JsonContractResolver(),").unwrap();
        }
        writeln!(output, "}};").unwrap();
    }
    // Closing brace for struct ReducerArgs
    writeln!(output, "}}").unwrap();

    if use_namespace {
        output.dedent(1);
        writeln!(output, "}}").unwrap();
    }

    result.push(vec![("ReducerJsonSettings.cs".into(), output.into_inner())]);

    result
}
