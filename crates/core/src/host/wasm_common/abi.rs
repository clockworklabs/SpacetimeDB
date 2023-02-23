use super::{STDB_ABI_IS_ADDR_SYM, STDB_ABI_SYM};
use std::collections::HashMap;

const ABIVER_GLOBAL_TY: wasmparser::GlobalType = wasmparser::GlobalType {
    content_type: wasmparser::ValType::I32,
    mutable: false,
};

macro_rules! def_abiversion {
    ($($v:ident => $tup:tt,)*) => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
        pub enum SpacetimeAbiVersion {
            $($v,)*
        }

        impl SpacetimeAbiVersion {
            pub const fn from_tuple(tup: VersionTuple) -> Option<Self> {
                match tup {
                    $(VersionTuple::$v => Some(Self::$v),)*
                    _ => None,
                }
            }
            pub const fn as_tuple(self) -> VersionTuple {
                match self {
                    $(Self::$v => VersionTuple::$v,)*
                }
            }
        }

        impl VersionTuple {
            $(const $v: Self = VersionTuple::new $tup;)*
        }
    };
}
spacetimedb_lib::abi_versions!(def_abiversion);

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub struct VersionTuple {
    pub schema_ver: u16,
    pub function_ver: u16,
}
impl VersionTuple {
    const fn new(schema_ver: u16, function_ver: u16) -> Self {
        Self {
            schema_ver,
            function_ver,
        }
    }
}
impl From<u32> for VersionTuple {
    fn from(v: u32) -> Self {
        Self {
            schema_ver: (v >> 16) as u16,
            function_ver: (v & 0xFFFF) as u16,
        }
    }
}
impl From<VersionTuple> for u32 {
    fn from(v: VersionTuple) -> Self {
        (v.schema_ver as u32) << 16 | v.function_ver as u32
    }
}

/// wasm-runtime-agnostic function to extract spacetime ABI version from a module
/// wasm_module must be a valid wasm module
pub fn determine_spacetime_abi(wasm_module: &[u8]) -> Result<SpacetimeAbiVersion, AbiVersionError> {
    let mut parser = wasmparser::Parser::new(0);
    let mut data = wasm_module;
    let (mut globals, mut exports, mut datas) = (None, None, None);
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
        .map(Result::unwrap)
        .map(|exp| (exp.name, exp))
        .collect::<HashMap<_, _>>();

    let export = exports.get(STDB_ABI_SYM).ok_or(AbiVersionError::NoVersion)?;
    // LLVM can only output statics as addresses into the data section currently, so we need to do
    // resolve the address if it is one
    let export_is_addr = exports.contains_key(STDB_ABI_IS_ADDR_SYM);

    if export.kind != wasmparser::ExternalKind::Global {
        return Err(AbiVersionError::NoVersion);
    }

    let global = globals
        .into_iter()
        .map(Result::unwrap)
        .nth(export.index as usize)
        .unwrap();

    if global.ty != ABIVER_GLOBAL_TY {
        return Err(AbiVersionError::Malformed);
    }
    let mut op_rdr = global.init_expr.get_operators_reader();
    let wasmparser::Operator::I32Const { value } = op_rdr.read().unwrap() else {
        return Err(AbiVersionError::Malformed);
    };
    let ver = if export_is_addr {
        let mut datas = datas.ok_or(AbiVersionError::Malformed)?;
        let data = datas.read().unwrap();
        let wasmparser::DataKind::Active { memory_index: 0, offset_expr } = data.kind else {
            return Err(AbiVersionError::Malformed);
        };
        let offset_op = offset_expr.get_operators_reader().read().unwrap();
        let wasmparser::Operator::I32Const { value: offset } = offset_op else { unreachable!() };
        let slice = value
            .checked_sub(offset)
            .and_then(|idx| data.data.get(idx as usize..)?.get(..4))
            .ok_or(AbiVersionError::Malformed)?;
        u32::from_le_bytes(slice.try_into().unwrap())
    } else {
        value as u32
    };
    let ver = VersionTuple::from(ver);
    let ver = SpacetimeAbiVersion::from_tuple(ver).ok_or(AbiVersionError::UnknownVersion(ver))?;
    Ok(ver)
}

#[derive(thiserror::Error, Debug)]
pub enum AbiVersionError {
    #[error("module doesn't indicate spacetime ABI version")]
    NoVersion,
    #[error("abi version is malformed somehow (out-of-bounds, etc)")]
    Malformed,
    #[error("unknown ABI version (too new?): {0:x?}")]
    UnknownVersion(VersionTuple),
    #[error("abi version {0:?} ({:?}) is not supported", .0.as_tuple())]
    UnsupportedVersion(SpacetimeAbiVersion),
}
