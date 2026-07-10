pub(crate) fn parse_dotnet_version(dotnet_version: Option<&str>) -> anyhow::Result<Option<u8>> {
    dotnet_version
        .map(|version| match version.parse::<u8>() {
            Ok(version @ (8 | 10)) => Ok(version),
            Ok(version) => anyhow::bail!("Unsupported --dotnet-version {version}. Supported values: 8, 10."),
            Err(error) => anyhow::bail!("Invalid --dotnet-version: {error}"),
        })
        .transpose()
}

pub(crate) fn build_options_with_dotnet_version(
    build_options: &str,
    dotnet_version: Option<&str>,
) -> anyhow::Result<String> {
    let Some(version) = parse_dotnet_version(dotnet_version)? else {
        return Ok(build_options.to_string());
    };

    Ok(if build_options.is_empty() {
        format!("--dotnet-version {version}")
    } else {
        format!("{build_options} --dotnet-version {version}")
    })
}

#[cfg(test)]
mod tests {
    use super::{build_options_with_dotnet_version, parse_dotnet_version};

    #[test]
    fn dotnet_version_accepts_supported_sdk_majors() {
        assert_eq!(parse_dotnet_version(None).unwrap(), None);
        assert_eq!(parse_dotnet_version(Some("8")).unwrap(), Some(8));
        assert_eq!(parse_dotnet_version(Some("10")).unwrap(), Some(10));
    }

    #[test]
    fn dotnet_version_rejects_unsupported_sdk_majors() {
        assert!(parse_dotnet_version(Some("9")).is_err());
        assert!(parse_dotnet_version(Some("not-a-number")).is_err());
    }

    #[test]
    fn dotnet_version_is_added_to_build_options_after_validation() {
        assert_eq!(
            build_options_with_dotnet_version("", Some("8")).unwrap(),
            "--dotnet-version 8"
        );
        assert_eq!(
            build_options_with_dotnet_version("--debug", Some("10")).unwrap(),
            "--debug --dotnet-version 10"
        );
        assert!(build_options_with_dotnet_version("", Some("9")).is_err());
    }
}
