use super::{STDB_ABI_IS_ADDR_SYM, STDB_ABI_SYM};
use anyhow::Context;
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
pub fn determine_spacetime_abi(wasm_module: &[u8]) -> anyhow::Result<SpacetimeAbiVersion> {
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

    let err_msg = "module doesn't indicate spacetime ABI version";

    let (globals, exports) = globals.zip(exports).context(err_msg)?;

    // TODO: wasmparser Validator should provide access to exports map? bytecodealliance/wasm-tools#806

    let exports = exports
        .into_iter()
        .map(Result::unwrap)
        .map(|exp| (exp.name, exp))
        .collect::<HashMap<_, _>>();

    let export = exports.get(STDB_ABI_SYM).context(err_msg)?;
    // LLVM can only output statics as addresses into the data section currently, so we need to do
    // resolve the address if it is one
    let export_is_addr = exports.contains_key(STDB_ABI_IS_ADDR_SYM);

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
    let ver = if export_is_addr {
        let mut datas = datas.context("SPACETIME_ABI_VERSION is an address but there's no data")?;
        let data = datas.read().unwrap();
        let wasmparser::DataKind::Active { memory_index, offset_expr } = data.kind else {
            anyhow::bail!("passive data segment")
        };
        anyhow::ensure!(memory_index == 0, "non-0 memory_index");
        let offset_op = offset_expr.get_operators_reader().read().unwrap();
        let wasmparser::Operator::I32Const { value: offset } = offset_op else { unreachable!() };
        let slice = value
            .checked_sub(offset)
            .and_then(|idx| data.data.get(idx as usize..)?.get(..4))
            .context("abi version addr out of bounds")?;
        u32::from_le_bytes(slice.try_into().unwrap())
    } else {
        value as u32
    };
    let ver = VersionTuple::from(ver);
    let ver = SpacetimeAbiVersion::from_tuple(ver)
        .ok_or_else(|| anyhow::anyhow!("unknown ABI version (too new?): {ver:x?}"))?;
    Ok(ver)
}
