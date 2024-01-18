use super::util::fmt_fn;

use std::fmt::{self, Write};

use convert_case::{Case, Casing};
use spacetimedb_lib::sats::db::def::TableSchema;
use spacetimedb_lib::sats::{
    AlgebraicType, AlgebraicTypeRef, ArrayType, BuiltinType, MapType, ProductType, ProductTypeElement, SumType,
    SumTypeVariant,
};
use spacetimedb_lib::{ReducerDef, TableDesc};
use spacetimedb_primitives::ColList;

use super::code_indenter::CodeIndenter;
use super::{GenCtx, GenItem, INDENT};

enum MaybePrimitive<'a> {
    Primitive(&'static str),
    Array(&'a ArrayType),
    Map(&'a MapType),
}

fn maybe_primitive(b: &BuiltinType) -> MaybePrimitive {
    MaybePrimitive::Primitive(match b {
        BuiltinType::Bool => "boolean",
        BuiltinType::I8
        | BuiltinType::U8
        | BuiltinType::I16
        | BuiltinType::U16
        | BuiltinType::I32
        | BuiltinType::U32
        | BuiltinType::F32
        | BuiltinType::F64 => "number",
        BuiltinType::I128 | BuiltinType::U128 | BuiltinType::I64 | BuiltinType::U64 => "BigInt",
        BuiltinType::String => "string",
        BuiltinType::Array(ty) => return MaybePrimitive::Array(ty),
        BuiltinType::Map(m) => return MaybePrimitive::Map(m),
    })
}

fn ty_fmt<'a>(ctx: &'a GenCtx, ty: &'a AlgebraicType, ref_prefix: &'a str) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicType::Sum(sum_type) => {
            if let Some(inner_ty) = sum_type.as_option() {
                write!(f, "{} | null", ty_fmt(ctx, inner_ty, ref_prefix))
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Product(prod) => {
            // The only type that is allowed here is the identity type. All other types should fail.
            if prod.is_identity() {
                write!(f, "Identity")
            } else if prod.is_address() {
                write!(f, "Address")
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Builtin(b) => match maybe_primitive(b) {
            MaybePrimitive::Primitive(p) => f.write_str(p),
            MaybePrimitive::Array(ArrayType { elem_ty }) if **elem_ty == AlgebraicType::U8 => f.write_str("Uint8Array"),
            MaybePrimitive::Array(ArrayType { elem_ty }) => {
                write!(f, "{}[]", ty_fmt(ctx, elem_ty, ref_prefix))
            }
            MaybePrimitive::Map(ty) => {
                write!(
                    f,
                    "Map<{}, {}>",
                    ty_fmt(ctx, &ty.ty, ref_prefix),
                    ty_fmt(ctx, &ty.key_ty, ref_prefix)
                )
            }
        },
        AlgebraicType::Ref(r) => write!(f, "{}{}", ref_prefix, typescript_typename(ctx, *r)),
    })
}
fn typescript_as_type(b: &BuiltinType) -> &str {
    match b {
        BuiltinType::Bool => "Boolean",
        BuiltinType::I8 => "Number",
        BuiltinType::U8 => "Number",
        BuiltinType::I16 => "Number",
        BuiltinType::U16 => "Number",
        BuiltinType::I32 => "Number",
        BuiltinType::U32 => "Number",
        BuiltinType::I64 => "BigInt",
        BuiltinType::U64 => "BigInt",
        BuiltinType::I128 => "BigInt",
        BuiltinType::U128 => "BigInt",
        BuiltinType::F32 => "Number",
        BuiltinType::F64 => "Number",
        BuiltinType::String => "String",
        BuiltinType::Array(_) => "Array",
        BuiltinType::Map(_) => "Map",
    }
}
fn convert_builtintype<'a>(
    ctx: &'a GenCtx,
    vecnest: usize,
    b: &'a BuiltinType,
    value: impl fmt::Display + 'a,
    ref_prefix: &'a str,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match maybe_primitive(b) {
        MaybePrimitive::Primitive(_) => {
            let typescript_as_type = typescript_as_type(b);
            write!(f, "{value}.as{typescript_as_type}()")
        }
        MaybePrimitive::Array(ArrayType { elem_ty }) if **elem_ty == AlgebraicType::U8 => {
            write!(f, "{value}.asBytes()")
        }
        MaybePrimitive::Array(ArrayType { elem_ty }) => {
            let typescript_type = ty_fmt(ctx, elem_ty, ref_prefix);
            writeln!(
                f,
                "{value}.asArray().map(el => {}) as {typescript_type}[];",
                convert_type(ctx, vecnest + 1, elem_ty, "el", ref_prefix)
            )
        }
        MaybePrimitive::Map(_) => todo!(),
    })
}

fn convert_type<'a>(
    ctx: &'a GenCtx,
    vecnest: usize,
    ty: &'a AlgebraicType,
    value: impl fmt::Display + 'a,
    ref_prefix: &'a str,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicType::Product(product) => {
            if product.is_identity() {
                write!(f, "new Identity({}.asProductValue().elements[0].asBytes())", value)
            } else if product.is_address() {
                write!(f, "new Address({}.asProductValue().elements[0].asBytes())", value)
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Sum(sum_type) => {
            if let Some(inner_ty) = sum_type.as_option() {
                use AlgebraicType::Builtin;
                match inner_ty {
                    Builtin(ty) => match ty {
                        BuiltinType::Bool
                        | BuiltinType::I8
                        | BuiltinType::U8
                        | BuiltinType::I16
                        | BuiltinType::U16
                        | BuiltinType::I32
                        | BuiltinType::U32
                        | BuiltinType::I64
                        | BuiltinType::U64
                        | BuiltinType::I128
                        | BuiltinType::U128
                        | BuiltinType::F32
                        | BuiltinType::F64 => write!(
                            f,
                            "{}.asSumValue().tag == 1 ? null : {}.asSumValue().value{}",
                            value,
                            value,
                            &convert_type(ctx, vecnest, inner_ty, "", ref_prefix),
                        ),
                        _ => fmt::Display::fmt(
                            &convert_type(
                                ctx,
                                vecnest,
                                inner_ty,
                                format_args!("{value}.asSumValue().tag == 1 ? null : {value}.asSumValue().value"),
                                ref_prefix
                            ),
                            f,
                        ),
                    },
                    AlgebraicType::Ref(_) => fmt::Display::fmt(
                        &format!(
                            "function() {{ const value = {}.asSumValue().tag == 1 ? null : {}.asSumValue().value; return value ? {} : null; }}()",
                            value,
                            value,
                            convert_type(
                                ctx,
                                vecnest,
                                inner_ty,
                                "value",
                                ref_prefix
                            )
                        ),
                        f,
                    ),
                    _ => fmt::Display::fmt(
                        &convert_type(
                            ctx,
                            vecnest,
                            inner_ty,
                            format_args!("{value}.asSumValue().tag == 1 ? null : {value}.asSumValue().value"),
                            ref_prefix
                        ),
                        f,
                    ),
                }
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Builtin(b) => fmt::Display::fmt(&convert_builtintype(ctx, vecnest, b, &value, ref_prefix), f),
        AlgebraicType::Ref(r) => {
            let name = typescript_typename(ctx, *r);
            write!(f, "{ref_prefix}{name}.fromValue({value})",)
        }
    })
}

// can maybe do something fancy with this in the future
fn typescript_typename(ctx: &GenCtx, typeref: AlgebraicTypeRef) -> &str {
    ctx.names[typeref.idx()].as_deref().expect("tuples should have names")
}

fn typescript_filename(ctx: &GenCtx, typeref: AlgebraicTypeRef) -> String {
    ctx.names[typeref.idx()]
        .as_deref()
        .expect("tuples should have names")
        .to_case(Case::Snake)
}

macro_rules! indent_scope {
    ($x:ident) => {
        let mut $x = $x.indented(1);
    };
}

fn convert_algebraic_type<'a>(ctx: &'a GenCtx, ty: &'a AlgebraicType, ref_prefix: &'a str) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicType::Product(product_type) => write!(f, "{}", convert_product_type(ctx, product_type, ref_prefix)),
        AlgebraicType::Sum(sum_type) => write!(f, "{}", convert_sum_type(ctx, sum_type, ref_prefix)),
        AlgebraicType::Builtin(b) => match maybe_primitive(b) {
            MaybePrimitive::Primitive(_) => {
                write!(f, "AlgebraicType.createPrimitiveType(BuiltinType.Type.{b:?})")
            }
            MaybePrimitive::Array(ArrayType { elem_ty }) => write!(
                f,
                "AlgebraicType.createArrayType({})",
                convert_algebraic_type(ctx, elem_ty, ref_prefix)
            ),
            MaybePrimitive::Map(_) => todo!(),
        },
        AlgebraicType::Ref(r) => write!(f, "{ref_prefix}{}.getAlgebraicType()", typescript_typename(ctx, *r)),
    })
}

fn convert_product_type<'a>(
    ctx: &'a GenCtx,
    product_type: &'a ProductType,
    ref_prefix: &'a str,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| {
        writeln!(f, "AlgebraicType.createProductType([")?;
        for (_, elem) in product_type.elements.iter().enumerate() {
            writeln!(
                f,
                "{INDENT}new ProductTypeElement(\"{}\", {}),",
                elem.name
                    .to_owned()
                    .map(|s| {
                        if s == "__identity_bytes" {
                            s
                        } else {
                            typescript_field_name(s.to_case(Case::Camel))
                        }
                    })
                    .unwrap_or("null".into()),
                convert_algebraic_type(ctx, &elem.algebraic_type, ref_prefix)
            )?;
        }
        write!(f, "])")
    })
}

fn convert_sum_type<'a>(ctx: &'a GenCtx, sum_type: &'a SumType, ref_prefix: &'a str) -> impl fmt::Display + 'a {
    fmt_fn(move |f| {
        writeln!(f, "AlgebraicType.createSumType([")?;
        for (_, elem) in sum_type.variants.iter().enumerate() {
            writeln!(
                f,
                "\tnew SumTypeVariant({}, {}),",
                elem.name
                    .to_owned()
                    .map(|s| format!("\"{s}\""))
                    .unwrap_or("null".into()),
                convert_algebraic_type(ctx, &elem.algebraic_type, ref_prefix)
            )?;
        }
        write!(f, "])")
    })
}

fn serialize_type<'a>(
    ctx: &'a GenCtx,
    ty: &'a AlgebraicType,
    value: &'a str,
    prefix: &'a str,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicType::Product(prod) => {
            if prod.is_identity() {
                write!(f, "Array.from({value}.toUint8Array())")
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Sum(sum_type) => {
            if let Some(inner_ty) = sum_type.as_option() {
                write!(
                    f,
                    "{value} ? {{ \"some\": {} }} : {{ \"none\": [] }}",
                    serialize_type(ctx, inner_ty, value, prefix)
                )
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Builtin(BuiltinType::Array(ArrayType { elem_ty })) => match &**elem_ty {
            AlgebraicType::Builtin(BuiltinType::U8) => write!(f, "Array.from({value})"),
            AlgebraicType::Builtin(_) => write!(f, "{value}"),
            t => write!(f, "{value}.map(el => {})", serialize_type(ctx, t, "el", prefix)),
        },
        AlgebraicType::Builtin(_) => write!(f, "{value}"),
        AlgebraicType::Ref(r) => {
            let typename = typescript_typename(ctx, *r);
            write!(f, "{prefix}{typename}.serialize({value})",)
        }
    })
}

pub fn autogen_typescript_sum(ctx: &GenCtx, name: &str, sum_type: &SumType) -> String {
    let mut output = CodeIndenter::new(String::new());

    let sum_type_name = name.replace("r#", "").to_case(Case::Pascal);

    writeln!(
        output,
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE",
    )
    .unwrap();
    writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "// @ts-ignore").unwrap();
    writeln!(
        output,
        "import {{ __SPACETIMEDB__, AlgebraicType, SumTypeVariant, BuiltinType, AlgebraicValue }} from \"@clockworklabs/spacetimedb-sdk\";"
    )
    .unwrap();

    let mut imports = Vec::new();
    generate_imports_variants(ctx, &sum_type.variants, &mut imports, Some("__"));

    for import in imports {
        writeln!(output, "// @ts-ignore").unwrap();
        writeln!(output, "{import}").unwrap();
    }

    writeln!(output).unwrap();

    writeln!(output, "export namespace {sum_type_name} {{").unwrap();

    let mut names = Vec::new();
    {
        indent_scope!(output);

        writeln!(output, "export function getAlgebraicType(): AlgebraicType {{").unwrap();

        {
            indent_scope!(output);

            writeln!(output, "return {};", convert_sum_type(ctx, sum_type, "__")).unwrap();
        }
        writeln!(output, "}}\n").unwrap();

        let serialize = format!("export function serialize(value: {sum_type_name}): object {{");
        writeln!(output, "{serialize}").unwrap();

        {
            indent_scope!(output);

            if sum_type.is_simple_enum() {
                // for a simple enum we can simplify the fromValue function
                writeln!(output, "const result: {{[key: string]: any}} = {{}};").unwrap();
                writeln!(output, "result[value.tag] = [];").unwrap();
                writeln!(output, "return result;").unwrap();
            } else {
                writeln!(output, "switch(value.tag) {{").unwrap();
                {
                    indent_scope!(output);

                    for variant in sum_type.variants.iter() {
                        let variant_name = variant
                            .name
                            .as_ref()
                            .expect("All sum variants should have names!")
                            .replace("r#", "")
                            .to_case(Case::Pascal);
                        writeln!(output, "case \"{variant_name}\":").unwrap();

                        let field_type = &variant.algebraic_type;
                        let to_return = match field_type {
                            AlgebraicType::Product(p) if p.elements.is_empty() => {
                                format!("{{ \"{variant_name}\": [] }}")
                            }
                            _ => format!(
                                "{{ \"{variant_name}\": {} }}",
                                serialize_type(ctx, field_type, "value.value", "__")
                            ),
                        };
                        writeln!(output, "\treturn {to_return};").unwrap();
                    }

                    writeln!(output, "default:").unwrap();
                    writeln!(output, "\tthrow(\"unreachable\");").unwrap();
                }
                writeln!(output, "}}").unwrap();
            }
        }
        writeln!(output, "}}").unwrap();

        writeln!(output).unwrap();

        for variant in &sum_type.variants {
            let variant_name = variant
                .name
                .as_ref()
                .expect("All sum variants should have names!")
                .replace("r#", "")
                .to_case(Case::Pascal);

            names.push(variant_name.clone());

            let a_type = match variant.algebraic_type {
                AlgebraicType::Product(_) => "undefined".to_string(),
                _ => {
                    format!("{}", ty_fmt(ctx, &variant.algebraic_type, "__"))
                }
            };
            writeln!(
                output,
                "export type {variant_name} = {{ tag: \"{variant_name}\", value: {a_type} }};"
            )
            .unwrap();

            // export an object or a function representing an enum value, so people
            // can pass it as an argument
            match variant.algebraic_type {
                AlgebraicType::Product(_) => writeln!(
                    output,
                    "export const {variant_name} = {{ tag: \"{variant_name}\", value: undefined }};"
                )
                .unwrap(),
                _ => writeln!(
                    output,
                    "export const {variant_name} = (value: {a_type}): {variant_name} => {{ return {{ tag: \"{variant_name}\", value }} }};"
                )
                .unwrap(),
            };
        }

        writeln!(output).unwrap();

        let from_value = format!("export function fromValue(value: AlgebraicValue): {sum_type_name} {{");
        writeln!(output, "{from_value}").unwrap();

        {
            indent_scope!(output);

            writeln!(output, "let sumValue = value.asSumValue();").unwrap();

            if sum_type.is_simple_enum() {
                // for a simple enum we can simplify the fromValue function
                writeln!(output, "let tag = sumValue.tag;").unwrap();
                writeln!(
                    output,
                    "let variant = {sum_type_name}.getAlgebraicType().sum.variants[tag];"
                )
                .unwrap();
                writeln!(
                    output,
                    "return {{ tag: variant.name, value: undefined }} as {sum_type_name};"
                )
                .unwrap();
            } else {
                writeln!(output, "switch(sumValue.tag) {{").unwrap();
                {
                    indent_scope!(output);

                    for (i, variant) in sum_type.variants.iter().enumerate() {
                        let variant_name = variant
                            .name
                            .as_ref()
                            .expect("All sum variants should have names!")
                            .replace("r#", "")
                            .to_case(Case::Pascal);
                        writeln!(output, "case {i}:").unwrap();

                        let field_type = &variant.algebraic_type;
                        let result = match field_type {
                            AlgebraicType::Product(product_type) if product_type.elements.is_empty() => {
                                format!("{{ tag: \"{variant_name}\", value: undefined }}")
                            }
                            _ => format!(
                                "{{ tag: \"{variant_name}\", value: {} }}",
                                convert_type(ctx, 0, field_type, "sumValue.value", "__")
                            ),
                        };
                        writeln!(output, "\treturn {result};").unwrap();
                    }

                    writeln!(output, "default:").unwrap();
                    writeln!(output, "\tthrow(\"unreachable\");").unwrap();
                }
                writeln!(output, "}}").unwrap();
            }
        }
        writeln!(output, "}}").unwrap();
    }

    writeln!(output, "}}").unwrap();

    let names = names
        .iter()
        .map(|s| format!("{sum_type_name}.{s}"))
        .collect::<Vec<String>>()
        .join(" | ");
    writeln!(output, "\nexport type {sum_type_name} = {names};").unwrap();
    writeln!(output, "export default {sum_type_name};").unwrap();

    output.into_inner()
}

const RESERVED_KEYWORDS: [&str; 36] = [
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
];

fn typescript_field_name(field_name: String) -> String {
    if RESERVED_KEYWORDS
        .into_iter()
        .map(String::from)
        .collect::<Vec<String>>()
        .contains(&field_name)
    {
        return format!("_{field_name}");
    }

    field_name
}

pub fn autogen_typescript_tuple(ctx: &GenCtx, name: &str, tuple: &ProductType) -> String {
    autogen_typescript_product_table_common(ctx, name, tuple, None)
}
pub fn autogen_typescript_table(ctx: &GenCtx, table: &TableDesc) -> String {
    let tuple = ctx.typespace[table.data].as_product().unwrap();
    autogen_typescript_product_table_common(
        ctx,
        &table.schema.table_name,
        tuple,
        Some(table.schema.clone().into_schema(0.into())),
    )
}

fn generate_imports(ctx: &GenCtx, elements: &Vec<ProductTypeElement>, imports: &mut Vec<String>, prefix: Option<&str>) {
    for field in elements {
        _generate_imports(ctx, &field.algebraic_type, imports, prefix);
    }
}

// TODO: refactor to allow passing both elements and variants
fn generate_imports_variants(
    ctx: &GenCtx,
    variants: &Vec<SumTypeVariant>,
    imports: &mut Vec<String>,
    prefix: Option<&str>,
) {
    for variant in variants {
        _generate_imports(ctx, &variant.algebraic_type, imports, prefix);
    }
}

fn _generate_imports(ctx: &GenCtx, ty: &AlgebraicType, imports: &mut Vec<String>, prefix: Option<&str>) {
    match ty {
        AlgebraicType::Builtin(b) => match b {
            BuiltinType::Array(ArrayType { elem_ty }) => _generate_imports(ctx, elem_ty, imports, prefix),
            BuiltinType::Map(map_type) => {
                _generate_imports(ctx, &map_type.key_ty, imports, prefix);
                _generate_imports(ctx, &map_type.ty, imports, prefix);
            }
            _ => (),
        },
        AlgebraicType::Ref(r) => {
            let class_name = typescript_typename(ctx, *r).to_string();
            let filename = typescript_filename(ctx, *r);

            let imported_as = match prefix {
                Some(prefix) => format!("{class_name} as {prefix}{class_name}"),
                None => class_name,
            };
            let import = format!("import {{ {imported_as} }} from \"./{filename}\";");
            imports.push(import);
        }
        // Generate imports for the fields of anonymous sum types like `Option<T>`.
        AlgebraicType::Sum(s) => {
            for variant in &s.variants {
                _generate_imports(ctx, &variant.algebraic_type, imports, prefix);
            }
        }
        // Do we need to generate imports for fields of anonymous product types as well?
        _ => (),
    }
}

fn autogen_typescript_product_table_common(
    ctx: &GenCtx,
    name: &str,
    product_type: &ProductType,
    schema: Option<TableSchema>,
) -> String {
    let mut output = CodeIndenter::new(String::new());

    let struct_name_pascal_case = name.replace("r#", "").to_case(Case::Pascal);

    writeln!(
        output,
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE",
    )
    .unwrap();
    writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "// @ts-ignore").unwrap();
    writeln!(output, "import {{ __SPACETIMEDB__, AlgebraicType, ProductType, BuiltinType, ProductTypeElement, SumType, SumTypeVariant, DatabaseTable, AlgebraicValue, ReducerEvent, Identity, Address, ClientDB, SpacetimeDBClient }} from \"@clockworklabs/spacetimedb-sdk\";").unwrap();

    let mut imports = Vec::new();
    generate_imports(ctx, &product_type.elements, &mut imports, None);

    for import in imports {
        writeln!(output, "// @ts-ignore").unwrap();
        writeln!(output, "{import}").unwrap();
    }

    writeln!(output).unwrap();

    writeln!(output, "export class {struct_name_pascal_case} extends DatabaseTable").unwrap();
    writeln!(output, "{{").unwrap();
    {
        indent_scope!(output);

        writeln!(output, "public static db: ClientDB = __SPACETIMEDB__.clientDB;").unwrap();
        writeln!(output, "public static tableName = \"{struct_name_pascal_case}\";").unwrap();

        let mut constructor_signature = Vec::new();
        let mut constructor_assignments = Vec::new();
        for field in &product_type.elements {
            let field_name = field
                .name
                .as_ref()
                .expect("autogen'd tuples should have field names")
                .replace("r#", "");
            let field_name_camel_case = typescript_field_name(field_name.to_case(Case::Camel));
            let arg = format!("{}: {}", field_name_camel_case, ty_fmt(ctx, &field.algebraic_type, ""));
            let assignment = format!("this.{field_name_camel_case} = {field_name_camel_case};");

            writeln!(output, "public {arg};",).unwrap();

            constructor_signature.push(arg);
            constructor_assignments.push(assignment);
        }

        writeln!(output).unwrap();

        if let Some(schema) = &schema {
            // if this table has a primary key add it to the codegen
            if let Some(primary_key) = schema
                .pk()
                .map(|field| format!("\"{}\"", field.col_name.to_case(Case::Camel)))
            {
                writeln!(
                    output,
                    "public static primaryKey: string | undefined = {};",
                    primary_key
                )
                .unwrap();
                writeln!(output).unwrap();
            }
        } else {
            writeln!(output, "public static primaryKey: string | undefined = undefined;",).unwrap();
            writeln!(output).unwrap();
        }

        writeln!(output, "constructor({}) {{", constructor_signature.join(", ")).unwrap();
        writeln!(output, "super();").unwrap();
        {
            indent_scope!(output);

            for assignment in constructor_assignments {
                writeln!(output, "{assignment}").unwrap();
            }
        }

        writeln!(output, "}}").unwrap();

        writeln!(output).unwrap();

        writeln!(
            output,
            "public static serialize(value: {struct_name_pascal_case}): object {{"
        )
        .unwrap();

        {
            indent_scope!(output);

            writeln!(output, "return [").unwrap();

            let mut args = Vec::new();
            for field in &product_type.elements {
                let field_name = field
                    .name
                    .as_ref()
                    .expect("autogen'd tuples should have field names")
                    .replace("r#", "");

                let field_name_camel_case = typescript_field_name(field_name.to_case(Case::Camel));
                let value_field = format!("value.{field_name_camel_case}");
                let arg = format!("{}", serialize_type(ctx, &field.algebraic_type, &value_field, ""));
                args.push(arg);
            }

            writeln!(output, "{}", args.join(", ")).unwrap();

            writeln!(output, "];").unwrap();
        }

        writeln!(output, "}}").unwrap();

        writeln!(output).unwrap();

        writeln!(output, "public static getAlgebraicType(): AlgebraicType").unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);
            writeln!(output, "return {};", convert_product_type(ctx, product_type, "")).unwrap();
        }
        writeln!(output, "}}").unwrap();
        writeln!(output).unwrap();

        write!(
            output,
            "{}",
            autogen_typescript_product_value_to_struct(ctx, &struct_name_pascal_case, product_type)
        )
        .unwrap();

        writeln!(output).unwrap();

        if let Some(schema) = &schema {
            autogen_typescript_access_funcs_for_struct(
                &mut output,
                &struct_name_pascal_case,
                product_type,
                name,
                schema,
            );

            writeln!(output).unwrap();
        }
    }
    writeln!(output, "}}").unwrap();

    writeln!(output, "\nexport default {struct_name_pascal_case};").unwrap();

    output.into_inner()
}

fn autogen_typescript_product_value_to_struct(
    ctx: &GenCtx,
    struct_name_pascal_case: &str,
    product_type: &ProductType,
) -> String {
    let mut output = CodeIndenter::new(String::new());

    writeln!(
        output,
        "public static fromValue(value: AlgebraicValue): {struct_name_pascal_case}",
    )
    .unwrap();
    writeln!(output, "{{").unwrap();
    {
        indent_scope!(output);
        writeln!(output, "let productValue = value.asProductValue();").unwrap();

        let mut constructor_args = Vec::new();
        // vec conversion go here
        for (idx, field) in product_type.elements.iter().enumerate() {
            let field_name = field
                .name
                .as_ref()
                .expect("autogen'd product types should have field names");
            let field_type = &field.algebraic_type;
            let typescript_field_name = typescript_field_name(field_name.to_string().replace("r#", ""));
            constructor_args.push(format!("__{typescript_field_name}"));

            writeln!(
                output,
                "let __{typescript_field_name} = {};",
                convert_type(ctx, 0, field_type, format_args!("productValue.elements[{idx}]"), "")
            )
            .unwrap();
        }

        writeln!(output, "return new this({});", constructor_args.join(", ")).unwrap();
    }
    writeln!(output, "}}").unwrap();

    output.into_inner()
}

#[allow(dead_code)]
fn indented_block<R>(output: &mut CodeIndenter<String>, f: impl FnOnce(&mut CodeIndenter<String>) -> R) -> R {
    writeln!(output, "{{").unwrap();
    let res = f(&mut output.indented(1));
    writeln!(output, "}}").unwrap();
    res
}

fn autogen_typescript_access_funcs_for_struct(
    output: &mut CodeIndenter<String>,
    struct_name_pascal_case: &str,
    product_type: &ProductType,
    table_name: &str,
    table: &TableSchema,
) {
    let constraints = table.column_constraints();
    for col in table.columns() {
        let is_unique = constraints[&ColList::new(col.col_pos)].has_unique();
        let field = &product_type.elements[usize::from(col.col_pos)];
        let field_name = field.name.as_ref().expect("autogen'd tuples should have field names");
        let field_type = &field.algebraic_type;
        let typescript_field_name_pascal = field_name.replace("r#", "").to_case(Case::Pascal);
        let typescript_field_name_camel = field_name.replace("r#", "").to_case(Case::Camel);

        let typescript_field_type = match field_type {
            AlgebraicType::Product(product) => {
                if product.is_identity() {
                    "Identity"
                } else if product.is_address() {
                    "Address"
                } else {
                    // TODO: We don't allow filtering on tuples right now, its possible we may consider it for the future.
                    continue;
                }
            }
            AlgebraicType::Ref(_) | AlgebraicType::Sum(_) => {
                // TODO: We don't allow filtering on enums or tuples right now, its possible we may consider it for the future.
                continue;
            }
            AlgebraicType::Builtin(b) => match maybe_primitive(b) {
                MaybePrimitive::Primitive(ty) => ty,
                MaybePrimitive::Array(ArrayType { elem_ty }) => {
                    if let Some(BuiltinType::U8) = elem_ty.as_builtin() {
                        // Do allow filtering for byte arrays
                        "Uint8Array"
                    } else {
                        // TODO: We don't allow filtering based on an array type, but we might want other functionality here in the future.
                        continue;
                    }
                }
                MaybePrimitive::Map(_) => {
                    // TODO: It would be nice to be able to say, give me all entries where this vec contains this value, which we can do.
                    continue;
                }
            },
        };

        let filter_return_type = fmt_fn(|f| {
            if is_unique {
                write!(f, "{struct_name_pascal_case} | null")
            } else {
                write!(f, "{struct_name_pascal_case}[]")
            }
        });

        writeln!(
            output,
            "public static filterBy{typescript_field_name_pascal}(value: {typescript_field_type}): {filter_return_type}"
        )
        .unwrap();

        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);
            if !is_unique {
                writeln!(output, "let result: {filter_return_type} = [];").unwrap();
            }
            writeln!(
                output,
                "for(let instance of this.db.getTable(\"{table_name}\").getInstances())"
            )
            .unwrap();
            writeln!(output, "{{").unwrap();
            {
                indent_scope!(output);
                if typescript_field_type == "Identity" || typescript_field_type == "Address" {
                    writeln!(output, "if (instance.{typescript_field_name_camel}.isEqual(value)) {{",).unwrap();
                    {
                        indent_scope!(output);
                        if is_unique {
                            writeln!(output, "return instance;").unwrap();
                        } else {
                            writeln!(output, "result.push(instance);").unwrap();
                        }
                    }
                    writeln!(output, "}}").unwrap();
                } else if typescript_field_type == "Uint8Array" {
                    writeln!(
                        output,
                        "let byteArrayCompare = function (a1: Uint8Array, a2: Uint8Array)
{{
    if (a1.length !== a2.length)
        return false;

    for (let i=0; i<a1.length; i++)
        if (a1[i]!==a2[i])
            return false;

    return true;
}}"
                    )
                    .unwrap();
                    writeln!(output).unwrap();
                    writeln!(
                        output,
                        "if (byteArrayCompare(instance.{typescript_field_name_camel}, value)) {{"
                    )
                    .unwrap();
                    {
                        indent_scope!(output);
                        if is_unique {
                            writeln!(output, "return instance;").unwrap();
                        } else {
                            writeln!(output, "result.push(instance);").unwrap();
                        }
                    }
                    writeln!(output, "}}").unwrap();
                } else {
                    writeln!(output, "if (instance.{typescript_field_name_camel} === value) {{").unwrap();
                    {
                        indent_scope!(output);
                        if is_unique {
                            writeln!(output, "return instance;").unwrap();
                        } else {
                            writeln!(output, "result.push(instance);").unwrap();
                        }
                    }
                    writeln!(output, "}}").unwrap();
                }
            }
            // End foreach
            writeln!(output, "}}").unwrap();

            if is_unique {
                writeln!(output, "return null;").unwrap();
            } else {
                writeln!(output, "return result;").unwrap();
            }
        }
        // End Func
        writeln!(output, "}}").unwrap();
        writeln!(output).unwrap();
    }
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

pub fn autogen_typescript_reducer(ctx: &GenCtx, reducer: &ReducerDef) -> String {
    let func_name = &*reducer.name;
    // let reducer_pascal_name = func_name.to_case(Case::Pascal);
    let reducer_name_pascal_case = func_name.to_case(Case::Pascal);

    let mut output = CodeIndenter::new(String::new());

    writeln!(
        output,
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE"
    )
    .unwrap();
    writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "// @ts-ignore").unwrap();
    writeln!(output, "import {{ __SPACETIMEDB__, AlgebraicType, ProductType, BuiltinType, ProductTypeElement, DatabaseTable, AlgebraicValue, ReducerArgsAdapter, SumTypeVariant, Serializer, Identity, Address, ReducerEvent, Reducer, SpacetimeDBClient }} from \"@clockworklabs/spacetimedb-sdk\";").unwrap();

    let mut imports = Vec::new();
    generate_imports(
        ctx,
        &reducer.args.clone().into_iter().collect::<Vec<ProductTypeElement>>(),
        &mut imports,
        None,
    );

    for import in imports {
        writeln!(output, "// @ts-ignore").unwrap();
        writeln!(output, "{import}").unwrap();
    }

    writeln!(output).unwrap();

    let mut func_arguments = Vec::new();
    let mut arg_names = Vec::new();
    for arg in reducer.args.iter() {
        let name = arg
            .name
            .as_deref()
            .unwrap_or_else(|| panic!("reducer args should have names: {func_name}"));
        let arg_name = format!("_{}", name.to_case(Case::Camel));
        let arg_type_str = ty_fmt(ctx, &arg.algebraic_type, "");

        func_arguments.push(format!("{arg_name}: {arg_type_str}"));
        arg_names.push(arg_name.to_string());
    }

    let full_reducer_name = format!("{reducer_name_pascal_case}Reducer");

    writeln!(output, "export class {full_reducer_name} extends Reducer").unwrap();
    writeln!(output, "{{").unwrap();

    {
        indent_scope!(output);

        writeln!(
            output,
            "public static reducerName: string = \"{reducer_name_pascal_case}\";"
        )
        .unwrap();
        writeln!(output, "public static call({}) {{", func_arguments.join(", ")).unwrap();
        {
            indent_scope!(output);

            writeln!(output, "this.getReducer().call({});", arg_names.join(", ")).unwrap();
        }
        writeln!(output, "}}\n").unwrap();

        writeln!(output, "public call({}) {{", func_arguments.join(", ")).unwrap();
        {
            indent_scope!(output);

            writeln!(output, "const serializer = this.client.getSerializer();").unwrap();

            let mut arg_names = Vec::new();
            for arg in reducer.args.iter() {
                let ty = &arg.algebraic_type;
                let name = arg
                    .name
                    .as_deref()
                    .unwrap_or_else(|| panic!("reducer args should have names: {func_name}"));
                let arg_name = name.to_case(Case::Camel);

                writeln!(output, "let _{arg_name}Type = {};", convert_algebraic_type(ctx, ty, "")).unwrap();
                writeln!(output, "serializer.write(_{arg_name}Type, _{arg_name});").unwrap();

                arg_names.push(arg_name);
            }

            writeln!(output, "this.client.call(\"{func_name}\", serializer);").unwrap();
        }
        // Closing brace for reducer
        writeln!(output, "}}").unwrap();
        writeln!(output).unwrap();

        let args: &str = if reducer.args.is_empty() {
            "_adapter: ReducerArgsAdapter"
        } else {
            "adapter: ReducerArgsAdapter"
        };
        writeln!(output, "public static deserializeArgs({args}): any[] {{").unwrap();

        {
            indent_scope!(output);

            let mut arg_names = Vec::new();
            for arg in reducer.args.iter() {
                let ty = &arg.algebraic_type;
                let name = arg
                    .name
                    .as_deref()
                    .unwrap_or_else(|| panic!("reducer args should have names: {func_name}"));
                let arg_name = typescript_field_name(name.to_case(Case::Camel));

                writeln!(output, "let {arg_name}Type = {};", convert_algebraic_type(ctx, ty, "")).unwrap();
                writeln!(
                    output,
                    "let {arg_name}Value = AlgebraicValue.deserialize({arg_name}Type, adapter.next())"
                )
                .unwrap();
                writeln!(
                    output,
                    "let {arg_name} = {};",
                    convert_type(ctx, 0, ty, format_args!("{arg_name}Value"), "")
                )
                .unwrap();

                arg_names.push(arg_name);
            }

            writeln!(output, "return [{}];", arg_names.join(", ")).unwrap();
        }

        writeln!(output, "}}").unwrap();

        writeln!(output).unwrap();

        writeln!(
            output,
            "public static on(callback: (reducerEvent: ReducerEvent, {}) => void) {{",
            func_arguments.join(", ")
        )
        .unwrap();
        {
            indent_scope!(output);

            writeln!(output, "this.getReducer().on(callback);").unwrap();
        }
        writeln!(output, "}}").unwrap();

        // OnCreatePlayerEvent(dbEvent.Status, Identity.From(dbEvent.CallerIdentity.ToByteArray()), args[0].ToObject<string>());
        writeln!(
            output,
            "public on(callback: (reducerEvent: ReducerEvent, {}) => void)",
            func_arguments.join(", ")
        )
        .unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);

            writeln!(
                output,
                "this.client.on(\"reducer:{reducer_name_pascal_case}\", callback);"
            )
            .unwrap();
        }

        // Closing brace for Event parsing function
        writeln!(output, "}}").unwrap();
    }
    // Closing brace for class
    writeln!(output, "}}").unwrap();

    writeln!(output).unwrap();

    writeln!(output, "\nexport default {reducer_name_pascal_case}Reducer").unwrap();

    output.into_inner()
}

pub fn autogen_typescript_globals(_ctx: &GenCtx, _items: &[GenItem]) -> Vec<Vec<(String, String)>> {
    vec![] //TODO
}
