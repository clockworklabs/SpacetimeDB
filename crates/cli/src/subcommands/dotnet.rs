pub(crate) fn parse_dotnet_version(version: &str) -> anyhow::Result<u8> {
    match version.parse::<u8>() {
        Ok(version @ (8 | 10)) => Ok(version),
        Ok(version) => anyhow::bail!("Unsupported --dotnet-version {version}. Supported values: 8, 10."),
        Err(error) => anyhow::bail!("Invalid --dotnet-version: {error}"),
    }
}

pub(crate) fn parse_optional_dotnet_version(dotnet_version: Option<&str>) -> anyhow::Result<Option<u8>> {
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
