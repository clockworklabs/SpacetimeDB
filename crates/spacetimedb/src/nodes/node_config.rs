use crate::nodes::node_options::NodeOptions;

#[derive(Debug, Clone)]
pub struct ControlNodeConfig {
    pub peer_api_listen_addr: String,
    pub peer_api_advertise_addr: String,
    pub peer_api_bootstrap_addrs: Vec<String>,
    pub worker_api_listen_addr: String,
    pub worker_api_advertise_addr: String,
    pub client_api_listen_addr: String,
    pub client_api_advertise_addr: String,
}

#[derive(Debug, Clone)]
pub struct WorkerNodeConfig {
    // Address to contact a bootstrap control node
    pub worker_api_bootstrap_addrs: Vec<String>,
    pub client_api_bootstrap_addrs: Vec<String>,
    pub listen_addr: String,
}

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub worker_node: Option<WorkerNodeConfig>,
    pub control_node: Option<ControlNodeConfig>,
}

impl NodeConfig {
    pub fn from_options(mut options: NodeOptions) -> Self {
        if options.control_node && options.worker_node && options.worker_api_bootstrap_addrs.len() == 0 {
            options.worker_api_bootstrap_addrs = vec!["localhost".to_owned()];
        }

        if options.control_node && options.worker_node && options.client_api_bootstrap_addrs.len() == 0 {
            options.client_api_bootstrap_addrs = vec!["localhost".to_owned()];
        }

        options.normalize();

        let control_node = if options.control_node {
            Some(ControlNodeConfig {
                peer_api_listen_addr: options.peer_api_listen_addr.unwrap(),
                peer_api_advertise_addr: options.peer_api_advertise_addr.unwrap(),
                peer_api_bootstrap_addrs: options.peer_api_bootstrap_addrs,
                worker_api_listen_addr: options.worker_api_listen_addr.unwrap(),
                worker_api_advertise_addr: options.worker_api_advertise_addr.unwrap(),
                client_api_listen_addr: options.control_api_listen_addr.unwrap(),
                client_api_advertise_addr: options.control_api_advertise_addr.unwrap(),
            })
        } else {
            None
        };

        let worker_node = if options.worker_node {
            Some(WorkerNodeConfig {
                worker_api_bootstrap_addrs: options.worker_api_bootstrap_addrs,
                client_api_bootstrap_addrs: options.client_api_bootstrap_addrs,
                listen_addr: options.listen_addr.unwrap(),
            })
        } else {
            None
        };

        Self {
            worker_node,
            control_node,
        }
    }
}
