use spacetimedb::identity::Identity;
use spacetimedb::messages::control_db::{Database, EnergyBalance, Node, Replica};
use spacetimedb::{energy, stdb_path};

use spacetimedb_client_api_messages::name::{
    DomainName, DomainParsingError, InsertDomainResult, RegisterTldResult, Tld, TldRef,
};
use spacetimedb_lib::bsatn;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct ControlDb {
    db: sled::Db,
}

pub type Result<T> = core::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("collection not found: {0}")]
    CollectionNotFound(sled::Error),
    #[error("database error: {0}")]
    Database(sled::Error),
    #[error("record with the name {0} already exists")]
    RecordAlreadyExists(DomainName),
    #[error("database with identity {0} already exists")]
    DatabaseAlreadyExists(Identity),
    #[error("failed to register {0} domain")]
    DomainRegistrationFailure(DomainName),
    #[error("failed to decode data")]
    Decoding(#[from] bsatn::DecodeError),
    #[error(transparent)]
    DomainParsing(#[from] DomainParsingError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<sled::Error> for Error {
    fn from(err: sled::Error) -> Self {
        match err {
            sled::Error::CollectionNotFound(_) => Error::CollectionNotFound(err),
            err => Error::Database(err),
        }
    }
}

impl ControlDb {
    pub fn new() -> Result<Self> {
        let config = sled::Config::default()
            .path(stdb_path("control_node/control_db"))
            .flush_every_ms(Some(50))
            .mode(sled::Mode::HighThroughput);
        let db = config.open()?;
        Ok(Self { db })
    }

    #[cfg(test)]
    pub fn at(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let config = sled::Config::default()
            .path(path.as_ref())
            .flush_every_ms(Some(50))
            .mode(sled::Mode::HighThroughput);
        let db = config.open()?;
        Ok(Self { db })
    }
}

impl ControlDb {
    pub fn spacetime_dns(&self, domain: &DomainName) -> Result<Option<Identity>> {
        let tree = self.db.open_tree("dns")?;
        let value = tree.get(domain.to_lowercase().as_bytes())?;
        if let Some(value) = value {
            return Ok(Some(Identity::from_slice(&value[..])));
        }
        Ok(None)
    }

    pub fn spacetime_reverse_dns(&self, database_identity: &Identity) -> Result<Vec<DomainName>> {
        let tree = self.db.open_tree("reverse_dns")?;
        let value = tree.get(database_identity.to_byte_array())?;
        if let Some(value) = value {
            let vec: Vec<DomainName> = serde_json::from_slice(&value[..])?;
            return Ok(vec);
        }
        Ok(vec![])
    }

    /// Creates a new domain which points to the database identity. For example:
    ///  * `my_domain/my_database`
    ///  * `my_company/my_team/my_product`
    ///
    /// A TLD itself is also a fully qualified database name:
    ///  * `clockworklabs`
    ///  * `bitcraft`
    ///  * `...`
    ///
    /// # Arguments
    ///  * `database_identity` - The identity the database name should point to
    ///  * `database_name` - The database name to register
    ///  * `owner_identity` - The identity that is publishing the database name
    pub fn spacetime_insert_domain(
        &self,
        database_identity: &Identity,
        domain: DomainName,
        owner_identity: Identity,
        try_register_tld: bool,
    ) -> Result<InsertDomainResult> {
        let database_identity = *database_identity;
        if self.spacetime_dns(&domain)?.is_some() {
            return Err(Error::RecordAlreadyExists(domain));
        }
        let tld = domain.tld();
        match self.spacetime_lookup_tld(tld)? {
            Some(owner) => {
                if owner != owner_identity {
                    return Ok(InsertDomainResult::PermissionDenied { domain });
                }
            }
            None => {
                if try_register_tld {
                    // Let's try to automatically register this TLD for the identity
                    let result = self.spacetime_register_tld(tld.to_owned(), owner_identity)?;
                    if let RegisterTldResult::Success { .. } = result {
                        // This identity now owns this TLD
                    } else {
                        // This is technically possibly due to race conditions
                        return Err(Error::DomainRegistrationFailure(domain));
                    }
                } else {
                    return Ok(InsertDomainResult::TldNotRegistered { domain });
                }
            }
        }

        let identity_bytes = database_identity.to_byte_array();
        let tree = self.db.open_tree("dns")?;
        tree.insert(domain.to_lowercase().as_bytes(), &identity_bytes)?;

        let tree = self.db.open_tree("reverse_dns")?;
        match tree.get(identity_bytes)? {
            Some(value) => {
                let mut vec: Vec<DomainName> = serde_json::from_slice(&value[..])?;
                vec.push(domain.clone());
                tree.insert(identity_bytes, serde_json::to_string(&vec)?.as_bytes())?;
            }
            None => {
                tree.insert(identity_bytes, serde_json::to_string(&vec![&domain])?.as_bytes())?;
            }
        }

        Ok(InsertDomainResult::Success {
            domain,
            database_identity,
        })
    }

    /// Inserts a top level domain that will be owned by `owner_identity`.
    ///
    /// # Arguments
    ///
    /// * `domain` - The domain name to register
    /// * `owner_identity` - The identity that should own this domain name.
    pub fn spacetime_register_tld(&self, tld: Tld, owner_identity: Identity) -> Result<RegisterTldResult> {
        let tree = self.db.open_tree("top_level_domains")?;
        let key = tld.to_lowercase();
        let current_owner = tree.get(&key)?;
        match current_owner {
            Some(owner) => {
                if Identity::from_slice(&owner[..]) == owner_identity {
                    Ok(RegisterTldResult::AlreadyRegistered { domain: tld })
                } else {
                    Ok(RegisterTldResult::Unauthorized { domain: tld })
                }
            }
            None => {
                tree.insert(key, &owner_identity.to_byte_array())?;
                Ok(RegisterTldResult::Success { domain: tld })
            }
        }
    }

    /// Returns the owner (or `None` if there is no owner) of the domain.
    ///
    /// # Arguments
    ///  * `domain` - The domain to lookup
    pub fn spacetime_lookup_tld(&self, domain: impl AsRef<TldRef>) -> Result<Option<Identity>> {
        let tree = self.db.open_tree("top_level_domains")?;
        match tree.get(domain.as_ref().to_lowercase().as_bytes())? {
            Some(owner) => Ok(Some(Identity::from_slice(&owner[..]))),
            None => Ok(None),
        }
    }

    pub fn get_databases(&self) -> Result<Vec<Database>> {
        let tree = self.db.open_tree("database")?;
        let mut databases = Vec::new();
        let scan_key: &[u8] = b"";
        for result in tree.range(scan_key..) {
            let (_key, value) = result?;
            let database = compat::Database::from_slice(&value).unwrap().into();
            databases.push(database);
        }
        Ok(databases)
    }

    pub fn get_database_by_id(&self, id: u64) -> Result<Option<Database>> {
        for database in self.get_databases()? {
            if database.id == id {
                return Ok(Some(database));
            }
        }
        Ok(None)
    }

    pub fn get_database_by_identity(&self, identity: &Identity) -> Result<Option<Database>> {
        let tree = self.db.open_tree("database_by_identity")?;
        let key = identity.to_hex();
        let value = tree.get(key.as_bytes())?;
        if let Some(value) = value {
            let database = compat::Database::from_slice(&value[..]).unwrap().into();
            return Ok(Some(database));
        }
        Ok(None)
    }

    pub fn insert_database(&self, mut database: Database) -> Result<u64> {
        let id = self.db.generate_id()?;
        let tree = self.db.open_tree("database_by_identity")?;

        let key = database.database_identity.to_hex();
        if tree.contains_key(key)? {
            return Err(Error::DatabaseAlreadyExists(database.database_identity));
        }

        database.id = id;

        let buf = sled::IVec::from(compat::Database::from(database).to_vec().unwrap());

        tree.insert(key, buf.clone())?;

        let tree = self.db.open_tree("database")?;
        tree.insert(id.to_be_bytes(), buf)?;

        Ok(id)
    }

    pub fn delete_database(&self, id: u64) -> Result<Option<u64>> {
        let tree = self.db.open_tree("database")?;
        let tree_by_identity = self.db.open_tree("database_by_identity")?;

        if let Some(old_value) = tree.get(id.to_be_bytes())? {
            let database = compat::Database::from_slice(&old_value[..])?;
            let key = database.database_identity().to_hex();

            tree_by_identity.remove(key.as_bytes())?;
            tree.remove(id.to_be_bytes())?;
            return Ok(Some(id));
        }

        Ok(None)
    }

    pub fn get_replicas(&self) -> Result<Vec<Replica>> {
        let tree = self.db.open_tree("replica")?;
        let mut replicas = Vec::new();
        let scan_key: &[u8] = b"";
        for result in tree.range(scan_key..) {
            let (_key, value) = result?;
            let replica = bsatn::from_slice(&value[..]).unwrap();
            replicas.push(replica);
        }
        Ok(replicas)
    }

    pub fn get_replica_by_id(&self, replica_id: u64) -> Result<Option<Replica>> {
        for di in self.get_replicas()? {
            if di.id == replica_id {
                return Ok(Some(di));
            }
        }
        Ok(None)
    }

    pub fn get_leader_replica_by_database(&self, database_id: u64) -> Option<Replica> {
        self.get_replicas()
            .unwrap()
            .into_iter()
            .find(|instance| instance.database_id == database_id && instance.leader)
    }

    pub fn get_replicas_by_database(&self, database_id: u64) -> Result<Vec<Replica>> {
        // TODO: because we don't have foreign key constraints it's actually possible to have
        // instances in here with no database. Although we'd be in a bit of a corrupted state
        // in that case
        //
        // let tree = self.db.open_tree("database")?;
        // if !tree.contains_key(database_id.to_be_bytes())? {
        //     return Err(anyhow::anyhow!("No such database."));
        // }
        //
        let replicas = self
            .get_replicas()?
            .iter()
            .filter(|instance| instance.database_id == database_id)
            .cloned()
            .collect::<Vec<_>>();
        Ok(replicas)
    }

    pub fn insert_replica(&self, mut replica: Replica) -> Result<u64> {
        let tree = self.db.open_tree("replica")?;

        let id = self.db.generate_id()?;

        replica.id = id;
        let buf = bsatn::to_vec(&replica).unwrap();

        tree.insert(id.to_be_bytes(), buf)?;

        Ok(id)
    }

    pub fn delete_replica(&self, id: u64) -> Result<()> {
        let tree = self.db.open_tree("replica")?;
        tree.remove(id.to_be_bytes())?;
        Ok(())
    }

    pub fn _get_nodes(&self) -> Result<Vec<Node>> {
        let tree = self.db.open_tree("node")?;
        let mut nodes = Vec::new();
        let scan_key: &[u8] = b"";
        for result in tree.range(scan_key..) {
            let (_key, value) = result?;
            let node = bsatn::from_slice(&value[..]).unwrap();
            nodes.push(node);
        }
        Ok(nodes)
    }

    pub fn _get_node(&self, id: u64) -> Result<Option<Node>> {
        let tree = self.db.open_tree("node")?;

        let value = tree.get(id.to_be_bytes())?;
        if let Some(value) = value {
            let node = bsatn::from_slice(&value[..])?;
            Ok(Some(node))
        } else {
            Ok(None)
        }
    }

    pub fn _insert_node(&self, mut node: Node) -> Result<u64> {
        let tree = self.db.open_tree("node")?;

        let id = self.db.generate_id()?;

        node.id = id;
        let buf = bsatn::to_vec(&node).unwrap();

        tree.insert(id.to_be_bytes(), buf)?;

        Ok(id)
    }

    pub fn _update_node(&self, node: Node) -> Result<()> {
        let tree = self.db.open_tree("node")?;

        let buf = bsatn::to_vec(&node).unwrap();

        tree.insert(node.id.to_be_bytes(), buf)?;
        Ok(())
    }

    pub fn _delete_node(&self, id: u64) -> Result<()> {
        let tree = self.db.open_tree("node")?;
        tree.remove(id.to_be_bytes())?;
        Ok(())
    }

    /// Return the current budget for all identities as stored in the db.
    /// Note: this function is for the stored budget only and should *only* be called by functions in
    /// `control_budget`, where a cached copy is stored along with business logic for managing it.
    pub fn _get_energy_balances(&self) -> Result<Vec<EnergyBalance>> {
        let mut balances = vec![];
        let tree = self.db.open_tree("energy_budget")?;
        for balance_entry in tree.iter() {
            let balance_entry = match balance_entry {
                Ok(e) => e,
                Err(e) => {
                    log::error!("Invalid iteration in energy_budget control_db tree: {}", e);
                    continue;
                }
            };
            let arr = <[u8; 16]>::try_from(balance_entry.1.as_ref()).map_err(|_| bsatn::DecodeError::BufferLength {
                for_type: "balance_entry".into(),
                expected: 16,
                given: balance_entry.1.len(),
            })?;
            let balance = i128::from_be_bytes(arr);
            let energy_balance = EnergyBalance {
                identity: Identity::from_slice(balance_entry.0.iter().as_slice()),
                balance,
            };
            balances.push(energy_balance);
        }
        Ok(balances)
    }

    /// Return the current budget for a given identity as stored in the db.
    /// Note: this function is for the stored budget only and should *only* be called by functions in
    /// `control_budget`, where a cached copy is stored along with business logic for managing it.
    pub fn get_energy_balance(&self, identity: &Identity) -> Result<Option<energy::EnergyBalance>> {
        let tree = self.db.open_tree("energy_budget")?;
        let value = tree.get(identity.to_byte_array())?;
        if let Some(value) = value {
            let arr = <[u8; 16]>::try_from(value.as_ref()).map_err(|_| bsatn::DecodeError::BufferLength {
                for_type: "Identity".into(),
                expected: 16,
                given: value.as_ref().len(),
            })?;
            let balance = i128::from_be_bytes(arr);
            Ok(Some(energy::EnergyBalance::new(balance)))
        } else {
            Ok(None)
        }
    }

    /// Update the stored current budget for a identity.
    /// Note: this function is for the stored budget only and should *only* be called by functions in
    /// `control_budget`, where a cached copy is stored along with business logic for managing it.
    pub fn set_energy_balance(&self, identity: Identity, energy_balance: energy::EnergyBalance) -> Result<()> {
        let tree = self.db.open_tree("energy_budget")?;
        tree.insert(identity.to_byte_array(), &energy_balance.get().to_be_bytes())?;

        Ok(())
    }
}

mod compat {
    use spacetimedb::hash::Hash;
    use spacetimedb::messages::control_db::{Database as CanonicalDatabase, HostType};
    use spacetimedb::Identity;
    use spacetimedb_lib::bsatn::ser::BsatnError;
    use spacetimedb_lib::bsatn::{self, DecodeError};
    use spacetimedb_lib::{de::Deserialize, ser::Serialize};

    /// Serialized form of a [`spacetimedb::messages::control_db::Database`].
    ///
    /// To maintain compatibility.
    #[derive(Serialize, Deserialize)]
    pub(super) struct Database {
        id: u64,
        database_identity: Identity,
        owner_identity: Identity,
        host_type: HostType,
        initial_program: Hash,
    }

    impl Database {
        pub fn database_identity(&self) -> Identity {
            self.database_identity
        }

        #[inline]
        pub fn from_slice(s: &[u8]) -> Result<Self, DecodeError> {
            bsatn::from_slice(s)
        }

        #[inline]
        pub fn to_vec(&self) -> Result<Vec<u8>, BsatnError> {
            bsatn::to_vec(self)
        }
    }

    impl From<Database> for CanonicalDatabase {
        fn from(
            Database {
                id,
                database_identity,
                owner_identity,
                host_type,
                initial_program,
            }: Database,
        ) -> Self {
            Self {
                id,
                database_identity,
                owner_identity,
                host_type,
                initial_program,
            }
        }
    }

    impl From<CanonicalDatabase> for Database {
        fn from(
            CanonicalDatabase {
                id,
                database_identity,
                owner_identity,
                host_type,
                initial_program,
            }: CanonicalDatabase,
        ) -> Self {
            Self {
                id,
                database_identity,
                owner_identity,
                host_type,
                initial_program,
            }
        }
    }
}
