use super::STDB_ABI_SYM;
use anyhow::Context;

const ABIVER_GLOBAL_TY: wasmparser::GlobalType = wasmparser::GlobalType {
    content_type: wasmparser::ValType::I32,
    mutable: false,
};

#[derive(Copy, Clone)]
pub enum SpacetimeAbiVersion {
    V0,
}
impl SpacetimeAbiVersion {
    pub fn from_u32(v: u32) -> Option<Self> {
        let function_abi_ver = v & 0xFFFF_FFFF;
        let schema_abi_ver = v >> 16;
        const _: () = assert!(spacetimedb_lib::SCHEMA_FORMAT_VERSION == 0);
        let ver = match (function_abi_ver, schema_abi_ver) {
            (0, 0) => Self::V0,
            _ => return None,
        };
        Some(ver)
    }
}

/// wasm-runtime-agnostic function to extract spacetime ABI version from a module
/// wasm_module must be a valid wasm module
pub fn determine_spacetime_abi(wasm_module: &[u8]) -> anyhow::Result<SpacetimeAbiVersion> {
    let mut parser = wasmparser::Parser::new(0);
    let mut data = wasm_module;
    let (mut globals, mut exports) = (None, None);
    loop {
        let payload = match parser.parse(data, true).unwrap() {
            wasmparser::Chunk::NeedMoreData(_) => unreachable!(),
            wasmparser::Chunk::Parsed { consumed, payload } => {
                data = &data[consumed..];
                payload
            }
        };
        match payload {
            wasmparser::Payload::GlobalSection(rdr) => globals = Some(rdr),
            wasmparser::Payload::ExportSection(rdr) => exports = Some(rdr),
            wasmparser::Payload::CodeSectionStart { size, .. } => {
                data = &data[size as usize..];
                parser.skip_section()
            }
            wasmparser::Payload::End(_) => break,
            _ => {}
        }
    }

    let err_msg = "module doesn't indicate spacetime ABI version";

    let (globals, exports) = globals.zip(exports).context(err_msg)?;

    // TODO: wasmparser Validator should provide access to exports map? bytecodealliance/wasm-tools#806

    let export = exports
        .into_iter()
        .map(Result::unwrap)
        .find(|exp| exp.name == STDB_ABI_SYM)
        .context(err_msg)?;

    anyhow::ensure!(export.kind == wasmparser::ExternalKind::Global, "{err_msg}");

    let global = globals
        .into_iter()
        .map(Result::unwrap)
        .nth(export.index as usize)
        .unwrap();

    anyhow::ensure!(global.ty == ABIVER_GLOBAL_TY, "{STDB_ABI_SYM} is wrong type");
    let mut op_rdr = global.init_expr.get_operators_reader();
    let wasmparser::Operator::I32Const { value } = op_rdr.read()? else {
        anyhow::bail!("invalid const expr for ABI version")
    };
    let ver =
        SpacetimeAbiVersion::from_u32(value as u32).ok_or_else(|| anyhow::anyhow!("invalid ABI version: {value:x}"))?;
    Ok(ver)
}
