//! Canonical platform Wasm for a container database without a user module.
//!
//! The declared environment changes the program identity. Verification extracts
//! only bounded declaration data, then regenerates and compares every byte. It
//! never executes publisher-supplied code or trusts an empty-looking schema.

use crate::db::raw_def::v10::{RawModuleDefV10, RawModuleDefV10Section};
use crate::environment::{
    EnvironmentConstraint, EnvironmentDeclaration, EnvironmentSchema, MAX_ENV_KEY_BYTES, MAX_ENV_SCHEMA_BYTES,
    MAX_ENV_UNION_ENTRIES, MAX_ENV_VALUE_BYTES, MAX_ENV_VARS,
};
use crate::{bsatn, hash_bytes, Hash, RawModuleDef, SpacetimeType};

pub const VERSION: u32 = 2;
const WASM_HEADER: &[u8] = b"\0asm\x01\0\0\0";
// String bytes have their own aggregate limit. Include every possible BSATN
// length prefix, constraint tag, optional flag, and enclosing module section.
const MAX_METADATA_BYTES: usize = MAX_ENV_SCHEMA_BYTES + MAX_ENV_VARS * (10 + MAX_ENV_UNION_ENTRIES * 4) + 128;
pub const MAX_PROGRAM_BYTES: usize = MAX_METADATA_BYTES + 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SystemEmptyModule {
    pub version: u32,
    pub program_hash: Hash,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid platform container module")]
pub struct InvalidSystemModule;

type ModuleResult<T> = std::result::Result<T, InvalidSystemModule>;

pub struct GeneratedModule {
    pub descriptor: SystemEmptyModule,
    pub bytes: Box<[u8]>,
}

/// The canonical empty declaration schema used for an initial Keep or explicit
/// module removal without container environment declarations.
pub fn empty() -> &'static GeneratedModule {
    static EMPTY: std::sync::OnceLock<GeneratedModule> = std::sync::OnceLock::new();
    EMPTY.get_or_init(|| generate(&EnvironmentSchema::default()).expect("empty platform schema must encode"))
}

/// Generate the exact versioned program for a previously validated schema.
pub fn generate(environment: &EnvironmentSchema) -> ModuleResult<GeneratedModule> {
    let metadata = RawModuleDef::V10(RawModuleDefV10 {
        sections: vec![
            RawModuleDefV10Section::Typespace(Default::default()),
            RawModuleDefV10Section::Environment(environment.declarations().cloned().collect()),
            RawModuleDefV10Section::Capabilities(vec!["hosted_auth_v1".into()]),
        ],
    });
    let metadata = bsatn::to_vec(&metadata).map_err(|_| InvalidSystemModule)?;
    if metadata.len() > MAX_METADATA_BYTES {
        return Err(InvalidSystemModule);
    }
    // The size pointer is at16 and metadata starts at32. Fix both minimum and
    // maximum memory to the smallest page count containing this exact schema.
    let pages = (32 + metadata.len()).div_ceil(65536);
    let mut wasm = WASM_HEADER.to_vec();
    section(
        &mut wasm,
        1,
        &[
            4, 0x60, 3, 0x7f, 0x7f, 0x7f, 1, 0x7f, 0x60, 0, 1, 0x7f, 0x60, 1, 0x7f, 0, 0x60, 10, 0x7f, 0x7e, 0x7e,
            0x7e, 0x7e, 0x7e, 0x7e, 0x7e, 0x7f, 0x7f, 1, 0x7f,
        ],
    );
    let mut imports = vec![2];
    name(&mut imports, b"spacetime_10.0");
    name(&mut imports, b"bytes_sink_write");
    imports.extend([0, 0]);
    name(&mut imports, b"spacetime_10.7");
    name(&mut imports, b"get_call_auth_flags");
    imports.extend([0, 1]);
    section(&mut wasm, 2, &imports);
    section(&mut wasm, 3, &[2, 2, 3]);
    let mut memory = vec![1, 1];
    leb(&mut memory, pages);
    leb(&mut memory, pages);
    section(&mut wasm, 5, &memory);
    let mut exports = vec![3];
    name(&mut exports, b"memory");
    exports.extend([2, 0]);
    name(&mut exports, b"__describe_module__");
    exports.extend([0, 2]);
    name(&mut exports, b"__call_reducer__");
    exports.extend([0, 3]);
    section(&mut wasm, 7, &exports);
    // __describe_module__(sink): bytes_sink_write(sink,32,16), trap on error.
    // __call_reducer__: unreachable, since there are no declared reducers.
    section(
        &mut wasm,
        10,
        &[
            2, 14, 0, 0x20, 0, 0x41, 0x20, 0x41, 0x10, 0x10, 0, 0x04, 0x40, 0, 0x0b, 0x0b, 3, 0, 0, 0x0b,
        ],
    );
    let mut data = vec![2, 0, 0x41, 0x10, 0x0b, 4];
    data.extend((metadata.len() as u32).to_le_bytes());
    data.extend([0, 0x41, 0x20, 0x0b]);
    leb(&mut data, metadata.len());
    data.extend(metadata);
    section(&mut wasm, 11, &data);
    Ok(GeneratedModule {
        descriptor: SystemEmptyModule {
            version: VERSION,
            program_hash: hash_bytes(&wasm),
        },
        bytes: wasm.into(),
    })
}

/// Verify the version, hash, declarations, executable code, exports, imports,
/// memory limits, and absence of additional sections without running the module.
pub fn verify(descriptor: &SystemEmptyModule, bytes: &[u8]) -> ModuleResult<EnvironmentSchema> {
    if descriptor.version != VERSION || bytes.len() > MAX_PROGRAM_BYTES {
        return Err(InvalidSystemModule);
    }
    let environment = read_environment(bytes)?;
    let expected = generate(&environment)?;
    if expected.descriptor != *descriptor || expected.bytes.as_ref() != bytes {
        return Err(InvalidSystemModule);
    }
    Ok(environment)
}

fn read_environment(bytes: &[u8]) -> ModuleResult<EnvironmentSchema> {
    let mut wasm = Reader(bytes);
    wasm.expect(WASM_HEADER)?;
    let mut data = None;
    while !wasm.0.is_empty() {
        let tag = wasm.byte()?;
        let len = wasm.leb()?;
        let payload = wasm.take(len)?;
        if tag == 11 && data.replace(payload).is_some() {
            return Err(InvalidSystemModule);
        }
    }
    let mut data = Reader(data.ok_or(InvalidSystemModule)?);
    data.expect(&[2, 0, 0x41, 0x10, 0x0b, 4])?;
    let size = data.u32()?;
    if size > MAX_METADATA_BYTES {
        return Err(InvalidSystemModule);
    }
    data.expect(&[0, 0x41, 0x20, 0x0b])?;
    if data.leb()? != size {
        return Err(InvalidSystemModule);
    }
    let mut metadata = Reader(data.take(size)?);
    data.end()?;
    // RawModuleDef::V10, exactly3 sections: empty Typespace, ENV15, Capabilities16.
    metadata.expect(&[2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 15])?;
    let count = metadata.count(MAX_ENV_VARS)?;
    let mut declarations = Vec::with_capacity(count);
    let mut string_bytes = 0;
    for _ in 0..count {
        let name = metadata.string(MAX_ENV_KEY_BYTES, &mut string_bytes)?;
        let constraint = match metadata.byte()? {
            0 => EnvironmentConstraint::AnyString,
            1 => EnvironmentConstraint::Literal(metadata.string(MAX_ENV_VALUE_BYTES, &mut string_bytes)?),
            2 => {
                let count = metadata.count(MAX_ENV_UNION_ENTRIES)?;
                let mut values = Vec::with_capacity(count);
                for _ in 0..count {
                    values.push(metadata.string(MAX_ENV_VALUE_BYTES, &mut string_bytes)?);
                }
                EnvironmentConstraint::OneOf(values)
            }
            _ => return Err(InvalidSystemModule),
        };
        let optional = match metadata.byte()? {
            0 => false,
            1 => true,
            _ => return Err(InvalidSystemModule),
        };
        declarations.push(EnvironmentDeclaration {
            name,
            constraint,
            optional,
        });
    }
    metadata.expect(&[16, 1, 0, 0, 0, 14, 0, 0, 0])?;
    metadata.expect(b"hosted_auth_v1")?;
    metadata.end()?;
    EnvironmentSchema::new(declarations).map_err(|_| InvalidSystemModule)
}

fn leb(out: &mut Vec<u8>, mut value: usize) {
    loop {
        let byte = (value & 127) as u8;
        value >>= 7;
        out.push(byte | if value == 0 { 0 } else { 128 });
        if value == 0 {
            break;
        }
    }
}

fn name(out: &mut Vec<u8>, value: &[u8]) {
    leb(out, value.len());
    out.extend(value);
}
fn section(out: &mut Vec<u8>, tag: u8, payload: &[u8]) {
    out.push(tag);
    name(out, payload);
}

/// This reader accepts only the platform program's declaration framing, not
/// arbitrary Wasm or BSATN. Every length/count is bounded before allocation.
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> ModuleResult<&'a [u8]> {
        let (head, tail) = self.0.split_at_checked(len).ok_or(InvalidSystemModule)?;
        self.0 = tail;
        Ok(head)
    }
    fn byte(&mut self) -> ModuleResult<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> ModuleResult<usize> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize)
    }
    fn count(&mut self, limit: usize) -> ModuleResult<usize> {
        let count = self.u32()?;
        if count > limit {
            return Err(InvalidSystemModule);
        }
        Ok(count)
    }
    fn leb(&mut self) -> ModuleResult<usize> {
        let mut value = 0u32;
        for shift in (0..35).step_by(7) {
            let byte = self.byte()?;
            if shift == 28 && byte > 15 {
                return Err(InvalidSystemModule);
            }
            value |= u32::from(byte & 127) << shift;
            if byte & 128 == 0 {
                return Ok(value as usize);
            }
        }
        Err(InvalidSystemModule)
    }
    fn string(&mut self, limit: usize, total: &mut usize) -> ModuleResult<String> {
        let len = self.count(limit)?;
        *total += len;
        if *total > MAX_ENV_SCHEMA_BYTES {
            return Err(InvalidSystemModule);
        }
        let bytes = self.take(len)?;
        Ok(std::str::from_utf8(bytes).map_err(|_| InvalidSystemModule)?.to_owned())
    }
    fn expect(&mut self, bytes: &[u8]) -> ModuleResult<()> {
        if self.take(bytes.len())? != bytes {
            return Err(InvalidSystemModule);
        }
        Ok(())
    }
    fn end(&self) -> ModuleResult<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(InvalidSystemModule)
        }
    }
}

#[cfg(test)]
#[path = "system_empty/tests.rs"]
mod tests;
