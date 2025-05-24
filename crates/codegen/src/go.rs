use crate::Lang;
use convert_case::Casing;
use spacetimedb_schema::def::{ModuleDef, ReducerDef, ScopedTypeName, TableDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse, PrimitiveType};
use std::ops::Deref;

/// Go language generator for SpacetimeDB client bindings
pub struct Go {
    /// Package name for generated code
    pub package_name: String,
}

impl Default for Go {
    fn default() -> Self {
        Self {
            package_name: "spacetimedb".to_string(),
        }
    }
}

impl Go {
    /// Create a new Go generator with a custom package name
    pub fn new(package_name: String) -> Self {
        Self { package_name }
    }

    /// Convert a SpacetimeDB type to Go type
    fn go_type(&self, ty: &AlgebraicTypeUse, module: &ModuleDef) -> String {
        match ty {
            AlgebraicTypeUse::Unit => "struct{}".to_string(),
            AlgebraicTypeUse::String => "string".to_string(),
            AlgebraicTypeUse::Primitive(primitive) => match primitive {
                PrimitiveType::Bool => "bool".to_string(),
                PrimitiveType::I8 => "int8".to_string(),
                PrimitiveType::U8 => "uint8".to_string(),
                PrimitiveType::I16 => "int16".to_string(),
                PrimitiveType::U16 => "uint16".to_string(),
                PrimitiveType::I32 => "int32".to_string(),
                PrimitiveType::U32 => "uint32".to_string(),
                PrimitiveType::I64 => "int64".to_string(),
                PrimitiveType::U64 => "uint64".to_string(),
                PrimitiveType::I128 => "[16]uint8".to_string(), // Go doesn't have native i128
                PrimitiveType::U128 => "[16]uint8".to_string(), // Go doesn't have native u128
                PrimitiveType::I256 => "[32]uint8".to_string(), // Go doesn't have native i256
                PrimitiveType::U256 => "[32]uint8".to_string(), // Go doesn't have native u256
                PrimitiveType::F32 => "float32".to_string(),
                PrimitiveType::F64 => "float64".to_string(),
            },
            AlgebraicTypeUse::Array(elem_ty) => {
                format!("[]{}", self.go_type(elem_ty, module))
            }
            AlgebraicTypeUse::Option(inner) => {
                format!("*{}", self.go_type(inner, module))
            }
            AlgebraicTypeUse::Ref(type_ref) => {
                // Use the type_ref_name function like in the existing codegens
                crate::util::type_ref_name(module, *type_ref)
            }
            AlgebraicTypeUse::Identity => "spacetimedb.Identity".to_string(),
            AlgebraicTypeUse::ConnectionId => "spacetimedb.ConnectionId".to_string(),
            AlgebraicTypeUse::Timestamp => "spacetimedb.Timestamp".to_string(),
            AlgebraicTypeUse::TimeDuration => "spacetimedb.TimeDuration".to_string(),
            AlgebraicTypeUse::ScheduleAt => "spacetimedb.ScheduleAt".to_string(),
            AlgebraicTypeUse::Never => "interface{}".to_string(), // Never type as empty interface
        }
    }

    /// Generate combined struct tags for JSON and BSATN
    fn struct_tags(&self, field_name: &str) -> String {
        format!("`json:\"{}\" bsatn:\"{}\"`", field_name.to_lowercase(), field_name.to_lowercase())
    }

    /// Convert identifier to Go-style PascalCase
    fn pascal_case(&self, name: &str) -> String {
        name.to_case(convert_case::Case::Pascal)
    }

    /// Generate file header with package and imports
    fn file_header(&self, imports: &[&str]) -> String {
        let mut result = format!("// {}\n", crate::util::AUTO_GENERATED_PREFIX);
        result.push_str(&format!("package {}\n\n", self.package_name));
        
        if !imports.is_empty() {
            result.push_str("import (\n");
            for import in imports {
                result.push_str(&format!("\t\"{}\"\n", import));
            }
            result.push_str(")\n\n");
        }
        
        result
    }

    /// Generate reducer function signature
    fn generate_reducer_signature(&self, reducer: &ReducerDef) -> String {
        let func_name = self.pascal_case(reducer.name.deref());
        let args_part = if reducer.params_for_generate.elements.is_empty() {
            String::new()
        } else {
            format!(", args {}Args", func_name)
        };
        
        format!("func {}(ctx *spacetimedb.ReducerContext{}) error", func_name, args_part)
    }

    /// Generate reducer arguments struct
    fn generate_reducer_args(&self, reducer: &ReducerDef, module: &ModuleDef) -> String {
        if reducer.params_for_generate.elements.is_empty() {
            return String::new();
        }

        let mut result = String::new();
        let struct_name = format!("{}Args", self.pascal_case(reducer.name.deref()));
        
        result.push_str(&format!("// {} contains arguments for the {} reducer\n", struct_name, reducer.name));
        result.push_str(&format!("type {} struct {{\n", struct_name));
        
        for (field_ident, field_ty) in reducer.params_for_generate.elements.iter() {
            let field_name = self.pascal_case(field_ident.deref());
            let field_type = self.go_type(field_ty, module);
            let tags = self.struct_tags(&field_name);
            
            result.push_str(&format!("\t{} {} {}\n", field_name, field_type, tags));
        }
        
        result.push_str("}\n\n");
        result
    }
}

impl Lang for Go {
    fn table_filename(&self, _module: &ModuleDef, table: &TableDef) -> String {
        format!("{}.go", table.name.deref().to_lowercase())
    }

    fn type_filename(&self, type_name: &ScopedTypeName) -> String {
        format!("{}.go", crate::util::collect_case(convert_case::Case::Snake, type_name.name_segments()))
    }

    fn reducer_filename(&self, reducer_name: &Identifier) -> String {
        format!("{}_reducer.go", reducer_name.deref().to_lowercase())
    }

    fn generate_table(&self, module: &ModuleDef, table: &TableDef) -> String {
        let mut result = self.file_header(&[
            "github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb",
        ]);

        let table_name = self.pascal_case(table.name.deref());
        let product_def = module.typespace_for_generate()[table.product_type_ref].as_product().unwrap();
        
        // Generate table struct
        result.push_str(&format!("// {} represents the {} table\n", table_name, table.name));
        result.push_str(&format!("type {} struct {{\n", table_name));
        
        for (field_ident, field_ty) in &product_def.elements {
            let field_name = self.pascal_case(field_ident.deref());
            let field_type = self.go_type(field_ty, module);
            let tags = self.struct_tags(&field_name);
            
            result.push_str(&format!("\t{} {} {}\n", field_name, field_type, tags));
        }
        
        result.push_str("}\n\n");

        // Generate table registration function
        result.push_str(&format!("// Register{} registers the {} table with SpacetimeDB\n", table_name, table.name));
        result.push_str(&format!("func Register{}() error {{\n", table_name));
        result.push_str(&format!("\ttable := spacetimedb.NewTableInfo(\"{}\")\n", table.name));
        
        for (field_ident, field_ty) in &product_def.elements {
            let col_name = field_ident.deref();
            let col_type = self.go_type(field_ty, module);
            result.push_str(&format!("\ttable.Columns = append(table.Columns, spacetimedb.NewColumn(\"{}\", \"{}\"))\n", col_name, col_type));
        }
        
        result.push_str("\treturn spacetimedb.GlobalRegisterTable(table)\n");
        result.push_str("}\n\n");

        // Generate BSATN methods
        result.push_str(&format!("// ToBSATN serializes {} to BSATN format\n", table_name));
        result.push_str(&format!("func (t *{}) ToBSATN() ([]byte, error) {{\n", table_name));
        result.push_str("\treturn spacetimedb.BsatnToBytes(t)\n");
        result.push_str("}\n\n");

        result.push_str(&format!("// FromBSATN deserializes {} from BSATN format\n", table_name));
        result.push_str(&format!("func (t *{}) FromBSATN(data []byte) error {{\n", table_name));
        result.push_str("\treturn spacetimedb.BsatnFromBytes(data, t)\n");
        result.push_str("}\n\n");

        result
    }

    fn generate_type(&self, module: &ModuleDef, typ: &TypeDef) -> String {
        let mut result = self.file_header(&[
            "github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb",
        ]);

        let type_name = crate::util::collect_case(convert_case::Case::Pascal, typ.name.name_segments());
        
        match &module.typespace_for_generate()[typ.ty] {
            AlgebraicTypeDef::Product(product) => {
                result.push_str(&format!("// {} represents a product type\n", type_name));
                result.push_str(&format!("type {} struct {{\n", type_name));
                
                for (field_ident, field_ty) in &product.elements {
                    let field_name = self.pascal_case(field_ident.deref());
                    let field_type = self.go_type(field_ty, module);
                    let tags = self.struct_tags(&field_name);
                    
                    result.push_str(&format!("\t{} {} {}\n", field_name, field_type, tags));
                }
                
                result.push_str("}\n\n");
            }
            
            AlgebraicTypeDef::Sum(sum) => {
                result.push_str(&format!("// {} represents a sum type (enum)\n", type_name));
                result.push_str(&format!("type {} interface {{\n", type_name));
                result.push_str(&format!("\tIs{}() bool\n", type_name));
                result.push_str("}\n\n");
                
                for (variant_ident, variant_ty) in &sum.variants {
                    let variant_name = self.pascal_case(variant_ident.deref());
                    let variant_type_name = format!("{}{}", type_name, variant_name);
                    
                    result.push_str(&format!("// {} represents the {} variant of {}\n", variant_type_name, variant_name, type_name));
                    
                    match variant_ty {
                        AlgebraicTypeUse::Unit => {
                            result.push_str(&format!("type {} struct {{}}\n\n", variant_type_name));
                        }
                        _ => {
                            result.push_str(&format!("type {} struct {{\n", variant_type_name));
                            result.push_str(&format!("\tValue {} `json:\"value\" bsatn:\"value\"`\n", self.go_type(variant_ty, module)));
                            result.push_str("}\n\n");
                        }
                    }
                    
                    // Implement interface method
                    result.push_str(&format!("// Is{} implements the {} interface\n", type_name, type_name));
                    result.push_str(&format!("func (v *{}) Is{}() bool {{\n", variant_type_name, type_name));
                    result.push_str("\treturn true\n");
                    result.push_str("}\n\n");
                }
            }
            
            AlgebraicTypeDef::PlainEnum(plain_enum) => {
                result.push_str(&format!("// {} represents a plain enum\n", type_name));
                result.push_str(&format!("type {} uint8\n\n", type_name));
                
                result.push_str("const (\n");
                for (i, variant) in plain_enum.variants.iter().enumerate() {
                    let variant_name = self.pascal_case(variant.deref());
                    if i == 0 {
                        result.push_str(&format!("\t{}{} {} = iota\n", type_name, variant_name, type_name));
                    } else {
                        result.push_str(&format!("\t{}{}\n", type_name, variant_name));
                    }
                }
                result.push_str(")\n\n");
            }
        }

        result
    }

    fn generate_reducer(&self, module: &ModuleDef, reducer: &ReducerDef) -> String {
        let mut result = self.file_header(&[
            "github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb",
        ]);

        // Generate reducer arguments struct if needed
        result.push_str(&self.generate_reducer_args(reducer, module));

        // Generate reducer function signature
        let func_name = self.pascal_case(reducer.name.deref());
        result.push_str(&format!("// {} is a SpacetimeDB reducer\n", func_name));
        result.push_str(&format!("// TODO: Implement your reducer logic here\n"));
        result.push_str(&self.generate_reducer_signature(reducer));
        result.push_str(" {\n");
        result.push_str("\t// TODO: Add your reducer implementation here\n");
        result.push_str("\treturn nil\n");
        result.push_str("}\n\n");

        // Generate reducer registration
        result.push_str(&format!("// Register{} registers the {} reducer with SpacetimeDB\n", func_name, reducer.name));
        result.push_str(&format!("func Register{}() {{\n", func_name));
        
        if reducer.params_for_generate.elements.is_empty() {
            result.push_str(&format!("\tspacetimedb.RegisterReducer(\"{}\", \"{}\", func(ctx *spacetimedb.ReducerContext, args []byte) spacetimedb.ReducerResult {{\n", reducer.name, func_name));
            result.push_str(&format!("\t\tif err := {}(ctx); err != nil {{\n", func_name));
            result.push_str("\t\t\treturn spacetimedb.ReducerResult{Error: err}\n");
            result.push_str("\t\t}\n");
            result.push_str("\t\treturn spacetimedb.ReducerResult{}\n");
            result.push_str("\t})\n");
        } else {
            let args_type = format!("{}Args", func_name);
            result.push_str(&format!("\tspacetimedb.RegisterReducer(\"{}\", \"{}\", func(ctx *spacetimedb.ReducerContext, args []byte) spacetimedb.ReducerResult {{\n", reducer.name, func_name));
            result.push_str(&format!("\t\tvar parsedArgs {}\n", args_type));
            result.push_str("\t\tif err := spacetimedb.BsatnFromBytes(args, &parsedArgs); err != nil {\n");
            result.push_str("\t\t\treturn spacetimedb.ReducerResult{Error: err}\n");
            result.push_str("\t\t}\n");
            result.push_str(&format!("\t\tif err := {}(ctx, parsedArgs); err != nil {{\n", func_name));
            result.push_str("\t\t\treturn spacetimedb.ReducerResult{Error: err}\n");
            result.push_str("\t\t}\n");
            result.push_str("\t\treturn spacetimedb.ReducerResult{}\n");
            result.push_str("\t})\n");
        }
        
        result.push_str("}\n\n");

        result
    }

    fn generate_globals(&self, module: &ModuleDef) -> Vec<(String, String)> {
        let mut globals = Vec::new();

        // Generate module registration file
        let mut module_file = self.file_header(&[
            "github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb",
        ]);

        module_file.push_str("// RegisterModule registers all tables and reducers for this SpacetimeDB module\n");
        module_file.push_str("func RegisterModule() error {\n");
        
        // Register all tables
        for table in crate::util::iter_tables(module) {
            let table_name = self.pascal_case(table.name.deref());
            module_file.push_str(&format!("\tif err := Register{}(); err != nil {{\n", table_name));
            module_file.push_str(&format!("\t\treturn err\n"));
            module_file.push_str("\t}\n");
        }
        
        // Register all reducers
        for reducer in crate::util::iter_reducers(module) {
            let func_name = self.pascal_case(reducer.name.deref());
            module_file.push_str(&format!("\tRegister{}()\n", func_name));
        }
        
        module_file.push_str("\treturn nil\n");
        module_file.push_str("}\n\n");

        // Generate init function
        module_file.push_str("// init automatically registers the module when the package is imported\n");
        module_file.push_str("func init() {\n");
        module_file.push_str("\tif err := RegisterModule(); err != nil {\n");
        module_file.push_str("\t\tpanic(\"Failed to register SpacetimeDB module: \" + err.Error())\n");
        module_file.push_str("\t}\n");
        module_file.push_str("}\n");

        globals.push(("module.go".to_string(), module_file));

        globals
    }
} 