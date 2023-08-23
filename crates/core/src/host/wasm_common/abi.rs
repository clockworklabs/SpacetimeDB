use super::{STDB_ABI_IS_ADDR_SYM, STDB_ABI_SYM};
use std::collections::HashMap;

pub use spacetimedb_lib::VersionTuple;

const ABIVER_GLOBAL_TY: wasmparser::GlobalType = wasmparser::GlobalType {
    content_type: wasmparser::ValType::I32,
    mutable: false,
};

/// wasm-runtime-agnostic function to extract spacetime ABI version from a module
/// wasm_module must be a valid wasm module
pub fn determine_spacetime_abi(wasm_module: &[u8]) -> Result<VersionTuple, AbiVersionError> {
    let mut parser = wasmparser::Parser::new(0);
    let mut data = wasm_module;
    let (mut globals, mut exports, mut datas) = (None, None, None);
    loop {
        let payload = match parser.parse(data, true).map_err(|_| AbiVersionError::Malformed)? {
            wasmparser::Chunk::NeedMoreData(_) => unreachable!("determine_spacetime_abi:NeedMoreData"),
            wasmparser::Chunk::Parsed { consumed, payload } => {
                data = &data[consumed..];
                payload
            }
        };
        match payload {
            wasmparser::Payload::GlobalSection(rdr) => globals = Some(rdr),
            wasmparser::Payload::ExportSection(rdr) => exports = Some(rdr),
            wasmparser::Payload::DataSection(rdr) => datas = Some(rdr),
            wasmparser::Payload::CodeSectionStart { size, .. } => {
                data = &data[size as usize..];
                parser.skip_section()
            }
            wasmparser::Payload::End(_) => break,
            _ => {}
        }
    }

    let (globals, exports) = globals.zip(exports).ok_or(AbiVersionError::NoVersion)?;

    // TODO: wasmparser Validator should provide access to exports map? bytecodealliance/wasm-tools#806

    let exports = exports
        .into_iter()
        .map(|exp| {
            let exp = exp.map_err(|_| AbiVersionError::Malformed)?;
            Ok((exp.name, exp))
        })
        .collect::<Result<HashMap<_, _>, AbiVersionError>>()?;

    let export = exports.get(STDB_ABI_SYM).ok_or(AbiVersionError::NoVersion)?;
    // LLVM can only output statics as addresses into the data section currently, so we need to do
    // resolve the address if it is one
    let export_is_addr = exports.contains_key(STDB_ABI_IS_ADDR_SYM);

    if export.kind != wasmparser::ExternalKind::Global {
        return Err(AbiVersionError::NoVersion);
    }

    let global = globals
        .into_iter()
        .nth(export.index as usize)
        .and_then(Result::ok)
        .ok_or(AbiVersionError::Malformed)?;

    if global.ty != ABIVER_GLOBAL_TY {
        return Err(AbiVersionError::Malformed);
    }
    let mut op_rdr = global.init_expr.get_operators_reader();
    let Ok(wasmparser::Operator::I32Const { value }) = op_rdr.read() else {
        return Err(AbiVersionError::Malformed);
    };
    let ver = if export_is_addr {
        datas
            .into_iter()
            .flatten()
            .find_map(|data| {
                let data = data.ok()?;
                let wasmparser::DataKind::Active {
                    memory_index: 0,
                    offset_expr,
                } = data.kind
                else {
                    return None;
                };
                let offset_op = offset_expr.get_operators_reader().read().ok()?;
                let wasmparser::Operator::I32Const { value: offset } = offset_op else {
                    return None;
                };
                let idx = value.checked_sub(offset)?;
                let slice = data.data.get(idx as usize..)?.get(..4)?;
                Some(u32::from_le_bytes(
                    slice.try_into().unwrap(/* we passed constant length above */),
                ))
            })
            .ok_or(AbiVersionError::Malformed)?
    } else {
        value as u32
    };
    Ok(VersionTuple::from_u32(ver))
}

#[derive(thiserror::Error, Debug)]
pub enum AbiVersionError {
    #[error("module doesn't indicate spacetime ABI version")]
    NoVersion,
    #[error("abi version is malformed somehow (out-of-bounds, etc)")]
    Malformed,
    #[error("abi version {got} is not supported (host implements {implement})")]
    UnsupportedVersion { got: VersionTuple, implement: VersionTuple },
}
