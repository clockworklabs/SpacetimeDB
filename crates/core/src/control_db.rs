use crate::address::Address;

use crate::hash::hash_bytes;
use crate::identity::Identity;
use crate::protobuf::control_db::{Database, DatabaseInstance, EnergyBalance, IdentityEmail, Node};
use crate::stdb_path;
use prost::Message;

use spacetimedb_lib::name::{parse_domain_name, DomainParsingError, InsertDomainResult, RegisterTldResult};
use spacetimedb_lib::recovery::RecoveryCode;

// TODO: Consider making not static
lazy_static::lazy_static! {
    pub static ref CONTROL_DB: ControlDb = ControlDb::init().unwrap();
}

pub struct ControlDb {
    db: sled::Db,
}

pub type Result<T> = core::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("collection not found")]
    CollectionNotFound(sled::Error),
    #[error("database error")]
    DatabaseError(sled::Error),
    #[error("record with the name {0} already exists")]
    RecordAlreadyExists(String),
    #[error("database with address {0} already exists")]
    DatabaseAlreadyExists(String),
    #[error("failed to register {0} domain")]
    DomainRegistrationFailure(String),
    #[error("failed to decode data")]
    DecodingError(prost::DecodeError),
    #[error(transparent)]
    DomainParsingError(#[from] DomainParsingError),
    #[error(transparent)]
    JSONDeserializationError(#[from] serde_json::Error),
}

impl From<sled::Error> for Error {
    fn from(err: sled::Error) -> Self {
        match err {
            sled::Error::CollectionNotFound(_) => Error::CollectionNotFound(err),
            err => Error::DatabaseError(err),
        }
    }
}

impl From<prost::DecodeError> for Error {
    fn from(err: prost::DecodeError) -> Self {
        Error::DecodingError(err)
    }
}

impl ControlDb {
    fn init() -> Result<Self> {
        let config = sled::Config::default()
            .path(stdb_path("control_node/control_db"))
            .flush_every_ms(Some(50))
            .mode(sled::Mode::HighThroughput);
        let db = config.open()?;
        Ok(Self { db })
    }
}

impl ControlDb {
    pub async fn spacetime_dns(&self, domain: &str) -> Result<Option<Address>> {
        let tree = self.db.open_tree("dns")?;
        let value = tree.get(domain.as_bytes())?;
        if let Some(value) = value {
            return Ok(Some(Address::from_slice(&value[..])));
        }
        Ok(None)
    }

    pub async fn spacetime_reverse_dns(&self, address: &Address) -> Result<Vec<String>> {
        let tree = self.db.open_tree("reverse_dns")?;
        let value = tree.get(address.as_slice())?;
        if let Some(value) = value {
            let vec: Vec<String> = serde_json::from_slice(&value[..])?;
            return Ok(vec);
        }
        Ok(vec![])
    }

    /// Creates a new domain which points to address. For example:
    ///  * `my_domain/my_database`
    ///  * `my_company/my_team/my_product`
    ///
    /// A TLD itself is also a fully qualified database name:
    ///  * `clockworklabs`
    ///  * `bitcraft`
    ///  * `...`
    ///
    /// # Arguments
    ///  * `address` - The address the database name should point to
    ///  * `database_name` - The database name to register
    ///  * `owner_identity` - The identity that is publishing the database name
    pub async fn spacetime_insert_domain(
        &self,
        address: &Address,
        domain: &str,
        owner_identity: Identity,
        try_register_tld: bool,
    ) -> Result<InsertDomainResult> {
        if self.spacetime_dns(domain).await?.is_some() {
            return Err(Error::RecordAlreadyExists(domain.to_string()));
        }
        let result = parse_domain_name(domain)?;
        match self.spacetime_lookup_tld(result.tld.as_str()).await? {
            Some(owner) => {
                if owner != owner_identity {
                    return Ok(InsertDomainResult::PermissionDenied {
                        domain: domain.to_string(),
                    });
                }
            }
            None => {
                if try_register_tld {
                    let parsed = parse_domain_name(domain)?;
                    // Let's try to automatically register this TLD for the identity
                    let result = CONTROL_DB
                        .spacetime_register_tld(parsed.tld.as_str(), owner_identity)
                        .await?;
                    if let RegisterTldResult::Success { .. } = result {
                        // This identity now owns this TLD
                    } else {
                        // This is technically possibly due to race conditions
                        return Err(Error::DomainRegistrationFailure(domain.to_string()));
                    }
                } else {
                    return Ok(InsertDomainResult::TldNotRegistered {
                        domain: domain.to_string(),
                    });
                }
            }
        }

        let tree = self.db.open_tree("dns")?;
        tree.insert(domain.as_bytes(), &address.as_slice()[..])?;

        let tree = self.db.open_tree("reverse_dns")?;
        match tree.get(address.as_slice())? {
            Some(value) => {
                let mut vec: Vec<String> = serde_json::from_slice(&value[..])?;
                vec.push(domain.to_string());
                tree.insert(address.as_slice(), serde_json::to_string(&vec)?.as_bytes())?;
            }
            None => {
                tree.insert(address.as_slice(), serde_json::to_string(&vec![domain])?.as_bytes())?;
            }
        }

        Ok(InsertDomainResult::Success {
            domain: domain.to_string(),
            address: address.to_hex(),
        })
    }

    /// Inserts a top level domain that will be owned by `owner_identity`.
    ///
    /// # Arguments
    ///
    /// * `domain` - The domain name to register
    /// * `owner_identity` - The identity that should own this domain name.
    pub async fn spacetime_register_tld(&self, tld: &str, owner_identity: Identity) -> Result<RegisterTldResult> {
        let tree = self.db.open_tree("top_level_domains")?;
        let current_owner = tree.get(tld.as_bytes())?;
        match current_owner {
            Some(owner) => {
                if Identity::from_slice(&owner[..]) == owner_identity {
                    Ok(RegisterTldResult::AlreadyRegistered {
                        domain: tld.to_string(),
                    })
                } else {
                    Ok(RegisterTldResult::Unauthorized {
                        domain: tld.to_string(),
                    })
                }
            }
            None => {
                tree.insert(tld.as_bytes(), owner_identity.as_slice())?;
                Ok(RegisterTldResult::Success {
                    domain: tld.to_string(),
                })
            }
        }
    }

    /// Starts a recovery code request
    ///
    ///  * `email` - The email to send the recovery code to
    pub async fn spacetime_insert_recovery_code(&self, email: &str, new_code: RecoveryCode) -> Result<()> {
        // TODO(jdetter): This function should take an identity instead of an email
        let tree = self.db.open_tree("recovery_codes")?;
        let current_requests = tree.get(email.as_bytes())?;
        match current_requests {
            None => {
                tree.insert(email.as_bytes(), serde_json::to_string(&vec![new_code])?.as_bytes())?;
            }
            Some(codes_bytes) => {
                let mut codes: Vec<RecoveryCode> = serde_json::from_slice(&codes_bytes[..])?;
                codes.push(new_code);
                tree.insert(email.as_bytes(), serde_json::to_string(&codes)?.as_bytes())?;
            }
        }

        Ok(())
    }

    pub async fn spacetime_get_recovery_code(&self, email: &str, code: &str) -> Result<Option<RecoveryCode>> {
        let tree = self.db.open_tree("recovery_codes")?;
        let current_requests = tree.get(email.as_bytes())?;
        match current_requests {
            None => Ok(None),
            Some(codes_bytes) => {
                let codes: Vec<RecoveryCode> = serde_json::from_slice(&codes_bytes[..])?;
                for recovery_code in codes {
                    if recovery_code.code == code {
                        return Ok(Some(recovery_code));
                    }
                }

                Ok(None)
            }
        }
    }

    /// Returns the owner (or `None` if there is no owner) of the domain.
    ///
    /// # Arguments
    ///  * `domain` - The domain to lookup
    pub async fn spacetime_lookup_tld(&self, domain: &str) -> Result<Option<Identity>> {
        // Make sure the domain is valid first
        parse_domain_name(domain)?;

        let tree = self.db.open_tree("top_level_domains")?;
        return match tree.get(domain.as_bytes())? {
            Some(owner) => Ok(Some(Identity::from_slice(&owner[..]))),
            None => Ok(None),
        };
    }

    pub async fn alloc_spacetime_identity(&self) -> Result<Identity> {
        // TODO: this really doesn't need to be a single global count
        let id = self.db.generate_id()?;
        let bytes: &[u8] = &id.to_le_bytes();
        let name = b"clockworklabs:";
        let bytes = [name, bytes].concat();
        let hash = Identity::from_hashing_bytes(bytes);
        Ok(hash)
    }

    pub async fn alloc_spacetime_address(&self) -> Result<Address> {
        // TODO: this really doesn't need to be a single global count
        // We could do something more intelligent for addresses...
        // A. generating them randomly
        // B. doing ipv6 generation
        let id = self.db.generate_id()?;
        let bytes: &[u8] = &id.to_le_bytes();
        let name = b"clockworklabs:";
        let bytes = [name, bytes].concat();
        let hash = hash_bytes(bytes);
        let address = Address::from_slice(&hash.as_slice()[0..16]);
        Ok(address)
    }

    pub async fn associate_email_spacetime_identity(&self, identity: &Identity, email: &str) -> Result<()> {
        // Lowercase the email before storing
        let email = email.to_lowercase();

        let tree = self.db.open_tree("email")?;
        let identity_email = IdentityEmail {
            identity: identity.as_slice().to_vec(),
            email,
        };
        let mut buf = Vec::new();
        identity_email.encode(&mut buf).unwrap();
        tree.insert(identity.as_slice(), buf)?;
        Ok(())
    }

    pub fn get_identities_for_email(&self, email: &str) -> Result<Vec<IdentityEmail>> {
        let mut result = Vec::<IdentityEmail>::new();
        let tree = self.db.open_tree("email")?;
        for i in tree.iter() {
            let i = i?;
            let iemail = IdentityEmail::decode(&i.1[..])?;
            if iemail.email == email {
                result.push(iemail);
            }
        }
        Ok(result)
    }

    pub async fn get_databases(&self) -> Result<Vec<Database>> {
        let tree = self.db.open_tree("database")?;
        let mut databases = Vec::new();
        let scan_key: &[u8] = b"";
        for result in tree.range(scan_key..) {
            let (_key, value) = result?;
            let database = Database::decode(&value[..]).unwrap();
            databases.push(database);
        }
        Ok(databases)
    }

    pub async fn get_database_by_id(&self, id: u64) -> Result<Option<Database>> {
        for database in self.get_databases().await? {
            if database.id == id {
                return Ok(Some(database));
            }
        }
        Ok(None)
    }

    pub async fn get_database_by_address(&self, address: &Address) -> Result<Option<Database>> {
        let tree = self.db.open_tree("database_by_address")?;
        let key = address.to_hex();
        let value = tree.get(key.as_bytes())?;
        if let Some(value) = value {
            let database = Database::decode(&value[..]).unwrap();
            return Ok(Some(database));
        }
        Ok(None)
    }

    pub async fn insert_database(&self, mut database: Database) -> Result<u64> {
        let id = self.db.generate_id()?;
        let tree = self.db.open_tree("database_by_address")?;

        let key = Address::from_slice(&database.address).to_hex();
        if tree.contains_key(key.as_bytes())? {
            return Err(Error::DatabaseAlreadyExists(key));
        }

        database.id = id;

        let mut buf = Vec::new();
        database.encode(&mut buf).unwrap();

        tree.insert(key, buf.clone())?;

        let tree = self.db.open_tree("database")?;
        tree.insert(id.to_be_bytes(), buf)?;

        Ok(id)
    }

    pub async fn update_database(&self, database: Database) -> Result<()> {
        let tree = self.db.open_tree("database")?;
        let tree_by_address = self.db.open_tree("database_by_address")?;
        let key = Address::from_slice(&database.address).to_hex();

        let old_value = tree.get(database.id.to_be_bytes())?;
        if let Some(old_value) = old_value {
            let old_database = Database::decode(&old_value[..])?;

            if database.address != old_database.address && tree_by_address.contains_key(key.as_bytes())? {
                return Err(Error::DatabaseAlreadyExists(key));
            }
        }

        let mut buf = Vec::new();
        database.encode(&mut buf).unwrap();

        tree.insert(database.id.to_be_bytes(), buf.clone())?;

        let key = Address::from_slice(&database.address).to_hex();
        tree_by_address.insert(key, buf)?;

        Ok(())
    }

    pub async fn delete_database(&self, id: u64) -> Result<Option<u64>> {
        let tree = self.db.open_tree("database")?;
        let tree_by_address = self.db.open_tree("database_by_address")?;

        if let Some(old_value) = tree.get(id.to_be_bytes())? {
            let database = Database::decode(&old_value[..])?;
            let key = Address::from_slice(database.address).to_hex();

            tree_by_address.remove(key.as_bytes())?;
            tree.remove(id.to_be_bytes())?;
            return Ok(Some(id));
        }

        Ok(None)
    }

    pub async fn get_database_instances(&self) -> Result<Vec<DatabaseInstance>> {
        let tree = self.db.open_tree("database_instance")?;
        let mut database_instances = Vec::new();
        let scan_key: &[u8] = b"";
        for result in tree.range(scan_key..) {
            let (_key, value) = result?;
            let database_instance = DatabaseInstance::decode(&value[..]).unwrap();
            database_instances.push(database_instance);
        }
        Ok(database_instances)
    }

    pub async fn get_leader_database_instance_by_database(&self, database_id: u64) -> Option<DatabaseInstance> {
        self.get_database_instances()
            .await
            .unwrap()
            .into_iter()
            .find(|instance| instance.database_id == database_id && instance.leader)
    }

    pub async fn get_database_instances_by_database(&self, database_id: u64) -> Result<Vec<DatabaseInstance>> {
        // TODO: because we don't have foreign key constraints it's actually possible to have
        // instances in here with no database. Although we'd be in a bit of a corrupted state
        // in that case
        //
        // let tree = self.db.open_tree("database")?;
        // if !tree.contains_key(database_id.to_be_bytes())? {
        //     return Err(anyhow::anyhow!("No such database."));
        // }
        //
        let database_instances = self
            .get_database_instances()
            .await?
            .iter()
            .filter(|instance| instance.database_id == database_id)
            .cloned()
            .collect::<Vec<_>>();
        Ok(database_instances)
    }

    pub async fn insert_database_instance(&self, mut database_instance: DatabaseInstance) -> Result<u64> {
        let tree = self.db.open_tree("database_instance")?;

        let id = self.db.generate_id()?;

        database_instance.id = id;
        let mut buf = Vec::new();
        database_instance.encode(&mut buf).unwrap();

        tree.insert(id.to_be_bytes(), buf)?;

        Ok(id)
    }

    pub async fn _update_database_instance(&self, database_instance: DatabaseInstance) -> Result<()> {
        let tree = self.db.open_tree("database_instance")?;

        let mut buf = Vec::new();
        database_instance.encode(&mut buf).unwrap();

        tree.insert(database_instance.id.to_be_bytes(), buf.clone())?;
        Ok(())
    }

    pub async fn delete_database_instance(&self, id: u64) -> Result<()> {
        let tree = self.db.open_tree("database_instance")?;
        tree.remove(id.to_be_bytes())?;
        Ok(())
    }

    pub async fn get_nodes(&self) -> Result<Vec<Node>> {
        let tree = self.db.open_tree("node")?;
        let mut nodes = Vec::new();
        let scan_key: &[u8] = b"";
        for result in tree.range(scan_key..) {
            let (_key, value) = result?;
            let node = Node::decode(&value[..]).unwrap();
            nodes.push(node);
        }
        Ok(nodes)
    }

    pub async fn get_node(&self, id: u64) -> Result<Option<Node>> {
        let tree = self.db.open_tree("node")?;

        let value = tree.get(id.to_be_bytes())?;
        if let Some(value) = value {
            let node = Node::decode(&value[..])?;
            Ok(Some(node))
        } else {
            Ok(None)
        }
    }

    pub async fn insert_node(&self, mut node: Node) -> Result<u64> {
        let tree = self.db.open_tree("node")?;

        let id = self.db.generate_id()?;

        node.id = id;
        let mut buf = Vec::new();
        node.encode(&mut buf).unwrap();

        tree.insert(id.to_be_bytes(), buf)?;

        Ok(id)
    }

    pub async fn update_node(&self, node: Node) -> Result<()> {
        let tree = self.db.open_tree("node")?;

        let mut buf = Vec::new();
        node.encode(&mut buf).unwrap();

        tree.insert(node.id.to_be_bytes(), buf.clone())?;
        Ok(())
    }

    pub async fn _delete_node(&self, id: u64) -> Result<()> {
        let tree = self.db.open_tree("node")?;
        tree.remove(id.to_be_bytes())?;
        Ok(())
    }

    /// Return the current budget for all identities as stored in the db.
    /// Note: this function is for the stored budget only and should *only* be called by functions in
    /// `control_budget`, where a cached copy is stored along with business logic for managing it.
    pub async fn get_energy_balances(&self) -> Result<Vec<EnergyBalance>> {
        let mut budgets = vec![];
        let tree = self.db.open_tree("energy_budget")?;
        for budget_entry in tree.iter() {
            let budget_entry = match budget_entry {
                Ok(budget_entry) => budget_entry,
                Err(e) => {
                    log::error!("Invalid iteration in energy_budget control_db tree: {}", e);
                    continue;
                }
            };
            let energy_budget = match EnergyBalance::decode(&budget_entry.1[..]) {
                Ok(balance) => balance,
                Err(e) => {
                    log::error!("Invalid value in energy_balance control_db tree: {}", e);
                    continue;
                }
            };
            budgets.push(energy_budget);
        }
        Ok(budgets)
    }

    /// Return the current budget for a given identity as stored in the db.
    /// Note: this function is for the stored budget only and should *only* be called by functions in
    /// `control_budget`, where a cached copy is stored along with business logic for managing it.
    pub async fn get_energy_balance(&self, identity: &Identity) -> Result<Option<EnergyBalance>> {
        let tree = self.db.open_tree("energy_budget")?;
        let key = identity.to_hex();
        let value = tree.get(key.as_bytes())?;
        if let Some(value) = value {
            let budget = EnergyBalance::decode(&value[..]).unwrap();
            Ok(Some(budget))
        } else {
            Ok(None)
        }
    }

    /// Update the stored current budget for a identity.
    /// Note: this function is for the stored budget only and should *only* be called by functions in
    /// `control_budget`, where a cached copy is stored along with business logic for managing it.
    pub fn set_energy_balance(&self, identity: &Identity, budget: &EnergyBalance) -> Result<()> {
        let tree = self.db.open_tree("energy_budget")?;
        let key = identity.to_hex();
        let mut buf = Vec::new();
        budget.encode(&mut buf).unwrap();
        tree.insert(key, buf.clone())?;

        Ok(())
    }
}
