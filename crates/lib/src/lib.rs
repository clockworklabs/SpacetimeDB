use crate::db::raw_def::v9::RawModuleDefV9Builder;
use crate::db::raw_def::RawTableDefV8;
use anyhow::Context;
use sats::typespace::TypespaceBuilder;
use spacetimedb_sats::{impl_serialize, WithTypespace};
use std::any::TypeId;
use std::collections::{btree_map, BTreeMap};


macro_rules! non_wasm {
    ($($item:item)*) => {
        $(
            #[cfg(not(target_arch = "wasm32"))]
            $item
        )*
    };
}


//XXX: we avoid anything 'mio' or 'openssl' which will fail to compile in wasm32; this this lib is
//used on both wasm32 during 'spacetime publish' compilation and non-wasm32 ie. x86_64
//#[cfg(not(target_arch = "wasm32"))]
non_wasm! {
use tokio::io::AsyncReadExt;
}


pub mod connection_id;
pub mod db;
mod direct_index_key;
pub mod error;
mod filterable_value;
pub mod identity;
pub mod metrics;
pub mod operator;
pub mod query;
pub mod relation;
pub mod scheduler;
pub mod st_var;
pub mod version;

pub mod type_def {
    pub use spacetimedb_sats::{AlgebraicType, ProductType, ProductTypeElement, SumType};
}
pub mod type_value {
    pub use spacetimedb_sats::{AlgebraicValue, ProductValue};
}

pub use connection_id::ConnectionId;
pub use direct_index_key::{assert_column_type_valid_for_direct_index, DirectIndexKey};
#[doc(hidden)]
pub use filterable_value::Private;
pub use filterable_value::{FilterableValue, IndexScanRangeBoundsTerminator, TermBound};
pub use identity::Identity;
pub use scheduler::ScheduleAt;
pub use spacetimedb_sats::hash::{self, hash_bytes, Hash};
pub use spacetimedb_sats::time_duration::TimeDuration;
pub use spacetimedb_sats::timestamp::Timestamp;
pub use spacetimedb_sats::SpacetimeType;
pub use spacetimedb_sats::__make_register_reftype;
pub use spacetimedb_sats::{self as sats, bsatn, buffer, de, ser};
pub use spacetimedb_sats::{AlgebraicType, ProductType, ProductTypeElement, SumType};
pub use spacetimedb_sats::{AlgebraicValue, ProductValue};

pub const MODULE_ABI_MAJOR_VERSION: u16 = 10;

// if it ends up we need more fields in the future, we can split one of them in two
#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct VersionTuple {
    /// Breaking change; different major versions are not at all compatible with each other.
    pub major: u16,
    /// Non-breaking change; a host can run a module that requests an older minor version than the
    /// host implements, but not the other way around
    pub minor: u16,
}

impl VersionTuple {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    #[inline]
    pub const fn eq(self, other: Self) -> bool {
        self.major == other.major && self.minor == other.minor
    }

    /// Checks if a host implementing this version can run a module that expects `module_version`
    #[inline]
    pub const fn supports(self, module_version: VersionTuple) -> bool {
        self.major == module_version.major && self.minor >= module_version.minor
    }

    #[inline]
    pub const fn from_u32(v: u32) -> Self {
        let major = (v >> 16) as u16;
        let minor = (v & 0xFF) as u16;
        Self { major, minor }
    }

    #[inline]
    pub const fn to_u32(self) -> u32 {
        (self.major as u32) << 16 | self.minor as u32
    }
}

impl std::fmt::Display for VersionTuple {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { major, minor } = *self;
        write!(f, "{major}.{minor}")
    }
}

extern crate self as spacetimedb_lib;

//WARNING: Change this structure(or any of their members) is an ABI change.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, SpacetimeType)]
#[sats(crate = crate)]
pub struct TableDesc {
    pub schema: RawTableDefV8,
    /// data should always point to a ProductType in the typespace
    pub data: sats::AlgebraicTypeRef,
}

impl TableDesc {
    pub fn into_table_def(table: WithTypespace<'_, TableDesc>) -> anyhow::Result<RawTableDefV8> {
        let schema = table
            .map(|t| &t.data)
            .resolve_refs()
            .context("recursive types not yet supported")?;
        let schema = schema.into_product().ok().context("table not a product type?")?;
        let table = table.ty();
        anyhow::ensure!(
            table.schema.columns.len() == schema.elements.len(),
            "mismatched number of columns"
        );

        Ok(table.schema.clone())
    }
}

#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
pub struct ReducerDef {
    pub name: Box<str>,
    pub args: Vec<ProductTypeElement>,
}

impl ReducerDef {
    pub fn encode(&self, writer: &mut impl buffer::BufWriter) {
        bsatn::to_writer(writer, self).unwrap()
    }

    pub fn serialize_args<'a>(ty: sats::WithTypespace<'a, Self>, value: &'a ProductValue) -> impl ser::Serialize + 'a {
        ReducerArgsWithSchema { value, ty }
    }

    pub fn deserialize(
        ty: sats::WithTypespace<'_, Self>,
    ) -> impl for<'de> de::DeserializeSeed<'de, Output = ProductValue> + '_ {
        ReducerDeserialize(ty)
    }
}

struct ReducerDeserialize<'a>(sats::WithTypespace<'a, ReducerDef>);

impl<'de> de::DeserializeSeed<'de> for ReducerDeserialize<'_> {
    type Output = ProductValue;

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        deserializer.deserialize_product(self)
    }
}

impl<'de> de::ProductVisitor<'de> for ReducerDeserialize<'_> {
    type Output = ProductValue;

    fn product_name(&self) -> Option<&str> {
        Some(&self.0.ty().name)
    }
    fn product_len(&self) -> usize {
        self.0.ty().args.len()
    }
    fn product_kind(&self) -> de::ProductKind {
        de::ProductKind::ReducerArgs
    }

    fn visit_seq_product<A: de::SeqProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
        de::visit_seq_product(self.0.map(|r| &*r.args), &self, tup)
    }

    fn visit_named_product<A: de::NamedProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
        de::visit_named_product(self.0.map(|r| &*r.args), &self, tup)
    }
}

struct ReducerArgsWithSchema<'a> {
    value: &'a ProductValue,
    ty: sats::WithTypespace<'a, ReducerDef>,
}
impl_serialize!([] ReducerArgsWithSchema<'_>, (self, ser) => {
    use itertools::Itertools;
    use ser::SerializeSeqProduct;
    let mut seq = ser.serialize_seq_product(self.value.elements.len())?;
    for (value, elem) in self.value.elements.iter().zip_eq(&self.ty.ty().args) {
        seq.serialize_element(&self.ty.with(&elem.algebraic_type).with_value(value))?;
    }
    seq.end()
});

//WARNING: Change this structure (or any of their members) is an ABI change.
#[derive(Debug, Clone, Default, SpacetimeType)]
#[sats(crate = crate)]
pub struct RawModuleDefV8 {
    pub typespace: sats::Typespace,
    pub tables: Vec<TableDesc>,
    pub reducers: Vec<ReducerDef>,
    pub misc_exports: Vec<MiscModuleExport>,
}

impl RawModuleDefV8 {
    pub fn builder() -> ModuleDefBuilder {
        ModuleDefBuilder::default()
    }

    pub fn with_builder(f: impl FnOnce(&mut ModuleDefBuilder)) -> Self {
        let mut builder = Self::builder();
        f(&mut builder);
        builder.finish()
    }
}

/// A versioned raw module definition.
///
/// This is what is actually returned by the module when `__describe_module__` is called, serialized to BSATN.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[non_exhaustive]
pub enum RawModuleDef {
    V8BackCompat(RawModuleDefV8),
    V9(db::raw_def::v9::RawModuleDefV9),
    // TODO(jgilles): It would be nice to have a custom error message if this fails with an unknown variant,
    // but I'm not sure if that can be done via the Deserialize trait.
}

/// A builder for a [`RawModuleDefV8`].
/// Deprecated.
#[derive(Default)]
pub struct ModuleDefBuilder {
    /// The module definition.
    module: RawModuleDefV8,
    /// The type map from `T: 'static` Rust types to sats types.
    type_map: BTreeMap<TypeId, sats::AlgebraicTypeRef>,
}

impl ModuleDefBuilder {
    pub fn add_type<T: SpacetimeType>(&mut self) -> AlgebraicType {
        TypespaceBuilder::add_type::<T>(self)
    }

    /// Add a type that may not correspond to a Rust type.
    /// Used only in tests.
    #[cfg(feature = "test")]
    pub fn add_type_for_tests(&mut self, name: &str, ty: AlgebraicType) -> spacetimedb_sats::AlgebraicTypeRef {
        let slot_ref = self.module.typespace.add(ty);
        self.module.misc_exports.push(MiscModuleExport::TypeAlias(TypeAlias {
            name: name.to_owned(),
            ty: slot_ref,
        }));
        slot_ref
    }

    /// Add a table that may not correspond to a Rust type.
    /// Wraps it in a `TableDesc` and generates a corresponding `ProductType` in the typespace.
    /// Used only in tests.
    /// Returns the `AlgebraicTypeRef` of the generated `ProductType`.
    #[cfg(feature = "test")]
    pub fn add_table_for_tests(&mut self, schema: RawTableDefV8) -> spacetimedb_sats::AlgebraicTypeRef {
        let ty: ProductType = schema
            .columns
            .iter()
            .map(|c| ProductTypeElement {
                name: Some(c.col_name.clone()),
                algebraic_type: c.col_type.clone(),
            })
            .collect();
        let data = self.module.typespace.add(ty.into());
        self.add_type_alias(TypeAlias {
            name: schema.table_name.clone().into(),
            ty: data,
        });
        self.add_table(TableDesc { schema, data });
        data
    }

    pub fn add_table(&mut self, table: TableDesc) {
        self.module.tables.push(table)
    }

    pub fn add_reducer(&mut self, reducer: ReducerDef) {
        self.module.reducers.push(reducer)
    }

    #[cfg(feature = "test")]
    pub fn add_reducer_for_tests(&mut self, name: impl Into<Box<str>>, args: ProductType) {
        self.add_reducer(ReducerDef {
            name: name.into(),
            args: args.elements.to_vec(),
        });
    }

    pub fn add_misc_export(&mut self, misc_export: MiscModuleExport) {
        self.module.misc_exports.push(misc_export)
    }

    pub fn add_type_alias(&mut self, type_alias: TypeAlias) {
        self.add_misc_export(MiscModuleExport::TypeAlias(type_alias))
    }

    pub fn typespace(&self) -> &sats::Typespace {
        &self.module.typespace
    }

    pub fn finish(self) -> RawModuleDefV8 {
        self.module
    }
}

impl TypespaceBuilder for ModuleDefBuilder {
    fn add(
        &mut self,
        typeid: TypeId,
        name: Option<&'static str>,
        make_ty: impl FnOnce(&mut Self) -> AlgebraicType,
    ) -> AlgebraicType {
        let r = match self.type_map.entry(typeid) {
            btree_map::Entry::Occupied(o) => *o.get(),
            btree_map::Entry::Vacant(v) => {
                // Bind a fresh alias to the unit type.
                let slot_ref = self.module.typespace.add(AlgebraicType::unit());
                // Relate `typeid -> fresh alias`.
                v.insert(slot_ref);

                // Alias provided? Relate `name -> slot_ref`.
                if let Some(name) = name {
                    self.module.misc_exports.push(MiscModuleExport::TypeAlias(TypeAlias {
                        name: name.to_owned(),
                        ty: slot_ref,
                    }));
                }

                // Borrow of `v` has ended here, so we can now convince the borrow checker.
                let ty = make_ty(self);
                self.module.typespace[slot_ref] = ty;
                slot_ref
            }
        };
        AlgebraicType::Ref(r)
    }
}

// an enum to keep it extensible without breaking abi
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
pub enum MiscModuleExport {
    TypeAlias(TypeAlias),
}

#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
pub struct TypeAlias {
    pub name: String,
    pub ty: sats::AlgebraicTypeRef,
}

/// Converts a hexadecimal string reference to a byte array.
///
/// This function takes a reference to a hexadecimal string and attempts to convert it into a byte array.
///
/// If the hexadecimal string starts with "0x", these characters are ignored.
pub fn from_hex_pad<R: hex::FromHex<Error = hex::FromHexError>, T: AsRef<[u8]>>(
    hex: T,
) -> Result<R, hex::FromHexError> {
    let hex = hex.as_ref();
    let hex = if hex.starts_with(b"0x") {
        &hex[2..]
    } else if hex.starts_with(b"X'") {
        &hex[2..hex.len()]
    } else {
        hex
    };
    hex::FromHex::from_hex(hex)
}

/// Returns a resolved `AlgebraicType` (containing no `AlgebraicTypeRefs`) for a given `SpacetimeType`,
/// using the v9 moduledef infrastructure.
/// Panics if the type is recursive.
///
/// TODO: we could implement something like this in `sats` itself, but would need a lightweight `TypespaceBuilder` implementation there.
pub fn resolved_type_via_v9<T: SpacetimeType>() -> AlgebraicType {
    let mut builder = RawModuleDefV9Builder::new();
    let ty = T::make_type(&mut builder);
    let module = builder.finish();

    WithTypespace::new(&module.typespace, &ty)
        .resolve_refs()
        .expect("recursive types not supported")
}

//#[cfg(not(target_arch = "wasm32"))]
non_wasm! {
pub async fn load_root_cert(cert_path: Option<&std::path::Path>) -> anyhow::Result<Option<native_tls::Certificate>> {
    if let Some(path) = cert_path {
        // Open file asynchronously
        use tokio::fs::File;
        use tokio::io::{AsyncReadExt, BufReader};
        let file = File::open(path)
            .await
            .context(format!("Failed to open certificate file: {}", path.display()))?;

        // Limit read to 1MiB (1,048,576 bytes)
        // otherwise you'd pass /dev/zero and oom
        const MAX_CERT_SIZE: u64 = 1_048_576;
        let mut reader = BufReader::new(file).take(MAX_CERT_SIZE);
        let mut cert_pem = String::new();

        // Read up to 1MiB into cert_pem
        reader
            .read_to_string(&mut cert_pem)
            .await
            .context(format!("Failed to read certificate file: {}", path.display()))?;

        // Check if we hit the limit (more data remains)
        if reader.limit() == 0 && reader.get_ref().get_ref().metadata().await.is_ok() {
            anyhow::bail!("Certificate file too large (>1MiB): {}", path.display());
        }

        // Parse PEM
        let cert = native_tls::Certificate::from_pem(cert_pem.as_bytes())
            .context(format!("Failed to parse PEM certificate: {}", path.display()))?;

        eprintln!("Added trusted certificate from {} for a new TLS connection.", path.display());
        Ok(Some(cert))
    } else {
        eprintln!("No trusted certificate specified via --cert for this new connection, thus if you used local CA or self-signed server certificate, you may get an error like '(unable to get local issuer certificate)' next.");
        Ok(None)
    }
}

//pub fn cert() -> clap::Arg {
//    clap::Arg::new("cert")
//        .long("cert")
//        .value_name("FILE")
//        .action(clap::ArgAction::Set)
//        .value_parser(clap::value_parser!(std::path::PathBuf))
//        .required(false)
//        .help("Path to the server’s self-signed certificate or CA certificate (PEM format) to trust during this command (ie. as if it were part of your system's cert root store)")
//}

//for cli clients:
pub fn trust_server_cert() -> clap::Arg {
    //TODO: rename this to trust_ca_cert() it's less confusing
    clap::Arg::new("trust-server-cert")
        .long("trust-server-cert")
        .alias("trust-server-certs")
        .alias("trust-server-cert-bundle")
        .alias("cert")
        .alias("certs")
        .alias("cert-bundle")
        .alias("root-cert")
        .alias("root-certs")
        .alias("root-cert-bundle")
        .alias("trust-ca-cert")
        .alias("trust-ca-certs")
        .alias("trust-ca-cert-bundle")
        .alias("ca-certs")
        .alias("ca-cert")
        .alias("ca-cert-bundle")
        .value_name("FILE")
        .action(clap::ArgAction::Set)
        .value_parser(clap::value_parser!(std::path::PathBuf))
        .required(false)
//        .requires("ssl")
        //.help("Path to PEM file containing certificates to trust for the server (e.g., CA or self-signed)")
        .help("Path to the server’s self-signed certificate or CA certificate (PEM format, can be a bundle ie. appended PEM certs) to trust during this command (ie. as if it were part of your system's cert trust/root store)")
}

//for the cli clients:
pub fn client_cert() -> clap::Arg {
    clap::Arg::new("client-cert")
        .long("client-cert")
        .value_name("FILE")
        .action(clap::ArgAction::Set)
        .value_parser(clap::value_parser!(std::path::PathBuf))
        .required(false)
//        .requires("ssl")
        .help("Path to the client’s certificate (PEM format) for authentication")
}

//for the cli clients:
pub fn client_key() -> clap::Arg {
    clap::Arg::new("client-key")
        .long("client-key")
        .value_name("FILE")
        .action(clap::ArgAction::Set)
        .value_parser(clap::value_parser!(std::path::PathBuf))
        .required(false)
        .requires("client-cert")
//        .requires("ssl")
        .help("Path to the client’s private key (PEM format) for authentication")
}

//for cli clients, this is the default(to trust):
pub fn trust_system_root_store() -> clap::Arg {
    clap::Arg::new("trust-system-root-store")
        .long("trust-system-root-store")
//        .alias("trust-root-store")
        .action(clap::ArgAction::SetTrue)
        .conflicts_with("no-trust-system-root-store")
//        .requires("ssl")
        .help("Use system root certificates (default)")
}

//for cli clients, setting this means only the --trust-server-certs arg is used to verify the
//target server's cert):
pub fn no_trust_system_root_store() -> clap::Arg {
    clap::Arg::new("no-trust-system-root-store")
        .long("no-trust-system-root-store")
        .alias("empty-trust-store")
//        .alias("no-trust-root-store")
        .action(clap::ArgAction::SetTrue)
        .conflicts_with("trust-system-root-store")
        .requires("trust-server-cert")
//        .requires("ssl")
        .help("Use empty trust store (requires --trust-server-cert else there'd be 0 certs to verify trust)")
}

//for the standalone server:
pub fn client_trust_cert() -> clap::Arg {
    clap::Arg::new("client-trust-cert")
        .long("client-trust-cert")
        .alias("client-cert")
        .alias("client-certs")
        .alias("client-ca-cert")
        .alias("client-root-cert")
        .alias("client-trust-certs")
        .alias("client-ca-certs")
        .alias("client-root-certs")
        .alias("client-cert-bundle")
        .alias("client-trust-cert-bundle")
        .alias("client-ca-cert-bundle")
        .alias("client-root-cert-bundle")
        .value_name("FILE")
        .action(clap::ArgAction::Set)
        .value_parser(clap::value_parser!(std::path::PathBuf))
        .requires("ssl")
        .required(false)
        .help("Path to PEM file containing certificate(s) to trust for client authentication (e.g., CA or self-signed)")
}

//for the standalone server:
pub fn client_trust_system_root_store() -> clap::Arg {
    clap::Arg::new("client-trust-system-root-store")
        .long("client-trust-system-root-store")
        .action(clap::ArgAction::SetTrue)
        .conflicts_with("client-no-trust-system-root-store")
        .requires("ssl")
        .help("Use system root certificates for client authentication (unusual)")
}

//for the standalone server:
pub fn client_no_trust_system_root_store() -> clap::Arg {
    clap::Arg::new("client-no-trust-system-root-store")
        .long("client-no-trust-system-root-store")
        .alias("client-empty-trust-store")
        .action(clap::ArgAction::SetTrue)
        .conflicts_with("client-trust-system-root-store")
        .requires("client-trust-cert")
        .requires("ssl")
        .help("Use empty trust store for client authentication (default), requires --client-trust-cert to validate client certs somehow.")
}

///// Asynchronously reads a file with a maximum size limit of 1 MiB.
//pub async fn read_file_limited(path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
//    const MAX_SIZE: usize = 1_048_576; // 1 MiB
//
//    let file = tokio::fs::File::open(path)
//        .await
//        .context(format!("Failed to open file {}", path.display()))?;
//        //.map_err(|e| anyhow::anyhow!("Failed to open file {}: {}", path.display(), e))?;
//    let metadata = file
//        .metadata()
//        .await
//        .context(format!("Failed to read metadata for {}", path.display()))?;
//        //.map_err(|e| anyhow::anyhow!("Failed to read metadata for {}: {}", path.display(), e))?;
//
//    if metadata.len() > MAX_SIZE as u64 {
//        return Err(anyhow::anyhow!(
//            "File {} exceeds maximum size of {} bytes",
//            path.display(),
//            MAX_SIZE
//        ));
//    }
//
//    let mut reader = tokio::io::BufReader::new(file);
//    let mut data = Vec::with_capacity(metadata.len() as usize);
//    reader
//        .read_to_end(&mut data)
//        .await
//        .context(format!("Failed to read(happens after open) file {}", path.display()))?;
//        //.map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", path.display(), e))?;
//
//    Ok(data)
//}
/// Asynchronously reads a file with a maximum size limit of 1 MiB.
/// Files of 1 MiB or larger will fail; files under 1 MiB are allowed.
/// This should avoid unresponsive system(DOS-ing) until OOM kicks in  if you do /dev/zero as the path.
//#[cfg(not(target_arch = "wasm32"))]
pub async fn read_file_limited(path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
    // if file is >= to this, fails!
    const MAX_SIZE: u64 = 1_048_576; // 1 MiB

    // it's explicitly opened as read only:
    let file = tokio::fs::OpenOptions::new()
        .read(true)
        .write(false)
        .open(path)
    //let file = tokio::fs::File::open(path)//it's read-only by default!
        .await
        .context(format!("Failed to open file: {}", path.display()))?;

    // This to avoid the reading/mem alloc for normal eg. non-/dev/zero files:
    let metadata = file
        .metadata()
        .await
        .context(format!("Failed to read metadata for {}", path.display()))?;

    if metadata.len() >= MAX_SIZE as u64 {
        return Err(anyhow::anyhow!(
            "File {} exceeds maximum size of {} bytes",
            path.display(),
            MAX_SIZE-1
        ));
    }

    let mut reader = tokio::io::BufReader::new(file).take(MAX_SIZE);
    //allocs 1MiB for any file size, once.
    //XXX: this is overkill for normal certs which are like 2KiB, or CA cert chains 250KiB+-
    //if memory's a problem, just Vec::new() here instead.
    let mut data = Vec::with_capacity(MAX_SIZE as usize);

    reader
        .read_to_end(&mut data)
        .await
        .context(format!("Failed to read(happens after open) file: {}", path.display()))?;

    if reader.limit() == 0 {
        return Err(anyhow::anyhow!(
            "Read(already) >= {} bytes from file {}, exceeding maximum accepted size of {} bytes.",
            MAX_SIZE,
            path.display(),
            MAX_SIZE-1
        ));
    }

    Ok(data)
}


#[macro_export]
macro_rules! set_string {
    ($s:expr, $new:expr) => {
        $s.replace_range(.., $new);
    };
}

pub fn set_string(s: &mut String, new: &str) {
    s.replace_range(.., new);
}

#[macro_export]
macro_rules! new_string {
    ($binding:ident, $initial:literal, $capacity:expr) => {
        let mut $binding: String = {
            const LOCAL: &str = $initial; // Explicit &str
            let capacity: usize = const {
                // Compile-time check
                const INIT_LEN: usize = $initial.len();
                if $capacity >= INIT_LEN { $capacity } else { INIT_LEN }
            };
            let mut s: String = String::with_capacity(capacity);
            s.push_str(LOCAL);
            //FIXME Move: Returns a String (24 bytes: ptr, len, capacity), moved to $binding.
            //--release: LLVM inlines the block, constructing s directly in $binding’s stack slot (zero cost). The move is eliminated—s is built in-place.
            s
        };
    };
    ($binding:ident, $initial:expr, $capacity:expr) => {
        let mut $binding: String = {
            let local: &str = $initial; // Explicit &str
            let capacity: usize = {
                // Runtime check
                let init_len = local.len();
                if $capacity >= init_len { $capacity } else { init_len }
            };
            let mut s: String = String::with_capacity(capacity);
            s.push_str(local);
            //FIXME Move: Returns a String (24 bytes: ptr, len, capacity), moved to $binding.
            //--release: LLVM inlines the block, constructing s directly in $binding’s stack slot (zero cost). The move is eliminated—s is built in-place.
            s
        };
    };
}

use std::path::Path;
use std::error::Error;

#[derive(Debug)]
pub struct ClientCertError {
    path: String,
    source: anyhow::Error,
}

impl ClientCertError {
    pub fn new(path: &Path, source: anyhow::Error) -> Self {
        Self {
            path: path.display().to_string(),
            source,
        }
    }
}

impl std::fmt::Display for ClientCertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Something failed with client certificate {}", self.path) //: {}", self.path, self.source)
    }
}

impl Error for ClientCertError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.source)
    }
}

#[derive(Debug)]
pub struct ClientKeyError {
    path: String,
    source: anyhow::Error,
}

impl ClientKeyError {
    pub fn new(path: &Path, source: anyhow::Error) -> Self {
        Self {
            path: path.display().to_string(),
            source,
        }
    }
}

impl std::fmt::Display for ClientKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Something failed with client private key {}", self.path) //: {}", self.path, self.source)
    }
}

impl Error for ClientKeyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.source)
    }
}

#[derive(Debug)]
pub struct TrustCertError {
    path: String,
    source: anyhow::Error,
}

impl TrustCertError {
    pub fn new(path: &Path, source: anyhow::Error) -> Self {
        Self {
            path: path.display().to_string(),
            source,
        }
    }
}

impl std::fmt::Display for TrustCertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Something failed with trust certificate {}", self.path) //: {}", self.path, self.source)
    }
}

impl Error for TrustCertError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.source)
    }
}

/*doneFIXME: find out why I got the following error only once.
ok it's this https://github.com/seanmonstar/reqwest/issues/1808 and possibly https://github.com/hyperium/hyper/issues/2136  but basically it's because client doesn't expect server to reply because client didn't request(HTTP1) anything first in order to expect a reply, so if both  reply and close  are happening on server then some race happens where mostly closed connection is handled first, even tho the reply itself is already gotten.
 * https://github.com/seanmonstar/reqwest/issues/2649
 * https://github.com/hyperium/hyper-util/pull/184
 *
$ spacetime server ping slocal --cert ../my/spacetimedb-cert-gen/ca.crt
WARNING: This command is UNSTABLE and subject to breaking changes.

Adding trusted root cert(for server verification): subject=CN=MyLocalCA, issuer=CN=MyLocalCA, serial=359627719638463223090969970838819027680303337392, expires=Mar 24 13:24:50 2035 +00:00, fingerprint=25bb314ec76db8ab97f225011ec24dd0bca8aff470cb21c812600a8f4ed0cca7
Error: Failed sending request to https://127.0.0.1:3000: Failed to construct or send the HTTP request, source: Some(
    hyper_util::client::legacy::Error(
        Canceled,
        hyper::Error(
            Canceled,
            hyper::Error(
                Io,
                Custom {
                    kind: Other,
                    error: Error {
                        code: ErrorCode(
                            1,
                        ),
                        cause: Some(
                            Ssl(
                                ErrorStack(
                                    [
                                        Error {
                                            code: 167773276,
                                            library: "SSL routines",
                                            function: "ssl3_read_bytes",
                                            reason: "tlsv13 alert certificate required",
                                            file: "ssl/record/rec_layer_s3.c",
                                            line: 908,
                                            data: "SSL alert number 116",
                                        },
                                    ],
                                ),
                            ),
                        ),
                    },
                },
            ),
        ),
    ),
)


XXX: and why I get instead this:

$ spacetime server ping slocal --cert ../my/spacetimedb-cert-gen/ca.crt
WARNING: This command is UNSTABLE and subject to breaking changes.

Adding trusted root cert(for server verification): subject=CN=MyLocalCA, issuer=CN=MyLocalCA, serial=359627719638463223090969970838819027680303337392, expires=Mar 24 13:24:50 2035 +00:00, fingerprint=25bb314ec76db8ab97f225011ec24dd0bca8aff470cb21c812600a8f4ed0cca7
Error: Failed sending request to https://127.0.0.1:3000: Server closed the connection because you did NOT provide the args --client-cert and --client-key for mutual TLS (mTLS), source: Some(
    hyper_util::client::legacy::Error(
        SendRequest,
        hyper::Error(
            ChannelClosed,
        ),
    ),
)

*/
//#[cfg(not(target_arch = "wasm32"))]
pub fn map_request_error<E: Into<anyhow::Error>>(
    e: E,
    url: &String,
    client_cert_path: Option<&Path>,
    client_key_path: Option<&Path>,
) -> anyhow::Error {
    let e = e.into();
    //let mut last_message:String = "Unknown error occurred".to_string();
    new_string!(last_message, "An error occurred that wasn't mapped into something better by map_request_error.", 512);
//    fn example<E: std::fmt::Display>(e: &E) -> String {
//        format!("err: {}, Error type: {}", e, std::any::type_name::<E>())
//    }
//    set_string!(last_message, &format!("{}", example(&e)));
    let mut max_specificity = 0; // 0: Unknown, 1: reqwest, 2: ChannelClosed, 3: tlsv13 alert, 4: file

    /*Normally Similar: For most types, &e and e.as_ref() are equivalent, as AsRef often just returns a reference to the type. For example, for String, e.as_ref() returns &String, same as &e.
      Your Case: For anyhow::Error, e.as_ref() is special:

      &e gives &anyhow::Error, a reference to the struct.
      e.as_ref() calls anyhow::Error’s AsRef implementation, returning &dyn std::error::Error + Send + Sync + 'static. This dynamic trait object satisfies the bounds needed for downcast_ref and source, avoiding E0277.
      */
    // Summary: e.as_ref() in map_request_error converts e: anyhow::Error to &dyn std::error::Error, enabling safe chain traversal.
    // Traverse the error chain using e.as_ref()
    let mut current: Option<&dyn std::error::Error> = Some(e.as_ref());
    while let Some(err) = current {
        // Check hyper::Error
        if let Some(hyper_err) = err.downcast_ref::<hyper::Error>() {
            if hyper_err.is_closed() {
                let msg = (
                    if client_cert_path.is_none() || client_key_path.is_none() {
                        "Server closed the connection likely because you did NOT provide the args --client-cert and --client-key for mutual TLS (mTLS) and server requires it, also you need the hyper-util patch which affects hyper/reqwest and makes them not hide connection errors behind ChannelClosed from here: https://github.com/hyperium/hyper-util/pull/184 which means that's why you're seeing this generic error."
                    } else {
                        "Connection channel closed unexpectedly (server may be down or misconfigured), you should have this PR https://github.com/hyperium/hyper-util/pull/184 applied to avoid hiding the real reason behind ChannelClosed error(s)."
                    },
                    2,
                );
                if msg.1 > max_specificity {
                    //last_message = msg.0.to_string();
                    //last_message. = msg.0.to_string();
                    set_string!(last_message, msg.0);
                    max_specificity = msg.1;
                }
            }
            if let Some(io_err) = hyper_err.source() {
                if let Some(ssl_err) = io_err.downcast_ref::<std::io::Error>() {
                    if let Some(openssl_err) = ssl_err.get_ref() {
                        if let Some(ssl_error) = openssl_err.downcast_ref::<openssl::ssl::Error>() {
                            if ssl_error
                                .ssl_error()
                                    .map(|stack: &openssl::error::ErrorStack| {
                                        stack
                                            .errors()
                                            .iter()
                                            .any(|e| e.reason() == Some("tlsv13 alert certificate required"))
                                    })
                            .unwrap_or(false)
                            {
                                let msg = (
                                    if client_cert_path.is_none() || client_key_path.is_none() {
                                        "You didn't pass the required client certificate(yours) for mTLS, use --client-cert and --client-key 🔒"
                                    } else {
                                        "TLS handshake failed: server requires a valid client certificate(yours) for mTLS 🔒"
                                    },
                                    3,
                                );
                                if msg.1 > max_specificity {
                                    //last_message = msg.0;
                                    set_string!(last_message, msg.0);
                                    max_specificity = msg.1;
                                }
                            }
                        }
                    }
                }
            }
        }
        // Check reqwest::Error
        else if let Some(reqwest_err) = err.downcast_ref::<reqwest::Error>() {
            let msg = if reqwest_err.is_connect() {
                Some(("Failed to connect to the server (connection refused or network unreachable)", 1))
            } else if reqwest_err.is_timeout() {
                Some(("Request timed out while trying to reach the server", 1))
            } else if reqwest_err.is_request() {
                Some(("Failed to construct or send the HTTP request", 1))
            } else if reqwest_err.is_body() {
                Some(("Error in the request body", 1))
            } else if reqwest_err.is_decode() {
                Some(("Failed to decode the response", 1))
            } else {
                None
            };
            if let Some((msg, spec)) = msg {
                if spec > max_specificity {
                    //last_message = msg;
                    set_string!(last_message, msg);
                    max_specificity = spec;
                }
            }
        }
        // Check custom file errors
        else if let Some(trust_err) = err.downcast_ref::<TrustCertError>() {
            let msg = (format!("problem with trust certificate file {}", trust_err.path), 4);
            if msg.1 > max_specificity {
                set_string!(last_message, &msg.0);
                max_specificity = msg.1;
            }
        }
        else if let Some(cert_err) = err.downcast_ref::<ClientCertError>() {
            let msg = (format!("problem with client certificate file {}", cert_err.path), 4);
            if msg.1 > max_specificity {
                set_string!(last_message, &msg.0);
                max_specificity = msg.1;
            }
        }
        else if let Some(key_err) = err.downcast_ref::<ClientKeyError>() {
            let msg = (format!("problem with client private key file {}", key_err.path), 4);
            if msg.1 > max_specificity {
                set_string!(last_message, &msg.0);
                max_specificity = msg.1;
            }
        }

        current = err.source();
    }
    //TODO: see if specificity is needed, and likely get rid of it

    let source_str = match e.source() {
        Some(err) => format!("{:#?}", err),
        None => "<no error cause/source>".to_string(),
    };
    let message=format!(
        "(as follows on next lines)\n------- map_request_error ------\nFailed sending request to {}\nerr   : {}\nsource: {}\n----- end -----",
        url,
        last_message,
        source_str,
    );
    // Chain the original error with the new message
    e.context(message)
}

#[macro_export]
macro_rules! map_request_error {
    ($result:expr, $url:expr, $client_cert_path:expr, $client_key_path:expr) => {
        //using self:: here requires only an use spacetimedb_lib::map_request_error; at call site.
        //and note how macro and fn name are same.
        $result.map_err(|e| self::map_request_error(e,
                &$url,
                $client_cert_path.as_deref(),
                $client_key_path.as_deref()
                ))
    };
}


} // end of non_wasm! macro call
