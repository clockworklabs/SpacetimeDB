use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeJson {
    pub id: u64,
    pub unschedulable: bool,
    pub advertise_addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub state: String,
    // TODO: node memory, CPU, and storage capacity
    // TODO: node memory, CPU, and storage allocatable capacity
    // SEE: https://kubernetes.io/docs/reference/kubernetes-api/cluster-resources/node-v1/#NodeStatus
}
