use std::net::ToSocketAddrs;

#[derive(Debug, Clone)]
pub struct NodeOptions {
    pub control_node: bool,
    pub worker_node: bool,

    // Worker flags
    pub listen_addr: Option<String>,
    pub advertise_addr: Option<String>,
    pub worker_api_bootstrap_addrs: Vec<String>,
    pub client_api_bootstrap_addrs: Vec<String>,

    // Control flags
    pub peer_api_listen_addr: Option<String>,
    pub peer_api_advertise_addr: Option<String>,
    pub peer_api_bootstrap_addrs: Vec<String>,

    pub worker_api_listen_addr: Option<String>,
    pub worker_api_advertise_addr: Option<String>,

    pub control_api_listen_addr: Option<String>,
    pub control_api_advertise_addr: Option<String>,
}

impl NodeOptions {
    const WORKER_CLIENT_API_DEFAULT_PORT: u16 = 80;

    const PEER_API_DEFAULT_PORT: u16 = 26259;
    const WORKER_API_DEFAULT_PORT: u16 = 26260;
    const CLIENT_API_DEFAULT_PORT: u16 = 26258;

    pub fn normalize(&mut self) {
        // So workers can connect to the Control node worker API
        let mut worker_api_bootstrap_addrs_defaults = Vec::new();
        if self.worker_api_bootstrap_addrs.is_empty() {
            for addr in &self.worker_api_bootstrap_addrs {
                if addr.contains(':') {
                    worker_api_bootstrap_addrs_defaults.push(addr.clone());
                } else {
                    worker_api_bootstrap_addrs_defaults.push(format!("{}:{}", addr, Self::WORKER_API_DEFAULT_PORT))
                }
            }
        }
        self.worker_api_bootstrap_addrs = worker_api_bootstrap_addrs_defaults;

        // So workers can connect to the Control node client API
        let mut client_api_bootstrap_addrs_defaults = Vec::new();
        if self.client_api_bootstrap_addrs.is_empty() {
            for addr in &self.client_api_bootstrap_addrs {
                if addr.contains(':') {
                    client_api_bootstrap_addrs_defaults.push(addr.clone());
                } else {
                    client_api_bootstrap_addrs_defaults.push(format!("{}:{}", addr, Self::CLIENT_API_DEFAULT_PORT))
                }
            }
        }
        self.client_api_bootstrap_addrs = client_api_bootstrap_addrs_defaults;

        if self.advertise_addr.is_none() {
            if self.listen_addr.is_none() {
                let hostname = hostname::get().unwrap().to_str().unwrap().to_owned();
                let addr = format!("{}:{}", hostname, Self::WORKER_CLIENT_API_DEFAULT_PORT);
                let _ = addr.to_socket_addrs().expect("resolve hostname");
                self.advertise_addr = Some(addr);
            } else {
                self.advertise_addr = self.listen_addr.clone();
            }
        }

        if self.listen_addr.is_none() {
            self.listen_addr = Some(format!("0.0.0.0:{}", Self::WORKER_CLIENT_API_DEFAULT_PORT));
        }

        // Peer API
        if self.peer_api_advertise_addr.is_none() {
            if self.peer_api_listen_addr.is_none() {
                let hostname = hostname::get().unwrap().to_str().unwrap().to_owned();
                let addr = format!("{}:{}", hostname, Self::PEER_API_DEFAULT_PORT);
                let _ = addr.to_socket_addrs().expect("resolve hostname");
                self.peer_api_advertise_addr = Some(addr);
            } else {
                self.peer_api_advertise_addr = self.peer_api_listen_addr.clone();
            }
        }

        if self.peer_api_listen_addr.is_none() {
            self.peer_api_listen_addr = Some(format!("0.0.0.0:{}", Self::PEER_API_DEFAULT_PORT));
        }

        let mut peer_api_bootstrap_addrs_defaults = Vec::new();
        if self.peer_api_bootstrap_addrs.is_empty() {
            for addr in &self.peer_api_bootstrap_addrs {
                if addr.contains(':') {
                    peer_api_bootstrap_addrs_defaults.push(addr.clone());
                } else {
                    peer_api_bootstrap_addrs_defaults.push(format!("{}:{}", addr, Self::PEER_API_DEFAULT_PORT))
                }
            }
        }
        self.peer_api_bootstrap_addrs = peer_api_bootstrap_addrs_defaults;

        // Worker API
        if self.worker_api_advertise_addr.is_none() {
            if self.worker_api_listen_addr.is_none() {
                let hostname = hostname::get().unwrap().to_str().unwrap().to_owned();
                let addr = format!("{}:{}", hostname, Self::WORKER_API_DEFAULT_PORT);
                let _ = addr.to_socket_addrs().expect("resolve hostname");
                self.worker_api_advertise_addr = Some(addr);
            } else {
                self.worker_api_advertise_addr = self.worker_api_listen_addr.clone();
            }
        }

        if self.worker_api_listen_addr.is_none() {
            self.worker_api_listen_addr = Some(format!("0.0.0.0:{}", Self::WORKER_API_DEFAULT_PORT));
        }

        // Client API
        if self.control_api_advertise_addr.is_none() {
            if self.control_api_listen_addr.is_none() {
                let hostname = hostname::get().unwrap().to_str().unwrap().to_owned();
                let addr = format!("{}:{}", hostname, Self::CLIENT_API_DEFAULT_PORT);
                let _ = addr.to_socket_addrs().expect("resolve hostname");
                self.control_api_advertise_addr = Some(addr);
            } else {
                self.control_api_advertise_addr = self.control_api_listen_addr.clone();
            }
        }

        if self.control_api_listen_addr.is_none() {
            self.control_api_listen_addr = Some(format!("0.0.0.0:{}", Self::CLIENT_API_DEFAULT_PORT));
        }
    }
}
