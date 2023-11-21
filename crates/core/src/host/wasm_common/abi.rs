pub use spacetimedb_lib::VersionTuple;

const MODULE_PREFIX: &str = "spacetime_";

pub fn determine_spacetime_abi<I>(
    imports: impl IntoIterator<Item = I>,
    get_module: impl Fn(&I) -> &str,
) -> Result<VersionTuple, AbiVersionError> {
    let it = imports.into_iter().filter_map(|imp| {
        let s = get_module(&imp);
        let err = || AbiVersionError::Parse { module: s.to_owned() };
        s.strip_prefix(MODULE_PREFIX).map(|ver| {
            let (major, minor) = ver.split_once('.').ok_or_else(err)?;
            let (major, minor) = Option::zip(major.parse().ok(), minor.parse().ok()).ok_or_else(err)?;
            Ok(VersionTuple { major, minor })
        })
    });
    itertools::process_results(it, |mut it| {
        let first = it.next().ok_or(AbiVersionError::NotDetected)?;
        it.try_fold(first, refine_ver_req)
    })?
}

fn refine_ver_req(ver: VersionTuple, new: VersionTuple) -> Result<VersionTuple, AbiVersionError> {
    if ver.major != new.major {
        Err(AbiVersionError::MultipleMajor(ver.major, new.major))
    } else {
        Ok(Ord::max(ver, new))
    }
}

pub fn verify_supported(implements: VersionTuple, got: VersionTuple) -> Result<(), AbiVersionError> {
    if implements.supports(got) {
        Ok(())
    } else {
        Err(AbiVersionError::UnsupportedVersion { implements, got })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum AbiVersionError {
    #[error("import module {module:?} has malformed version string")]
    Parse { module: String },
    #[error("module cannot depend on both major version {0} and major version {1}")]
    MultipleMajor(u16, u16),
    #[error("abi version {got} is not supported (host implements {implements})")]
    UnsupportedVersion {
        got: VersionTuple,
        implements: VersionTuple,
    },
    // TODO: (by 1.0, maybe) remove the parenthetical from this error message
    #[error("unable to determine ABI of module (may be on an spacetime version < 0.8)")]
    NotDetected,
}
