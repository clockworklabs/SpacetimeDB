//! Browser-safe resource metadata and accepted cumulative usage, not live gauges.
use super::decimal_u64;
use crate::container::OciDigest;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerConfiguration {
    pub image_digest: OciDigest,
    pub resources: ResourceLimits,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceLimits {
    #[serde(with = "decimal_u64")]
    pub cpu_millicores: u64,
    #[serde(with = "decimal_u64")]
    pub memory_bytes: u64,
    #[serde(with = "decimal_u64")]
    pub scratch_bytes: u64,
    #[serde(with = "decimal_u64")]
    pub pids_max: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportedUsage {
    #[serde(with = "decimal_u64")]
    pub sample_sequence: u64,
    pub cumulative: UsageTotals,
    /// Accounting has accepted its final totals. This flag does not establish
    /// the container's observed state or physical storage reclamation.
    pub final_report: bool,
    /// A host reboot interrupted measurement in this or an earlier segment.
    /// Cumulative totals include known usage only; the gap is not zero usage.
    pub measurement_interrupted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageTotals {
    #[serde(with = "decimal_u128")]
    pub cpu_nanoseconds: u128,
    #[serde(with = "decimal_u128")]
    pub memory_byte_seconds: u128,
    #[serde(with = "decimal_u128")]
    pub scratch_byte_seconds: u128,
    #[serde(with = "decimal_u128")]
    pub transmitted_bytes: u128,
    #[serde(with = "decimal_u128")]
    pub received_bytes: u128,
}

mod decimal_u128 {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(number: &u128, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(number)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u128, D::Error> {
        let text = String::deserialize(deserializer)?;
        let value: u128 = text.parse().map_err(serde::de::Error::custom)?;
        if value.to_string() != text {
            return Err(serde::de::Error::custom("expected canonical decimal u128"));
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cumulative_usage_preserves_large_counters_and_measurement_gaps() {
        let usage = ReportedUsage {
            sample_sequence: u64::MAX,
            cumulative: UsageTotals {
                cpu_nanoseconds: u128::MAX,
                memory_byte_seconds: u128::MAX - 1,
                scratch_byte_seconds: 0,
                transmitted_bytes: 9_007_199_254_740_993,
                received_bytes: 7,
            },
            final_report: true,
            measurement_interrupted: true,
        };
        let encoded = serde_json::to_value(&usage).unwrap();
        assert_eq!(encoded["cumulative"]["cpu_nanoseconds"], u128::MAX.to_string());
        assert_eq!(encoded["cumulative"]["transmitted_bytes"], "9007199254740993");
        assert_eq!(serde_json::from_value::<ReportedUsage>(encoded.clone()).unwrap(), usage);
        for value in [
            serde_json::json!(1),
            serde_json::json!("01"),
            serde_json::json!("-1"),
            serde_json::json!("340282366920938463463374607431768211456"),
        ] {
            let mut invalid = encoded.clone();
            invalid["cumulative"]["cpu_nanoseconds"] = value;
            assert!(serde_json::from_value::<ReportedUsage>(invalid).is_err());
        }
        let mut invalid = encoded;
        invalid["credential"] = serde_json::json!("must not appear");
        assert!(serde_json::from_value::<ReportedUsage>(invalid).is_err());
    }

    #[test]
    fn configuration_uses_the_oci_digest_and_exact_limits() {
        let configuration = ContainerConfiguration {
            image_digest: format!("sha256:{}", "ab".repeat(32)).parse().unwrap(),
            resources: ResourceLimits {
                cpu_millicores: 500,
                memory_bytes: u64::MAX,
                scratch_bytes: 64 * 1024 * 1024,
                pids_max: 64,
            },
        };
        let encoded = serde_json::to_value(&configuration).unwrap();
        assert_eq!(encoded["image_digest"], format!("sha256:{}", "ab".repeat(32)));
        assert_eq!(encoded["resources"]["memory_bytes"], u64::MAX.to_string());
        assert_eq!(
            serde_json::from_value::<ContainerConfiguration>(encoded).unwrap(),
            configuration
        );
    }
}
