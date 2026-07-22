use clap::ArgAction::SetTrue;
use clap::{value_parser, Arg, ValueEnum};

#[derive(Copy, Clone, Debug, ValueEnum, PartialEq)]
pub enum ClearMode {
    Always,     // parses as "always"
    OnConflict, // parses as "on-conflict"
    Never,      // parses as "never"
}

pub fn server() -> Arg {
    Arg::new("server")
        .long("server")
        .short('s')
        .help("The nickname, host name or URL of the server")
}

pub fn anonymous() -> Arg {
    Arg::new("anon_identity")
        .long("anonymous")
        .action(SetTrue)
        .help("Perform this action with an anonymous identity")
}

pub fn yes() -> Arg {
    Arg::new("force")
        .long("yes")
        .short('y')
        .action(SetTrue)
        .help("Run non-interactively wherever possible. This will answer \"yes\" to almost all prompts, but will sometimes answer \"no\" to preserve non-interactivity (e.g. when prompting whether to log in with spacetimedb.com).")
}

pub fn confirmed() -> Arg {
    Arg::new("confirmed")
        .required(false)
        .long("confirmed")
        .num_args(1)
        .value_parser(value_parser!(bool))
        .help("Instruct the server to deliver only updates of confirmed transactions")
}

pub fn clear_database() -> Arg {
    Arg::new("clear-database")
        .long("delete-data")
        .alias("clear-database")
        .short('c')
        .num_args(0..=1)
        .value_parser(value_parser!(ClearMode))
        // Because we have a default value for this flag, invocations can be ambiguous between
        //passing a value to this flag, vs using the default value and passing an anonymous arg
        // to the rest of the command. Adding `require_equals` resolves this ambiguity.
        .require_equals(true)
        .default_missing_value("always")
        .help(
            "When publishing to an existing database identity, first DESTROY all data associated with the module. With 'on-conflict': only when breaking schema changes occur."
        )
}

pub fn dotnet_version() -> Arg {
    Arg::new("dotnet_version")
        .long("dotnet-version")
        .value_name("VERSION")
        .value_parser(parse_dotnet_version)
        .help("Target .NET SDK major version for C# projects (e.g. 8 or 10). Auto-detected when omitted.")
}

pub fn parse_dotnet_version(version: &str) -> anyhow::Result<u8> {
    match version.parse::<u8>() {
        Ok(version @ (8 | 10)) => Ok(version),
        Ok(version) => anyhow::bail!("Unsupported --dotnet-version {version}. Supported values: 8, 10."),
        Err(error) => anyhow::bail!("Invalid --dotnet-version: {error}"),
    }
}

pub fn parse_optional_dotnet_version(dotnet_version: Option<&str>) -> anyhow::Result<Option<u8>> {
    dotnet_version.map(parse_dotnet_version).transpose()
}

#[cfg(test)]
mod tests {
    use super::parse_optional_dotnet_version;

    #[test]
    fn dotnet_version_accepts_supported_sdk_majors() {
        assert_eq!(parse_optional_dotnet_version(None).unwrap(), None);
        assert_eq!(parse_optional_dotnet_version(Some("8")).unwrap(), Some(8));
        assert_eq!(parse_optional_dotnet_version(Some("10")).unwrap(), Some(10));
    }

    #[test]
    fn dotnet_version_rejects_unsupported_sdk_majors() {
        assert!(parse_optional_dotnet_version(Some("9")).is_err());
        assert!(parse_optional_dotnet_version(Some("not-a-number")).is_err());
    }
}
