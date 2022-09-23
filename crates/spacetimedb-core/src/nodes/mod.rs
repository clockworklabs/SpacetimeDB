use anyhow::anyhow;
use int_enum::IntEnum;

pub mod control_node;
pub mod node_config;
pub mod node_options;
pub mod worker_node;

// Module host type supported by a given database.
// Maps 1:1 with HostType in control_db.proto
#[repr(i32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntEnum)]
pub enum HostType {
    // Int values here *must* match their equivalent in control_db.proto
    WASM32 = 0,
    CPYTHON = 1,
}

impl HostType {
    pub fn parse(host_type: Option<String>) -> Result<HostType, anyhow::Error> {
        host_type.map_or(Ok(HostType::WASM32), |host_type_str| match host_type_str.as_str() {
            "wasm32" => Ok(HostType::WASM32),
            "python" => Ok(HostType::CPYTHON),
            _ => Err(anyhow!("unknown host_type {}", host_type_str)),
        })
    }

    pub fn as_param_str(&self) -> String {
        match self {
            HostType::WASM32 => String::from("wasm32"),
            HostType::CPYTHON => String::from("python"),
        }
    }
}
