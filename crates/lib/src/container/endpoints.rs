//! Public address discovery. These addresses carry no administrative or runtime
//! authority and do not promise that an application is currently ready.

use super::PortProtocol;
use crate::Identity;
use serde::{Deserialize, Serialize};

pub const ENDPOINTS_PENDING: &str = "endpoints_pending";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerEndpoints {
    pub database_identity: Identity,
    pub endpoints: Vec<ContainerEndpoint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerEndpoint {
    pub name: String,
    pub protocol: PortProtocol,
    pub url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_wire_contains_only_explicit_public_addresses() {
        let response = ContainerEndpoints {
            database_identity: Identity::ZERO,
            endpoints: vec![ContainerEndpoint {
                name: "http".into(),
                protocol: PortProtocol::Http,
                url: "https://aaaqeayeaudaocajbifqydiob4.container.example.net/".into(),
            }],
        };
        let encoded = serde_json::to_value(&response).unwrap();
        assert_eq!(encoded.as_object().unwrap().len(), 2);
        assert_eq!(
            encoded["endpoints"][0],
            serde_json::json!({
                "name": "http",
                "protocol": "http",
                "url": "https://aaaqeayeaudaocajbifqydiob4.container.example.net/"
            })
        );
        assert_eq!(
            serde_json::from_value::<ContainerEndpoints>(encoded.clone()).unwrap(),
            response
        );
        let mut changed = encoded;
        changed["endpoints"][0]["upstream"] = serde_json::json!("10.0.0.1:8080");
        assert!(serde_json::from_value::<ContainerEndpoints>(changed).is_err());
    }
}
