use std::borrow::Cow;

use anyhow::{anyhow, Context};

use crate::address::Address;

use crate::hash::hash_bytes;
use crate::identity::Identity;
use crate::messages::control_db::{Database, DatabaseInstance, EnergyBalance, IdentityEmail, Node};
use crate::{energy, stdb_path};

use spacetimedb_lib::name::{DomainName, DomainParsingError, InsertDomainResult, RegisterTldResult, Tld, TldRef};
use spacetimedb_lib::recovery::RecoveryCode;
use spacetimedb_sats::bsatn;

#[cfg(test)]
mod tests;

#[derive(Clone)]
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
    RecordAlreadyExists(DomainName),
    #[error("database with address {0} already exists")]
    DatabaseAlreadyExists(Address),
    #[error("failed to register {0} domain")]
    DomainRegistrationFailure(DomainName),
    #[error("failed to decode data")]
    DecodingError(#[from] bsatn::DecodeError),
    #[error(transparent)]
    DomainParsingError(#[from] DomainParsingError),
    #[error("connection error")]
    ConnectionError(),
    #[error(transparent)]
    JSONDeserializationError(#[from] serde_json::Error),
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<sled::Error> for Error {
    fn from(err: sled::Error) -> Self {
        match err {
            sled::Error::CollectionNotFound(_) => Error::CollectionNotFound(err),
            err => Error::DatabaseError(err),
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
    pub fn spacetime_dns(&self, domain: &DomainName) -> Result<Option<Address>> {
        let tree = self.db.open_tree("dns")?;
        let value = tree.get(domain.to_lowercase().as_bytes())?;
        if let Some(value) = value {
            return Ok(Some(Address::from_slice(&value[..])));
        }
        Ok(None)
    }

    pub fn spacetime_reverse_dns(&self, address: &Address) -> Result<Vec<DomainName>> {
        let tree = self.db.open_tree("reverse_dns")?;
        let value = tree.get(address.as_slice())?;
        if let Some(value) = value {
            let vec: Vec<DomainName> = serde_json::from_slice(&value[..])?;
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
    pub fn spacetime_insert_domain(
        &self,
        address: &Address,
        domain: DomainName,
        owner_identity: Identity,
        try_register_tld: bool,
    ) -> Result<InsertDomainResult> {
        let address = *address;
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

        let tree = self.db.open_tree("dns")?;
        tree.insert(domain.to_lowercase().as_bytes(), &address.as_slice()[..])?;

        let tree = self.db.open_tree("reverse_dns")?;
        match tree.get(address.as_slice())? {
            Some(value) => {
                let mut vec: Vec<DomainName> = serde_json::from_slice(&value[..])?;
                vec.push(domain.clone());
                tree.insert(address.as_slice(), serde_json::to_string(&vec)?.as_bytes())?;
            }
            None => {
                tree.insert(address.as_slice(), serde_json::to_string(&vec![&domain])?.as_bytes())?;
            }
        }

        Ok(InsertDomainResult::Success { domain, address })
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
                tree.insert(key, owner_identity.as_bytes())?;
                Ok(RegisterTldResult::Success { domain: tld })
            }
        }
    }

    /// Starts a recovery code request
    ///
    ///  * `email` - The email to send the recovery code to
    pub fn spacetime_insert_recovery_code(&self, email: &str, new_code: RecoveryCode) -> Result<()> {
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

    pub fn spacetime_get_recovery_codes(&self, email: &str) -> Result<Vec<RecoveryCode>> {
        let tree = self.db.open_tree("recovery_codes")?;
        let current_requests = tree.get(email.as_bytes())?;
        current_requests
            .map(|bytes| {
                let codes: Vec<RecoveryCode> = serde_json::from_slice(&bytes[..])?;
                Ok(codes)
            })
            .unwrap_or(Ok(vec![]))
    }

    pub fn spacetime_get_recovery_code(&self, email: &str, code: &str) -> Result<Option<RecoveryCode>> {
        for recovery_code in self.spacetime_get_recovery_codes(email)? {
            if recovery_code.code == code {
                return Ok(Some(recovery_code));
            }
        }

        Ok(None)
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

    pub fn alloc_spacetime_identity(&self) -> Result<Identity> {
        // TODO: this really doesn't need to be a single global count
        let id = self.db.generate_id()?;
        let bytes: &[u8] = &id.to_le_bytes();
        let name = b"clockworklabs:";
        let bytes = [name, bytes].concat();
        let hash = Identity::from_hashing_bytes(bytes);
        Ok(hash)
    }

    pub fn alloc_spacetime_address(&self) -> Result<Address> {
        // TODO: this really doesn't need to be a single global count
        // We could do something more intelligent for addresses...
        // A. generating them randomly
        // B. doing ipv6 generation
        let id = self.db.generate_id()?;
        let bytes: &[u8] = &id.to_le_bytes();
        let name = b"clockworklabs:";
        let bytes = [name, bytes].concat();
        let hash = hash_bytes(bytes);
        let address = Address::from_slice(&hash.as_slice()[..16]);
        Ok(address)
    }

    pub async fn associate_email_spacetime_identity(&self, identity: Identity, email: &str) -> Result<()> {
        // Lowercase the email before storing
        let email = email.to_lowercase();

        let tree = self.db.open_tree("email")?;
        let identity_email = IdentityEmail { identity, email };
        let buf = bsatn::to_vec(&identity_email).unwrap();
        tree.insert(identity.as_bytes(), buf)?;
        Ok(())
    }

    pub fn get_identities_for_email(&self, email: &str) -> Result<Vec<IdentityEmail>> {
        let mut result = Vec::<IdentityEmail>::new();
        let tree = self.db.open_tree("email")?;
        for i in tree.iter() {
            let (_, value) = i?;
            let iemail: IdentityEmail = bsatn::from_slice(&value)?;
            if iemail.email == email {
                result.push(iemail);
            }
        }
        Ok(result)
    }

    pub fn get_emails_for_identity(&self, identity: &Identity) -> Result<Vec<IdentityEmail>> {
        let mut result = Vec::<IdentityEmail>::new();
        let tree = self.db.open_tree("email")?;
        for i in tree.iter() {
            let (_, value) = i?;
            let iemail: IdentityEmail = bsatn::from_slice(&value)?;
            if &iemail.identity == identity {
                result.push(iemail);
            }
        }
        Ok(result)
    }

    pub fn get_databases(&self) -> Result<Vec<Database>> {
        let tree = self.db.open_tree("database")?;
        let mut databases = Vec::new();
        let scan_key: &[u8] = b"";
        for result in tree.range(scan_key..) {
            let (_key, value) = result?;
            let database = bsatn::from_slice(&value).unwrap();
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

    pub fn get_database_by_address(&self, address: &Address) -> Result<Option<Database>> {
        let tree = self.db.open_tree("database_by_address")?;
        let key = address.to_hex();
        let value = tree.get(key.as_bytes())?;
        if let Some(value) = value {
            let database = bsatn::from_slice(&value[..]).unwrap();
            return Ok(Some(database));
        }
        Ok(None)
    }

    pub fn insert_database(&self, mut database: Database) -> Result<u64> {
        let id = self.db.generate_id()?;
        let tree = self.db.open_tree("database_by_address")?;

        let key = database.address.to_hex();
        if tree.contains_key(key)? {
            return Err(Error::DatabaseAlreadyExists(database.address));
        }

        database.id = id;

        let buf = sled::IVec::from(bsatn::to_vec(&database).unwrap());

        tree.insert(key, buf.clone())?;

        let tree = self.db.open_tree("database")?;
        tree.insert(id.to_be_bytes(), buf)?;

        Ok(id)
    }

    pub fn update_database(&self, database: Database) -> Result<()> {
        let tree = self.db.open_tree("database")?;
        let tree_by_address = self.db.open_tree("database_by_address")?;
        let key = database.address.to_hex();

        let old_value = tree.get(database.id.to_be_bytes())?;
        if let Some(old_value) = old_value {
            let old_database: Database = bsatn::from_slice(&old_value[..])?;

            if database.address != old_database.address && tree_by_address.contains_key(key.as_bytes())? {
                return Err(Error::DatabaseAlreadyExists(database.address));
            }
        }

        let buf = sled::IVec::from(bsatn::to_vec(&database).unwrap());

        tree.insert(database.id.to_be_bytes(), buf.clone())?;

        let key = database.address.to_hex();
        tree_by_address.insert(key, buf)?;

        Ok(())
    }

    pub fn delete_database(&self, id: u64) -> Result<Option<u64>> {
        let tree = self.db.open_tree("database")?;
        let tree_by_address = self.db.open_tree("database_by_address")?;

        if let Some(old_value) = tree.get(id.to_be_bytes())? {
            let database: Database = bsatn::from_slice(&old_value[..])?;
            let key = database.address.to_hex();

            tree_by_address.remove(key.as_bytes())?;
            tree.remove(id.to_be_bytes())?;
            return Ok(Some(id));
        }

        Ok(None)
    }

    pub fn get_database_instances(&self) -> Result<Vec<DatabaseInstance>> {
        let tree = self.db.open_tree("database_instance")?;
        let mut database_instances = Vec::new();
        let scan_key: &[u8] = b"";
        for result in tree.range(scan_key..) {
            let (_key, value) = result?;
            let database_instance = bsatn::from_slice(&value[..]).unwrap();
            database_instances.push(database_instance);
        }
        Ok(database_instances)
    }

    pub fn get_database_instance_by_id(&self, database_instance_id: u64) -> Result<Option<DatabaseInstance>> {
        for di in self.get_database_instances()? {
            if di.id == database_instance_id {
                return Ok(Some(di));
            }
        }
        Ok(None)
    }

    pub fn get_leader_database_instance_by_database(&self, database_id: u64) -> Option<DatabaseInstance> {
        self.get_database_instances()
            .unwrap()
            .into_iter()
            .find(|instance| instance.database_id == database_id && instance.leader)
    }

    pub fn get_database_instances_by_database(&self, database_id: u64) -> Result<Vec<DatabaseInstance>> {
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
            .get_database_instances()?
            .iter()
            .filter(|instance| instance.database_id == database_id)
            .cloned()
            .collect::<Vec<_>>();
        Ok(database_instances)
    }

    pub fn insert_database_instance(&self, mut database_instance: DatabaseInstance) -> Result<u64> {
        let tree = self.db.open_tree("database_instance")?;

        let id = self.db.generate_id()?;

        database_instance.id = id;
        let buf = bsatn::to_vec(&database_instance).unwrap();

        tree.insert(id.to_be_bytes(), buf)?;

        Ok(id)
    }

    pub fn update_database_instance(&self, database_instance: DatabaseInstance) -> Result<()> {
        let tree = self.db.open_tree("database_instance")?;

        let buf = bsatn::to_vec(&database_instance).unwrap();

        tree.insert(database_instance.id.to_be_bytes(), buf)?;
        Ok(())
    }

    pub fn delete_database_instance(&self, id: u64) -> Result<()> {
        let tree = self.db.open_tree("database_instance")?;
        tree.remove(id.to_be_bytes())?;
        Ok(())
    }

    pub fn get_nodes(&self) -> Result<Vec<Node>> {
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

    pub fn get_node(&self, id: u64) -> Result<Option<Node>> {
        let tree = self.db.open_tree("node")?;

        let value = tree.get(id.to_be_bytes())?;
        if let Some(value) = value {
            let node = bsatn::from_slice(&value[..])?;
            Ok(Some(node))
        } else {
            Ok(None)
        }
    }

    pub fn insert_node(&self, mut node: Node) -> Result<u64> {
        let tree = self.db.open_tree("node")?;

        let id = self.db.generate_id()?;

        node.id = id;
        let buf = bsatn::to_vec(&node).unwrap();

        tree.insert(id.to_be_bytes(), buf)?;

        Ok(id)
    }

    pub fn update_node(&self, node: Node) -> Result<()> {
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
    pub fn get_energy_balances(&self) -> Result<Vec<EnergyBalance>> {
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
            let Ok(arr) = <[u8; 16]>::try_from(balance_entry.1.as_ref()) else {
                return Err(Error::DecodingError(bsatn::DecodeError::BufferLength {
                    for_type: "balance_entry".into(),
                    expected: 16,
                    given: balance_entry.1.len(),
                }));
            };
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
        let value = tree.get(identity.as_bytes())?;
        if let Some(value) = value {
            let Ok(arr) = <[u8; 16]>::try_from(value.as_ref()) else {
                return Err(Error::DecodingError(bsatn::DecodeError::BufferLength {
                    for_type: "Identity".into(),
                    expected: 16,
                    given: value.as_ref().len(),
                }));
            };
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
        tree.insert(identity.as_bytes(), &energy_balance.get().to_be_bytes())?;

        Ok(())
    }

    /// Acquire a lock on `key`.
    ///
    /// If the lock can not be acquired immediately, an error is returned.
    ///
    /// This method is essentially simulating locking in the distributed version
    /// of SpacetimeDB. It does not, however, provide time-based expiration of
    /// a lock.
    pub fn lock<'a, S>(&self, key: S) -> Result<Lock<'a>>
    where
        S: Into<Cow<'a, str>>,
    {
        let tree = self.db.open_tree("locks")?;
        let token = self.db.generate_id().map(Some)?;
        let key = key.into();
        match cas_u64(&tree, &key, None, token)? {
            Ok(()) => Ok(Lock { key, token, tree }),
            Err(_) => Err(anyhow!("Lock on `{key}` taken").into()),
        }
    }
}

/// A keyed lock acquired by [`ControlDb::lock`].
///
/// The lock is released on drop, or by calling [`Lock::release`].
pub struct Lock<'a> {
    key: Cow<'a, str>,
    token: Option<u64>,
    tree: sled::Tree,
}

impl Lock<'_> {
    /// Return the [fencing token] associated with this lock.
    ///
    /// [fencing token]: https://martin.kleppmann.com/2016/02/08/how-to-do-distributed-locking.html
    pub fn token(&self) -> u64 {
        self.token.expect("fencing token must be set unless self was dropped")
    }

    /// Release this lock, consuming `self`.
    ///
    /// A [`Lock`] is automatically released when it goes out of scope, however
    /// any errors are lost in this case. Use [`Self::release`] to observe those
    /// errors.
    pub fn release(mut self) -> Result<()> {
        let this = &mut self;
        this.release_internal()
    }

    fn release_internal(&mut self) -> Result<()> {
        if let Some(tok) = self.token.take() {
            cas_u64(&self.tree, &self.key, Some(tok), None)?.context("lock token changed while held")?
        }
        Ok(())
    }
}

impl Drop for Lock<'_> {
    fn drop(&mut self) {
        if let Err(e) = self.release_internal() {
            log::error!("Failed to release lock on `{}`: {}", self.key, e);
        }
    }
}

/// [`sled::Tree::compare_and_swap`] specialized to `&str` keys and `u64` values.
fn cas_u64(
    tree: &sled::Tree,
    key: &str,
    old: Option<u64>,
    new: Option<u64>,
) -> sled::Result<std::result::Result<(), sled::CompareAndSwapError>> {
    tree.compare_and_swap(
        key,
        old.map(|x| x.to_be_bytes()).as_ref(),
        new.map(|x| x.to_be_bytes()).as_ref(),
    )
}
