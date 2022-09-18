use prost::Message;

use crate::hash::{hash_bytes, Hash};
use crate::protobuf::control_db::{Database, DatabaseInstance, EnergyBudget, IdentityEmail, Node};

// TODO: Consider making not static
lazy_static::lazy_static! {
    static ref CONTROL_DB: sled::Db = init().unwrap();
}

fn init() -> Result<sled::Db, anyhow::Error> {
    let config = sled::Config::default()
        .path("/stdb/control_node/control_db")
        .flush_every_ms(Some(50))
        .mode(sled::Mode::HighThroughput);
    let db = config.open()?;
    Ok(db)
}

pub async fn alloc_spacetime_identity() -> Result<Hash, anyhow::Error> {
    // TODO: this really doesn't need to be a single global count
    let id = CONTROL_DB.generate_id()?;
    let bytes: &[u8] = &id.to_le_bytes();
    let name = b"clockworklabs:";
    let bytes = [name, bytes].concat();
    let hash = hash_bytes(bytes);
    Ok(hash)
}

pub async fn associate_email_spacetime_identity(identity: &Hash, email: &str) -> Result<(), anyhow::Error> {
    // Lowercase the email before storing
    let email = email.to_lowercase();

    let tree = CONTROL_DB.open_tree("email")?;
    let identity_email = IdentityEmail {
        identity: identity.as_slice().to_vec(),
        email: email.to_string(),
    };
    let mut buf = Vec::new();
    identity_email.encode(&mut buf).unwrap();
    tree.insert(identity.as_slice(), buf)?;
    Ok(())
}

pub async fn get_databases() -> Result<Vec<Database>, anyhow::Error> {
    let tree = CONTROL_DB.open_tree("database")?;
    let mut databases = Vec::new();
    let scan_key: &[u8] = b"";
    for result in tree.range(scan_key..) {
        let (_key, value) = result?;
        let database = Database::decode(&value.to_vec()[..]).unwrap();
        databases.push(database);
    }
    Ok(databases)
}

pub async fn get_database_by_address(identity: &Hash, name: &str) -> Result<Option<Database>, anyhow::Error> {
    let tree = CONTROL_DB.open_tree("database_by_address")?;
    let key = format!("{}/{}", identity.to_hex(), name);
    let value = tree.get(key.as_bytes())?;
    if let Some(value) = value {
        let database = Database::decode(&value.to_vec()[..]).unwrap();
        return Ok(Some(database));
    }
    return Ok(None);
}

pub async fn insert_database(mut database: Database) -> Result<u64, anyhow::Error> {
    let id = CONTROL_DB.generate_id()?;
    let tree = CONTROL_DB.open_tree("database_by_address")?;

    let key = format!("{}/{}", Hash::from_slice(&database.identity).to_hex(), database.name);
    if tree.contains_key(key.as_bytes())? {
        return Err(anyhow::anyhow!("Database with address {} already exists", key));
    }

    database.id = id;

    let mut buf = Vec::new();
    database.encode(&mut buf).unwrap();

    tree.insert(key, buf.clone())?;

    let tree = CONTROL_DB.open_tree("database")?;
    tree.insert(id.to_be_bytes(), buf)?;

    Ok(id)
}

pub async fn update_database(database: Database) -> Result<(), anyhow::Error> {
    let tree = CONTROL_DB.open_tree("database")?;
    let tree_by_address = CONTROL_DB.open_tree("database_by_address")?;
    let key = format!("{}/{}", Hash::from_slice(&database.identity).to_hex(), database.name);

    let old_value = tree.get(database.id.to_be_bytes())?;
    if let Some(old_value) = old_value {
        let old_database = Database::decode(&old_value.to_vec()[..])?;

        if database.identity != old_database.identity || database.name != old_database.name {
            if tree_by_address.contains_key(key.as_bytes())? {
                return Err(anyhow::anyhow!("Database with address {} already exists", key));
            }
        }
    }

    let mut buf = Vec::new();
    database.encode(&mut buf).unwrap();

    tree.insert(database.id.to_be_bytes(), buf.clone())?;

    let key = format!("{}/{}", Hash::from_slice(&database.identity).to_hex(), database.name);
    tree_by_address.insert(key, buf)?;

    Ok(())
}

pub async fn delete_database(id: u64) -> Result<Option<u64>, anyhow::Error> {
    let tree = CONTROL_DB.open_tree("database")?;
    let tree_by_address = CONTROL_DB.open_tree("database_by_address")?;

    if let Some(old_value) = tree.get(id.to_be_bytes())? {
        let database = Database::decode(&old_value.to_vec()[..])?;
        let key = format!("{}/{}", Hash::from_slice(&database.identity).to_hex(), database.name);

        tree_by_address.remove(key.as_bytes())?;
        tree.remove(id.to_be_bytes())?;
        return Ok(Some(id));
    }

    Ok(None)
}

pub async fn get_database_instances() -> Result<Vec<DatabaseInstance>, anyhow::Error> {
    let tree = CONTROL_DB.open_tree("database_instance")?;
    let mut database_instances = Vec::new();
    let scan_key: &[u8] = b"";
    for result in tree.range(scan_key..) {
        let (_key, value) = result?;
        let database_instance = DatabaseInstance::decode(&value.to_vec()[..]).unwrap();
        database_instances.push(database_instance);
    }
    Ok(database_instances)
}

pub async fn get_database_instances_by_database(database_id: u64) -> Result<Vec<DatabaseInstance>, anyhow::Error> {
    // TODO: because we don't have foreign key constraints it's actually possible to have
    // instances in here with no database. Although we'd be in a bit of a corrupted state
    // in that case
    //
    // let tree = CONTROL_DB.open_tree("database")?;
    // if !tree.contains_key(database_id.to_be_bytes())? {
    //     return Err(anyhow::anyhow!("No such database."));
    // }
    //
    let database_instances = get_database_instances()
        .await?
        .iter()
        .filter(|instance| instance.database_id == database_id)
        .map(|i| i.clone())
        .collect::<Vec<_>>();
    Ok(database_instances)
}

pub async fn insert_database_instance(mut database_instance: DatabaseInstance) -> Result<u64, anyhow::Error> {
    let tree = CONTROL_DB.open_tree("database_instance")?;

    let id = CONTROL_DB.generate_id()?;

    database_instance.id = id;
    let mut buf = Vec::new();
    database_instance.encode(&mut buf).unwrap();

    tree.insert(id.to_be_bytes(), buf)?;

    Ok(id)
}

pub async fn _update_database_instance(database_instance: DatabaseInstance) -> Result<(), anyhow::Error> {
    let tree = CONTROL_DB.open_tree("database_instance")?;

    let mut buf = Vec::new();
    database_instance.encode(&mut buf).unwrap();

    tree.insert(database_instance.id.to_be_bytes(), buf.clone())?;
    Ok(())
}

pub async fn delete_database_instance(id: u64) -> Result<(), anyhow::Error> {
    let tree = CONTROL_DB.open_tree("database_instance")?;
    tree.remove(id.to_be_bytes())?;
    Ok(())
}

pub async fn get_nodes() -> Result<Vec<Node>, anyhow::Error> {
    let tree = CONTROL_DB.open_tree("node")?;
    let mut nodes = Vec::new();
    let scan_key: &[u8] = b"";
    for result in tree.range(scan_key..) {
        let (_key, value) = result?;
        let node = Node::decode(&value.to_vec()[..]).unwrap();
        nodes.push(node);
    }
    Ok(nodes)
}

pub async fn get_node(id: u64) -> Result<Option<Node>, anyhow::Error> {
    let tree = CONTROL_DB.open_tree("node")?;

    let value = tree.get(id.to_be_bytes())?;
    if let Some(value) = value {
        let node = Node::decode(&value[..])?;
        Ok(Some(node))
    } else {
        Ok(None)
    }
}

pub async fn insert_node(mut node: Node) -> Result<u64, anyhow::Error> {
    let tree = CONTROL_DB.open_tree("node")?;

    let id = CONTROL_DB.generate_id()?;

    node.id = id;
    let mut buf = Vec::new();
    node.encode(&mut buf).unwrap();

    tree.insert(id.to_be_bytes(), buf)?;

    Ok(id)
}

pub async fn update_node(node: Node) -> Result<(), anyhow::Error> {
    let tree = CONTROL_DB.open_tree("node")?;

    let mut buf = Vec::new();
    node.encode(&mut buf).unwrap();

    tree.insert(node.id.to_be_bytes(), buf.clone())?;
    Ok(())
}

pub async fn _delete_node(id: u64) -> Result<(), anyhow::Error> {
    let tree = CONTROL_DB.open_tree("node")?;
    tree.remove(id.to_be_bytes())?;
    Ok(())
}

/// Return the current budget for all module identities as stored in the db.
/// Note: this function is for the stored budget only and should *only* be called by functions in
/// `control_budget`, where a cached copy is stored along with business logic for managing it.
pub async fn get_energy_budgets() -> Result<Vec<EnergyBudget>, anyhow::Error> {
    let mut budgets = vec![];
    let tree = CONTROL_DB.open_tree("energy_budget")?;
    for budget_entry in tree.iter() {
        let budget_entry = match budget_entry {
            Ok(budget_entry) => budget_entry,
            Err(e) => {
                log::error!("Invalid iteration in energy_budget control_db tree: {}", e);
                continue;
            }
        };
        let energy_budget = match EnergyBudget::decode(&budget_entry.1[..]) {
            Ok(energy_budget) => energy_budget,
            Err(e) => {
                log::error!("Invalid value in energy_budget control_db tree: {}", e);
                continue;
            }
        };
        budgets.push(energy_budget);
    }
    Ok(budgets)
}

/// Return the current budget for a given module identity as stored in the db.
/// Note: this function is for the stored budget only and should *only* be called by functions in
/// `control_budget`, where a cached copy is stored along with business logic for managing it.
pub async fn get_energy_budget(module_identity: &Hash) -> Result<Option<EnergyBudget>, anyhow::Error> {
    let tree = CONTROL_DB.open_tree("energy_budget")?;
    let key = module_identity.to_hex();
    let value = tree.get(key.as_bytes())?;
    if let Some(value) = value {
        let budget = EnergyBudget::decode(&value.to_vec()[..]).unwrap();
        Ok(Some(budget))
    } else {
        Ok(None)
    }
}

/// Update the stored current budget for a module identity.
/// Note: this function is for the stored budget only and should *only* be called by functions in
/// `control_budget`, where a cached copy is stored along with business logic for managing it.
pub fn set_energy_budget(module_identity: &Hash, budget: &EnergyBudget) -> Result<(), anyhow::Error> {
    let tree = CONTROL_DB.open_tree("energy_budget")?;
    let key = module_identity.to_hex();
    let mut buf = Vec::new();
    budget.encode(&mut buf).unwrap();
    tree.insert(key, buf.clone())?;

    Ok(())
}
