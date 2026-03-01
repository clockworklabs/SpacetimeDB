// Note: the generated code depends on APIs and interfaces from sdks/go.
use super::util::fmt_fn;

use std::fmt;
use std::ops::Deref;

use super::code_indenter::CodeIndenter;
use super::Lang;
use crate::util::{
    collect_case, is_reducer_invokable, iter_reducers, iter_table_names_and_types, iter_unique_cols,
    print_auto_generated_file_comment, print_auto_generated_version_comment, type_ref_name,
};
use crate::{CodegenOptions, OutputFile};
use convert_case::{Case, Casing};
use spacetimedb_lib::sats::layout::PrimitiveType;
use spacetimedb_schema::def::{ModuleDef, ProcedureDef, ReducerDef, TableDef, TypeDef};
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_schema::type_for_generate::{
    AlgebraicTypeDef, AlgebraicTypeUse, PlainEnumTypeDef, ProductTypeDef, SumTypeDef,
};

const INDENT: &str = "\t";

const SDK_IMPORT: &str = "stdb \"github.com/clockworklabs/SpacetimeDB/sdks/go\"";

pub struct Go;

impl Lang for Go {
    fn generate_table_file_from_schema(
        &self,
        module: &ModuleDef,
        table: &TableDef,
        schema: TableSchema,
    ) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_go_header(out);

        let type_name = table.accessor_name.deref().to_case(Case::Pascal);
        let product_def = module.typespace_for_generate()[table.product_type_ref]
            .as_product()
            .unwrap();

        // Table handle type
        writeln!(out, "// {type_name}TableHandle provides access to the {type_name} table.");
        writeln!(out, "type {type_name}TableHandle struct {{");
        out.with_indent(|out| {
            writeln!(out, "conn stdb.TableAccessor");
        });
        writeln!(out, "}}");
        writeln!(out);

        // Count method
        writeln!(out, "func (t *{type_name}TableHandle) Count() int {{");
        out.with_indent(|out| {
            writeln!(out, "return t.conn.TableRowCount(\"{}\") ", table.name);
        });
        writeln!(out, "}}");
        writeln!(out);

        // Iter method
        writeln!(
            out,
            "func (t *{type_name}TableHandle) Iter(fn func(row *{type_name}) bool) {{"
        );
        out.with_indent(|out| {
            writeln!(out, "t.conn.TableIter(\"{}\", func(reader stdb.Reader) bool {{", table.name);
            out.with_indent(|out| {
                writeln!(out, "row, err := Read{type_name}(reader)");
                writeln!(out, "if err != nil {{");
                out.with_indent(|out| {
                    writeln!(out, "return false");
                });
                writeln!(out, "}}");
                writeln!(out, "return fn(row)");
            });
            writeln!(out, "}})");
        });
        writeln!(out, "}}");
        writeln!(out);

        // Generate unique column find methods
        for (col_name, col_type) in iter_unique_cols(
            module.typespace_for_generate(),
            &schema,
            product_def,
        ) {
            let col_name_pascal = col_name.deref().to_case(Case::Pascal);
            let col_type_str = ty_fmt(module, col_type);

            writeln!(
                out,
                "func (t *{type_name}TableHandle) FindBy{col_name_pascal}(val {col_type_str}) (*{type_name}, bool) {{"
            );
            out.with_indent(|out| {
                writeln!(
                    out,
                    "var result *{type_name}"
                );
                writeln!(out, "found := false");
                writeln!(out, "t.Iter(func(row *{type_name}) bool {{");
                out.with_indent(|out| {
                    writeln!(out, "if row.{col_name_pascal} == val {{");
                    out.with_indent(|out| {
                        writeln!(out, "result = row");
                        writeln!(out, "found = true");
                        writeln!(out, "return false");
                    });
                    writeln!(out, "}}");
                    writeln!(out, "return true");
                });
                writeln!(out, "}})");
                writeln!(out, "return result, found");
            });
            writeln!(out, "}}");
            writeln!(out);
        }

        // OnInsert callback
        writeln!(
            out,
            "func (t *{type_name}TableHandle) OnInsert(cb func(ctx stdb.EventContext, row *{type_name})) stdb.CallbackId {{"
        );
        out.with_indent(|out| {
            writeln!(
                out,
                "return t.conn.OnInsert(\"{}\", func(ctx stdb.EventContext, reader stdb.Reader) {{",
                table.name
            );
            out.with_indent(|out| {
                writeln!(out, "row, err := Read{type_name}(reader)");
                writeln!(out, "if err != nil {{");
                out.with_indent(|out| {
                    writeln!(out, "return");
                });
                writeln!(out, "}}");
                writeln!(out, "cb(ctx, row)");
            });
            writeln!(out, "}})");
        });
        writeln!(out, "}}");
        writeln!(out);

        // OnDelete callback
        writeln!(
            out,
            "func (t *{type_name}TableHandle) OnDelete(cb func(ctx stdb.EventContext, row *{type_name})) stdb.CallbackId {{"
        );
        out.with_indent(|out| {
            writeln!(
                out,
                "return t.conn.OnDelete(\"{}\", func(ctx stdb.EventContext, reader stdb.Reader) {{",
                table.name
            );
            out.with_indent(|out| {
                writeln!(out, "row, err := Read{type_name}(reader)");
                writeln!(out, "if err != nil {{");
                out.with_indent(|out| {
                    writeln!(out, "return");
                });
                writeln!(out, "}}");
                writeln!(out, "cb(ctx, row)");
            });
            writeln!(out, "}})");
        });
        writeln!(out, "}}");

        let filename = format!(
            "{}_table.go",
            table.accessor_name.deref().to_case(Case::Snake)
        );

        OutputFile {
            filename,
            code: output.into_inner(),
        }
    }

    fn generate_type_files(&self, module: &ModuleDef, typ: &TypeDef) -> Vec<OutputFile> {
        let name = collect_case(Case::Pascal, typ.accessor_name.name_segments());
        let filename = format!("{}_type.go", collect_case(Case::Snake, typ.accessor_name.name_segments()));
        let code = match &module.typespace_for_generate()[typ.ty] {
            AlgebraicTypeDef::Product(prod) => gen_go_product(module, &name, prod),
            AlgebraicTypeDef::Sum(sum) => gen_go_sum(module, &name, sum),
            AlgebraicTypeDef::PlainEnum(plain_enum) => gen_go_plain_enum(&name, plain_enum),
        };

        vec![OutputFile { filename, code }]
    }

    fn generate_reducer_file(&self, module: &ModuleDef, reducer: &ReducerDef) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_go_header(out);

        let func_name = reducer.accessor_name.deref().to_case(Case::Pascal);

        if is_reducer_invokable(reducer) {
            // Generate the call method on RemoteReducers
            write!(out, "func (r *RemoteReducers) {func_name}(");
            for (i, (arg_name, arg_type)) in reducer.params_for_generate.elements.iter().enumerate()
            {
                if i != 0 {
                    write!(out, ", ");
                }
                let arg_name_camel = arg_name.deref().to_case(Case::Camel);
                write!(out, "{arg_name_camel} {}", ty_fmt(module, arg_type));
            }
            writeln!(out, ") {{");
            out.with_indent(|out| {
                // Build args for the BSATN call
                writeln!(out, "args := func(w stdb.Writer) {{");
                out.with_indent(|out| {
                    for (arg_name, arg_type) in reducer.params_for_generate.elements.iter() {
                        let arg_name_camel = arg_name.deref().to_case(Case::Camel);
                        write_bsatn_encode(out, module, &arg_name_camel, arg_type);
                    }
                });
                writeln!(out, "}}");
                writeln!(out, "r.conn.CallReducer(\"{}\", args)", reducer.name);
            });
            writeln!(out, "}}");
            writeln!(out);
        }

        // Generate the callback registration method
        write!(
            out,
            "func (r *RemoteReducers) On{func_name}(cb func(ctx stdb.ReducerEventContext"
        );
        for (arg_name, arg_type) in reducer.params_for_generate.elements.iter() {
            let arg_name_camel = arg_name.deref().to_case(Case::Camel);
            write!(out, ", {} {}", arg_name_camel, ty_fmt(module, arg_type));
        }
        writeln!(out, ")) stdb.CallbackId {{");
        out.with_indent(|out| {
            writeln!(
                out,
                "return r.conn.OnReducer(\"{}\", func(ctx stdb.ReducerEventContext, reader stdb.Reader) {{",
                reducer.name
            );
            out.with_indent(|out| {
                for (arg_name, arg_type) in reducer.params_for_generate.elements.iter() {
                    let arg_name_camel = arg_name.deref().to_case(Case::Camel);
                    write_bsatn_decode(out, module, &arg_name_camel, arg_type);
                }
                write!(out, "cb(ctx");
                for (arg_name, _) in reducer.params_for_generate.elements.iter() {
                    let arg_name_camel = arg_name.deref().to_case(Case::Camel);
                    write!(out, ", {arg_name_camel}");
                }
                writeln!(out, ")");
            });
            writeln!(out, "}})");
        });
        writeln!(out, "}}");

        let filename = format!(
            "{}_reducer.go",
            reducer.accessor_name.deref().to_case(Case::Snake)
        );

        OutputFile {
            filename,
            code: output.into_inner(),
        }
    }

    fn generate_procedure_file(&self, module: &ModuleDef, procedure: &ProcedureDef) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_go_header(out);

        let func_name = procedure.accessor_name.deref().to_case(Case::Pascal);
        let return_type_str = ty_fmt(module, &procedure.return_type_for_generate);

        // Generate the call method on RemoteProcedures
        write!(out, "func (p *RemoteProcedures) {func_name}(");
        for (i, (arg_name, arg_type)) in procedure.params_for_generate.elements.iter().enumerate() {
            if i != 0 {
                write!(out, ", ");
            }
            let arg_name_camel = arg_name.deref().to_case(Case::Camel);
            write!(out, "{arg_name_camel} {}", ty_fmt(module, arg_type));
        }
        if !procedure.params_for_generate.elements.is_empty() {
            write!(out, ", ");
        }
        writeln!(out, "cb func(ctx stdb.ProcedureEventContext, result {return_type_str}, err error)) {{");
        out.with_indent(|out| {
            writeln!(out, "args := func(w stdb.Writer) {{");
            out.with_indent(|out| {
                for (arg_name, arg_type) in procedure.params_for_generate.elements.iter() {
                    let arg_name_camel = arg_name.deref().to_case(Case::Camel);
                    write_bsatn_encode(out, module, &arg_name_camel, arg_type);
                }
            });
            writeln!(out, "}}");
            writeln!(
                out,
                "p.conn.CallProcedure(\"{}\", args, func(ctx stdb.ProcedureEventContext, reader stdb.Reader) {{",
                procedure.name
            );
            out.with_indent(|out| {
                writeln!(out, "// Decode the return value");
                let return_var = "result";
                write_bsatn_decode(out, module, return_var, &procedure.return_type_for_generate);
                writeln!(out, "cb(ctx, {return_var}, nil)");
            });
            writeln!(out, "}})");
        });
        writeln!(out, "}}");

        let filename = format!(
            "{}_procedure.go",
            procedure.accessor_name.deref().to_case(Case::Snake)
        );

        OutputFile {
            filename,
            code: output.into_inner(),
        }
    }

    fn generate_global_files(&self, module: &ModuleDef, options: &CodegenOptions) -> Vec<OutputFile> {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_auto_generated_file_comment(out);
        print_auto_generated_version_comment(out);
        writeln!(out, "package module_bindings");
        writeln!(out);
        writeln!(out, "import (");
        out.with_indent(|out| {
            writeln!(out, "{SDK_IMPORT}");
        });
        writeln!(out, ")");
        writeln!(out);

        // RemoteTables struct
        writeln!(out, "// RemoteTables provides access to all tables in the module.");
        writeln!(out, "type RemoteTables struct {{");
        out.with_indent(|out| {
            for (_, accessor_name, _) in iter_table_names_and_types(module, options.visibility) {
                let table_name_pascal = accessor_name.deref().to_case(Case::Pascal);
                writeln!(out, "{table_name_pascal} *{table_name_pascal}TableHandle");
            }
        });
        writeln!(out, "}}");
        writeln!(out);

        // RemoteReducers struct
        writeln!(out, "// RemoteReducers provides access to all reducers in the module.");
        writeln!(out, "type RemoteReducers struct {{");
        out.with_indent(|out| {
            writeln!(out, "conn stdb.DbConnection");
        });
        writeln!(out, "}}");
        writeln!(out);

        // RemoteProcedures struct
        writeln!(out, "// RemoteProcedures provides access to all procedures in the module.");
        writeln!(out, "type RemoteProcedures struct {{");
        out.with_indent(|out| {
            writeln!(out, "conn stdb.DbConnection");
        });
        writeln!(out, "}}");
        writeln!(out);

        // Reducer enum type
        writeln!(out, "// Reducer identifies a reducer in the module.");
        writeln!(out, "type Reducer uint32");
        writeln!(out);
        writeln!(out, "const (");
        out.with_indent(|out| {
            let mut first = true;
            for reducer in iter_reducers(module, options.visibility) {
                let reducer_name = reducer.accessor_name.deref().to_case(Case::Pascal);
                if first {
                    writeln!(out, "Reducer{reducer_name} Reducer = iota");
                    first = false;
                } else {
                    writeln!(out, "Reducer{reducer_name}");
                }
            }
        });
        writeln!(out, ")");
        writeln!(out);

        // DbConnection struct
        writeln!(out, "// DbConnection represents a connection to a SpacetimeDB module.");
        writeln!(out, "type DbConnection struct {{");
        out.with_indent(|out| {
            writeln!(out, "stdb.DbConnection");
            writeln!(out, "Db         *RemoteTables");
            writeln!(out, "Reducers   *RemoteReducers");
            writeln!(out, "Procedures *RemoteProcedures");
        });
        writeln!(out, "}}");
        writeln!(out);

        // NewDbConnection function
        writeln!(out, "// NewDbConnection creates a new DbConnectionBuilder for connecting to a SpacetimeDB module.");
        writeln!(out, "func NewDbConnection() stdb.DbConnectionBuilder {{");
        out.with_indent(|out| {
            writeln!(out, "return stdb.NewDbConnectionBuilder()");
        });
        writeln!(out, "}}");

        vec![OutputFile {
            filename: "module_bindings.go".to_owned(),
            code: output.into_inner(),
        }]
    }
}

// --- Helper functions ---

/// Print the Go file header with auto-generated comment, package declaration, and import.
fn print_go_header(out: &mut CodeIndenter<String>) {
    print_auto_generated_file_comment(out);
    writeln!(out, "package module_bindings");
    writeln!(out);
    writeln!(out, "import (");
    out.with_indent(|out| {
        writeln!(out, "{SDK_IMPORT}");
    });
    writeln!(out, ")");
    writeln!(out);
}

/// Format an `AlgebraicTypeUse` as a Go type string.
fn ty_fmt<'a>(module: &'a ModuleDef, ty: &'a AlgebraicTypeUse) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicTypeUse::Identity => f.write_str("stdb.Identity"),
        AlgebraicTypeUse::ConnectionId => f.write_str("stdb.ConnectionId"),
        AlgebraicTypeUse::ScheduleAt => f.write_str("stdb.ScheduleAt"),
        AlgebraicTypeUse::Timestamp => f.write_str("stdb.Timestamp"),
        AlgebraicTypeUse::TimeDuration => f.write_str("stdb.TimeDuration"),
        AlgebraicTypeUse::Uuid => f.write_str("stdb.Uuid"),
        AlgebraicTypeUse::Unit => f.write_str("struct{}"),
        AlgebraicTypeUse::Option(inner_ty) => write!(f, "*{}", ty_fmt(module, inner_ty)),
        AlgebraicTypeUse::Result { ok_ty, err_ty } => {
            write!(
                f,
                "stdb.Result[{}, {}]",
                ty_fmt(module, ok_ty),
                ty_fmt(module, err_ty)
            )
        }
        AlgebraicTypeUse::Array(elem_ty) => {
            // Special case: []byte for Array(U8)
            if matches!(elem_ty.as_ref(), AlgebraicTypeUse::Primitive(PrimitiveType::U8)) {
                f.write_str("[]byte")
            } else {
                write!(f, "[]{}", ty_fmt(module, elem_ty))
            }
        }
        AlgebraicTypeUse::String => f.write_str("string"),
        AlgebraicTypeUse::Ref(r) => f.write_str(&type_ref_name(module, *r)),
        AlgebraicTypeUse::Primitive(prim) => f.write_str(match prim {
            PrimitiveType::Bool => "bool",
            PrimitiveType::I8 => "int8",
            PrimitiveType::U8 => "uint8",
            PrimitiveType::I16 => "int16",
            PrimitiveType::U16 => "uint16",
            PrimitiveType::I32 => "int32",
            PrimitiveType::U32 => "uint32",
            PrimitiveType::I64 => "int64",
            PrimitiveType::U64 => "uint64",
            PrimitiveType::I128 => "stdb.Int128",
            PrimitiveType::U128 => "stdb.Uint128",
            PrimitiveType::I256 => "stdb.Int256",
            PrimitiveType::U256 => "stdb.Uint256",
            PrimitiveType::F32 => "float32",
            PrimitiveType::F64 => "float64",
        }),
        AlgebraicTypeUse::Never => unimplemented!("never types are not yet supported in Go output"),
    })
}

/// Generate a Go product type (struct).
fn gen_go_product(module: &ModuleDef, name: &str, product: &ProductTypeDef) -> String {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;

    print_go_header(out);

    // Struct definition
    writeln!(out, "type {name} struct {{");
    out.with_indent(|out| {
        for (field_name, field_type) in product.elements.iter() {
            let go_field_name = field_name.deref().to_case(Case::Pascal);
            let go_type = ty_fmt(module, field_type);
            let tag_name = field_name.deref();
            writeln!(out, "{go_field_name} {go_type} `stdb:\"{tag_name}\"`");
        }
    });
    writeln!(out, "}}");
    writeln!(out);

    // WriteBsatn method
    writeln!(out, "func (v *{name}) WriteBsatn(w stdb.Writer) {{");
    out.with_indent(|out| {
        for (field_name, field_type) in product.elements.iter() {
            let go_field_name = field_name.deref().to_case(Case::Pascal);
            let accessor = format!("v.{go_field_name}");
            write_bsatn_encode(out, module, &accessor, field_type);
        }
    });
    writeln!(out, "}}");
    writeln!(out);

    // Read function
    writeln!(out, "func Read{name}(r stdb.Reader) (*{name}, error) {{");
    out.with_indent(|out| {
        writeln!(out, "var v {name}");
        writeln!(out, "var err error");
        for (field_name, field_type) in product.elements.iter() {
            let go_field_name = field_name.deref().to_case(Case::Pascal);
            write_bsatn_field_decode(out, module, &go_field_name, field_type);
        }
        writeln!(out, "return &v, nil");
    });
    writeln!(out, "}}");

    output.into_inner()
}

/// Generate a Go sum type (tagged union via interface).
fn gen_go_sum(module: &ModuleDef, name: &str, sum: &SumTypeDef) -> String {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;

    print_go_header(out);

    let marker_method = format!("is{name}");

    // Interface definition
    writeln!(out, "type {name} interface {{");
    out.with_indent(|out| {
        writeln!(out, "{marker_method}()");
        writeln!(out, "Tag() uint8");
    });
    writeln!(out, "}}");
    writeln!(out);

    // Generate variant types
    for (tag, (variant_name, variant_type)) in sum.variants.iter().enumerate() {
        let variant_pascal = variant_name.deref().to_case(Case::Pascal);
        let full_name = format!("{name}{variant_pascal}");

        match variant_type {
            AlgebraicTypeUse::Unit => {
                writeln!(out, "type {full_name} struct{{}}");
            }
            _ => {
                writeln!(out, "type {full_name} struct {{");
                out.with_indent(|out| {
                    writeln!(out, "Value {}", ty_fmt(module, variant_type));
                });
                writeln!(out, "}}");
            }
        }
        writeln!(out);

        writeln!(out, "func ({full_name}) {marker_method}() {{}}");
        writeln!(out, "func ({full_name}) Tag() uint8 {{ return {tag} }}");
        writeln!(out);
    }

    // Write function
    writeln!(out, "func Write{name}(w stdb.Writer, val {name}) {{");
    out.with_indent(|out| {
        writeln!(out, "w.PutU8(val.Tag())");
        writeln!(out, "switch v := val.(type) {{");
        for (variant_name, variant_type) in sum.variants.iter() {
            let variant_pascal = variant_name.deref().to_case(Case::Pascal);
            let full_name = format!("{name}{variant_pascal}");

            writeln!(out, "case {full_name}:");
            if !matches!(variant_type, AlgebraicTypeUse::Unit) {
                out.with_indent(|out| {
                    write_bsatn_encode(out, module, "v.Value", variant_type);
                });
            }
        }
        writeln!(out, "}}");
    });
    writeln!(out, "}}");
    writeln!(out);

    // Read function
    writeln!(out, "func Read{name}(r stdb.Reader) ({name}, error) {{");
    out.with_indent(|out| {
        writeln!(out, "tag, err := r.GetU8()");
        writeln!(out, "if err != nil {{");
        out.with_indent(|out| {
            writeln!(out, "return nil, err");
        });
        writeln!(out, "}}");
        writeln!(out, "switch tag {{");
        for (tag, (variant_name, variant_type)) in sum.variants.iter().enumerate() {
            let variant_pascal = variant_name.deref().to_case(Case::Pascal);
            let full_name = format!("{name}{variant_pascal}");

            writeln!(out, "case {tag}:");
            out.with_indent(|out| {
                if matches!(variant_type, AlgebraicTypeUse::Unit) {
                    writeln!(out, "return {full_name}{{}}, nil");
                } else {
                    write_bsatn_decode(out, module, "val", variant_type);
                    writeln!(out, "return {full_name}{{Value: val}}, nil");
                }
            });
        }
        writeln!(out, "default:");
        out.with_indent(|out| {
            writeln!(
                out,
                "return nil, stdb.ErrUnknownTag(\"{name}\", tag)"
            );
        });
        writeln!(out, "}}");
    });
    writeln!(out, "}}");

    output.into_inner()
}

/// Generate a Go plain enum (all unit variants).
fn gen_go_plain_enum(name: &str, plain_enum: &PlainEnumTypeDef) -> String {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;

    print_go_header(out);

    writeln!(out, "type {name} uint8");
    writeln!(out);
    writeln!(out, "const (");
    out.with_indent(|out| {
        for (i, variant) in plain_enum.variants.iter().enumerate() {
            let variant_pascal = variant.deref().to_case(Case::Pascal);
            if i == 0 {
                writeln!(out, "{name}{variant_pascal} {name} = iota");
            } else {
                writeln!(out, "{name}{variant_pascal}");
            }
        }
    });
    writeln!(out, ")");
    writeln!(out);

    // String method
    writeln!(out, "func (e {name}) String() string {{");
    out.with_indent(|out| {
        writeln!(out, "switch e {{");
        for variant in plain_enum.variants.iter() {
            let variant_pascal = variant.deref().to_case(Case::Pascal);
            let variant_str = variant.deref();
            writeln!(out, "case {name}{variant_pascal}:");
            out.with_indent(|out| {
                writeln!(out, "return \"{variant_str}\"");
            });
        }
        writeln!(out, "default:");
        out.with_indent(|out| {
            writeln!(out, "return \"unknown\"");
        });
        writeln!(out, "}}");
    });
    writeln!(out, "}}");
    writeln!(out);

    // WriteBsatn method
    writeln!(out, "func (e {name}) WriteBsatn(w stdb.Writer) {{");
    out.with_indent(|out| {
        writeln!(out, "w.PutU8(uint8(e))");
    });
    writeln!(out, "}}");
    writeln!(out);

    // Read function
    writeln!(out, "func Read{name}(r stdb.Reader) ({name}, error) {{");
    out.with_indent(|out| {
        writeln!(out, "val, err := r.GetU8()");
        writeln!(out, "if err != nil {{");
        out.with_indent(|out| {
            writeln!(out, "return 0, err");
        });
        writeln!(out, "}}");
        writeln!(out, "return {name}(val), nil");
    });
    writeln!(out, "}}");

    output.into_inner()
}

/// Write BSATN encoding for a given Go expression and type.
fn write_bsatn_encode(
    out: &mut CodeIndenter<String>,
    module: &ModuleDef,
    expr: &str,
    ty: &AlgebraicTypeUse,
) {
    match ty {
        AlgebraicTypeUse::Primitive(prim) => {
            let method = match prim {
                PrimitiveType::Bool => "PutBool",
                PrimitiveType::I8 => "PutI8",
                PrimitiveType::U8 => "PutU8",
                PrimitiveType::I16 => "PutI16",
                PrimitiveType::U16 => "PutU16",
                PrimitiveType::I32 => "PutI32",
                PrimitiveType::U32 => "PutU32",
                PrimitiveType::I64 => "PutI64",
                PrimitiveType::U64 => "PutU64",
                PrimitiveType::I128 => "PutI128",
                PrimitiveType::U128 => "PutU128",
                PrimitiveType::I256 => "PutI256",
                PrimitiveType::U256 => "PutU256",
                PrimitiveType::F32 => "PutF32",
                PrimitiveType::F64 => "PutF64",
            };
            writeln!(out, "w.{method}({expr})");
        }
        AlgebraicTypeUse::String => {
            writeln!(out, "w.PutString({expr})");
        }
        AlgebraicTypeUse::Identity => {
            writeln!(out, "{expr}.WriteBsatn(w)");
        }
        AlgebraicTypeUse::ConnectionId => {
            writeln!(out, "{expr}.WriteBsatn(w)");
        }
        AlgebraicTypeUse::Timestamp => {
            writeln!(out, "{expr}.WriteBsatn(w)");
        }
        AlgebraicTypeUse::TimeDuration => {
            writeln!(out, "{expr}.WriteBsatn(w)");
        }
        AlgebraicTypeUse::ScheduleAt => {
            writeln!(out, "{expr}.WriteBsatn(w)");
        }
        AlgebraicTypeUse::Uuid => {
            writeln!(out, "{expr}.WriteBsatn(w)");
        }
        AlgebraicTypeUse::Unit => {
            // Nothing to write for unit
        }
        AlgebraicTypeUse::Ref(r) => {
            let type_def = &module.typespace_for_generate()[r];
            match type_def {
                AlgebraicTypeDef::Product(_) => {
                    writeln!(out, "{expr}.WriteBsatn(w)");
                }
                AlgebraicTypeDef::Sum(_) => {
                    let type_name = type_ref_name(module, *r);
                    writeln!(out, "Write{type_name}(w, {expr})");
                }
                AlgebraicTypeDef::PlainEnum(_) => {
                    writeln!(out, "{expr}.WriteBsatn(w)");
                }
            }
        }
        AlgebraicTypeUse::Array(elem_ty) => {
            if matches!(elem_ty.as_ref(), AlgebraicTypeUse::Primitive(PrimitiveType::U8)) {
                writeln!(out, "w.PutBytes({expr})");
            } else {
                writeln!(out, "w.PutU32(uint32(len({expr})))");
                writeln!(out, "for _, item := range {expr} {{");
                out.with_indent(|out| {
                    write_bsatn_encode(out, module, "item", elem_ty);
                });
                writeln!(out, "}}");
            }
        }
        AlgebraicTypeUse::Option(inner_ty) => {
            writeln!(out, "if {expr} != nil {{");
            out.with_indent(|out| {
                writeln!(out, "w.PutU8(1)");
                write_bsatn_encode(out, module, &format!("*{expr}"), inner_ty);
            });
            writeln!(out, "}} else {{");
            out.with_indent(|out| {
                writeln!(out, "w.PutU8(0)");
            });
            writeln!(out, "}}");
        }
        AlgebraicTypeUse::Result { .. } => {
            writeln!(out, "{expr}.WriteBsatn(w)");
        }
        AlgebraicTypeUse::Never => unimplemented!("never types are not yet supported in Go output"),
    }
}

/// Write BSATN decoding for a type, binding the result to a variable name.
fn write_bsatn_decode(
    out: &mut CodeIndenter<String>,
    module: &ModuleDef,
    var_name: &str,
    ty: &AlgebraicTypeUse,
) {
    match ty {
        AlgebraicTypeUse::Primitive(prim) => {
            let method = match prim {
                PrimitiveType::Bool => "GetBool",
                PrimitiveType::I8 => "GetI8",
                PrimitiveType::U8 => "GetU8",
                PrimitiveType::I16 => "GetI16",
                PrimitiveType::U16 => "GetU16",
                PrimitiveType::I32 => "GetI32",
                PrimitiveType::U32 => "GetU32",
                PrimitiveType::I64 => "GetI64",
                PrimitiveType::U64 => "GetU64",
                PrimitiveType::I128 => "GetI128",
                PrimitiveType::U128 => "GetU128",
                PrimitiveType::I256 => "GetI256",
                PrimitiveType::U256 => "GetU256",
                PrimitiveType::F32 => "GetF32",
                PrimitiveType::F64 => "GetF64",
            };
            writeln!(out, "{var_name}, _ := r.{method}()");
        }
        AlgebraicTypeUse::String => {
            writeln!(out, "{var_name}, _ := r.GetString()");
        }
        AlgebraicTypeUse::Identity => {
            writeln!(out, "{var_name}, _ := stdb.ReadIdentity(r)");
        }
        AlgebraicTypeUse::ConnectionId => {
            writeln!(out, "{var_name}, _ := stdb.ReadConnectionId(r)");
        }
        AlgebraicTypeUse::Timestamp => {
            writeln!(out, "{var_name}, _ := stdb.ReadTimestamp(r)");
        }
        AlgebraicTypeUse::TimeDuration => {
            writeln!(out, "{var_name}, _ := stdb.ReadTimeDuration(r)");
        }
        AlgebraicTypeUse::ScheduleAt => {
            writeln!(out, "{var_name}, _ := stdb.ReadScheduleAt(r)");
        }
        AlgebraicTypeUse::Uuid => {
            writeln!(out, "{var_name}, _ := stdb.ReadUuid(r)");
        }
        AlgebraicTypeUse::Unit => {
            writeln!(out, "{var_name} := struct{{}}{{}}");
        }
        AlgebraicTypeUse::Ref(r) => {
            let type_name = type_ref_name(module, *r);
            let type_def = &module.typespace_for_generate()[r];
            match type_def {
                AlgebraicTypeDef::Product(_) => {
                    writeln!(out, "{var_name}, _ := Read{type_name}(r)");
                }
                AlgebraicTypeDef::Sum(_) => {
                    writeln!(out, "{var_name}, _ := Read{type_name}(r)");
                }
                AlgebraicTypeDef::PlainEnum(_) => {
                    writeln!(out, "{var_name}, _ := Read{type_name}(r)");
                }
            }
        }
        AlgebraicTypeUse::Array(elem_ty) => {
            if matches!(elem_ty.as_ref(), AlgebraicTypeUse::Primitive(PrimitiveType::U8)) {
                writeln!(out, "{var_name}, _ := r.GetBytes()");
            } else {
                let elem_type_str = ty_fmt(module, elem_ty);
                writeln!(out, "{var_name}Len, _ := r.GetU32()");
                writeln!(
                    out,
                    "{var_name} := make([]{elem_type_str}, {var_name}Len)"
                );
                writeln!(out, "for i := uint32(0); i < {var_name}Len; i++ {{");
                out.with_indent(|out| {
                    write_bsatn_decode(out, module, "elem", elem_ty);
                    writeln!(out, "{var_name}[i] = elem");
                });
                writeln!(out, "}}");
            }
        }
        AlgebraicTypeUse::Option(inner_ty) => {
            let inner_type_str = ty_fmt(module, inner_ty);
            writeln!(out, "{var_name}Tag, _ := r.GetU8()");
            writeln!(out, "var {var_name} *{inner_type_str}");
            writeln!(out, "if {var_name}Tag == 1 {{");
            out.with_indent(|out| {
                write_bsatn_decode(out, module, &format!("{var_name}Val"), inner_ty);
                writeln!(out, "{var_name} = &{var_name}Val");
            });
            writeln!(out, "}}");
        }
        AlgebraicTypeUse::Result { .. } => {
            writeln!(out, "{var_name}, _ := stdb.ReadResult(r)");
        }
        AlgebraicTypeUse::Never => unimplemented!("never types are not yet supported in Go output"),
    }
}

/// Write BSATN field decoding for a struct field (with error handling).
fn write_bsatn_field_decode(
    out: &mut CodeIndenter<String>,
    module: &ModuleDef,
    field_name: &str,
    ty: &AlgebraicTypeUse,
) {
    match ty {
        AlgebraicTypeUse::Primitive(prim) => {
            let method = match prim {
                PrimitiveType::Bool => "GetBool",
                PrimitiveType::I8 => "GetI8",
                PrimitiveType::U8 => "GetU8",
                PrimitiveType::I16 => "GetI16",
                PrimitiveType::U16 => "GetU16",
                PrimitiveType::I32 => "GetI32",
                PrimitiveType::U32 => "GetU32",
                PrimitiveType::I64 => "GetI64",
                PrimitiveType::U64 => "GetU64",
                PrimitiveType::I128 => "GetI128",
                PrimitiveType::U128 => "GetU128",
                PrimitiveType::I256 => "GetI256",
                PrimitiveType::U256 => "GetU256",
                PrimitiveType::F32 => "GetF32",
                PrimitiveType::F64 => "GetF64",
            };
            writeln!(
                out,
                "if v.{field_name}, err = r.{method}(); err != nil {{ return nil, err }}"
            );
        }
        AlgebraicTypeUse::String => {
            writeln!(
                out,
                "if v.{field_name}, err = r.GetString(); err != nil {{ return nil, err }}"
            );
        }
        AlgebraicTypeUse::Identity => {
            writeln!(out, "{{");
            out.with_indent(|out| {
                writeln!(out, "val, readErr := stdb.ReadIdentity(r)");
                writeln!(out, "if readErr != nil {{ return nil, readErr }}");
                writeln!(out, "v.{field_name} = val");
            });
            writeln!(out, "}}");
        }
        AlgebraicTypeUse::ConnectionId => {
            writeln!(out, "{{");
            out.with_indent(|out| {
                writeln!(out, "val, readErr := stdb.ReadConnectionId(r)");
                writeln!(out, "if readErr != nil {{ return nil, readErr }}");
                writeln!(out, "v.{field_name} = val");
            });
            writeln!(out, "}}");
        }
        AlgebraicTypeUse::Timestamp => {
            writeln!(out, "{{");
            out.with_indent(|out| {
                writeln!(out, "val, readErr := stdb.ReadTimestamp(r)");
                writeln!(out, "if readErr != nil {{ return nil, readErr }}");
                writeln!(out, "v.{field_name} = val");
            });
            writeln!(out, "}}");
        }
        AlgebraicTypeUse::TimeDuration => {
            writeln!(out, "{{");
            out.with_indent(|out| {
                writeln!(out, "val, readErr := stdb.ReadTimeDuration(r)");
                writeln!(out, "if readErr != nil {{ return nil, readErr }}");
                writeln!(out, "v.{field_name} = val");
            });
            writeln!(out, "}}");
        }
        AlgebraicTypeUse::ScheduleAt => {
            writeln!(out, "{{");
            out.with_indent(|out| {
                writeln!(out, "val, readErr := stdb.ReadScheduleAt(r)");
                writeln!(out, "if readErr != nil {{ return nil, readErr }}");
                writeln!(out, "v.{field_name} = val");
            });
            writeln!(out, "}}");
        }
        AlgebraicTypeUse::Uuid => {
            writeln!(out, "{{");
            out.with_indent(|out| {
                writeln!(out, "val, readErr := stdb.ReadUuid(r)");
                writeln!(out, "if readErr != nil {{ return nil, readErr }}");
                writeln!(out, "v.{field_name} = val");
            });
            writeln!(out, "}}");
        }
        AlgebraicTypeUse::Unit => {
            // Nothing to read for unit
        }
        AlgebraicTypeUse::Ref(r) => {
            let type_name = type_ref_name(module, *r);
            let type_def = &module.typespace_for_generate()[r];
            match type_def {
                AlgebraicTypeDef::Product(_) => {
                    writeln!(out, "{{");
                    out.with_indent(|out| {
                        writeln!(out, "val, readErr := Read{type_name}(r)");
                        writeln!(out, "if readErr != nil {{ return nil, readErr }}");
                        writeln!(out, "v.{field_name} = *val");
                    });
                    writeln!(out, "}}");
                }
                AlgebraicTypeDef::Sum(_) => {
                    writeln!(out, "{{");
                    out.with_indent(|out| {
                        writeln!(out, "val, readErr := Read{type_name}(r)");
                        writeln!(out, "if readErr != nil {{ return nil, readErr }}");
                        writeln!(out, "v.{field_name} = val");
                    });
                    writeln!(out, "}}");
                }
                AlgebraicTypeDef::PlainEnum(_) => {
                    writeln!(out, "{{");
                    out.with_indent(|out| {
                        writeln!(out, "val, readErr := Read{type_name}(r)");
                        writeln!(out, "if readErr != nil {{ return nil, readErr }}");
                        writeln!(out, "v.{field_name} = val");
                    });
                    writeln!(out, "}}");
                }
            }
        }
        AlgebraicTypeUse::Array(elem_ty) => {
            if matches!(elem_ty.as_ref(), AlgebraicTypeUse::Primitive(PrimitiveType::U8)) {
                writeln!(out, "{{");
                out.with_indent(|out| {
                    writeln!(out, "val, readErr := r.GetBytes()");
                    writeln!(out, "if readErr != nil {{ return nil, readErr }}");
                    writeln!(out, "v.{field_name} = val");
                });
                writeln!(out, "}}");
            } else {
                let elem_type_str = ty_fmt(module, elem_ty);
                writeln!(out, "{{");
                out.with_indent(|out| {
                    writeln!(out, "arrLen, readErr := r.GetU32()");
                    writeln!(out, "if readErr != nil {{ return nil, readErr }}");
                    writeln!(out, "v.{field_name} = make([]{elem_type_str}, arrLen)");
                    writeln!(out, "for i := uint32(0); i < arrLen; i++ {{");
                    out.with_indent(|out| {
                        write_bsatn_field_decode_inner(out, module, &format!("{field_name}[i]"), elem_ty);
                    });
                    writeln!(out, "}}");
                });
                writeln!(out, "}}");
            }
        }
        AlgebraicTypeUse::Option(inner_ty) => {
            let inner_type_str = ty_fmt(module, inner_ty);
            writeln!(out, "{{");
            out.with_indent(|out| {
                writeln!(out, "optTag, readErr := r.GetU8()");
                writeln!(out, "if readErr != nil {{ return nil, readErr }}");
                writeln!(out, "if optTag == 1 {{");
                out.with_indent(|out| {
                    writeln!(out, "var optVal {inner_type_str}");
                    writeln!(out, "_ = optVal");
                    write_bsatn_field_decode_inner(out, module, "optVal", inner_ty);
                    writeln!(out, "v.{field_name} = &optVal");
                });
                writeln!(out, "}}");
            });
            writeln!(out, "}}");
        }
        AlgebraicTypeUse::Result { .. } => {
            writeln!(out, "{{");
            out.with_indent(|out| {
                writeln!(out, "val, readErr := stdb.ReadResult(r)");
                writeln!(out, "if readErr != nil {{ return nil, readErr }}");
                writeln!(out, "v.{field_name} = val");
            });
            writeln!(out, "}}");
        }
        AlgebraicTypeUse::Never => unimplemented!("never types are not yet supported in Go output"),
    }
}

/// Helper for inner array/option field decoding (without the "v." prefix pattern).
fn write_bsatn_field_decode_inner(
    out: &mut CodeIndenter<String>,
    module: &ModuleDef,
    target: &str,
    ty: &AlgebraicTypeUse,
) {
    match ty {
        AlgebraicTypeUse::Primitive(prim) => {
            let method = match prim {
                PrimitiveType::Bool => "GetBool",
                PrimitiveType::I8 => "GetI8",
                PrimitiveType::U8 => "GetU8",
                PrimitiveType::I16 => "GetI16",
                PrimitiveType::U16 => "GetU16",
                PrimitiveType::I32 => "GetI32",
                PrimitiveType::U32 => "GetU32",
                PrimitiveType::I64 => "GetI64",
                PrimitiveType::U64 => "GetU64",
                PrimitiveType::I128 => "GetI128",
                PrimitiveType::U128 => "GetU128",
                PrimitiveType::I256 => "GetI256",
                PrimitiveType::U256 => "GetU256",
                PrimitiveType::F32 => "GetF32",
                PrimitiveType::F64 => "GetF64",
            };
            writeln!(out, "if val, readErr := r.{method}(); readErr != nil {{ return nil, readErr }} else {{ v.{target} = val }}");
        }
        AlgebraicTypeUse::String => {
            writeln!(out, "if val, readErr := r.GetString(); readErr != nil {{ return nil, readErr }} else {{ v.{target} = val }}");
        }
        AlgebraicTypeUse::Ref(r) => {
            let type_name = type_ref_name(module, *r);
            let type_def = &module.typespace_for_generate()[r];
            match type_def {
                AlgebraicTypeDef::Product(_) => {
                    writeln!(out, "if val, readErr := Read{type_name}(r); readErr != nil {{ return nil, readErr }} else {{ v.{target} = *val }}");
                }
                _ => {
                    writeln!(out, "if val, readErr := Read{type_name}(r); readErr != nil {{ return nil, readErr }} else {{ v.{target} = val }}");
                }
            }
        }
        _ => {
            // Fallback: use a block-scoped decode
            writeln!(out, "// TODO: complex nested decode for {target}");
        }
    }
}
