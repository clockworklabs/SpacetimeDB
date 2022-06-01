#![spacetimedb(database = bitcraft)]
mod inventory;

use spacetimedb_bindings::*;

#[spacetimedb(table(private))]
struct User {
    #[spacetimedb(index = hash)]
    id: u64,
    #[spacetimedb(index = btree)]
    email: String,
}

#[spacetimedb(table)]
pub struct Entity {
    #[spacetimedb(unique, index = hash)]
    id: u64,
}

#[spacetimedb(1)] // define table with id with 1
#[spacetimedb(table(SignedIn, SignedOut))]
#[spacetimedb_index(btree, name=index1, entity_id, id)] // advance feature
#[spacetimedb_index(btree, entity_id, id)] // advance feature
pub struct PlayerState {
    #[foreign_key(Entity(id))]
    entity_id: u64,
    #[spacetimedb(index = btree)]
    x: i32,
    #[spacetimedb(index = btree)]
    z: i32,

    #[index(btree), primary_key, col_id(5)] // be able to override col_id
    entity_id: u64,
    #[primary_key] // simple index
    id: u64,
    #[spacetimedb(index = hash)]
    actor: u64,
    #[spacetimedb(index = btree)]
    username: String,
    #[spacetimedb]
    inventory: Inventory,
}

#[spacetimedb(table)]
pub struct Location {
    #[foreign_key(Entity(id))]
    entity_id: u64,
    #[spacetimedb(index = btree)]
    x: i32,
    #[spacetimedb(index = btree)]
    z: i32,
}

#[spacetimedb(table)]
pub struct Health {
    #[spacetimedb(foreign_key=Entity(id))]
    entity_id: u64,
    #[spacetimedb]
    pub health: f32,
}

#[spacetimedb(table)]
pub struct Resource {
    #[spacetimedb(foreign_key=Entity(id))]
    entity_id: u64,
    #[spacetimedb]
    pub resource_id: u32, 
    #[spacetimedb]
    pub quantity: u32,
}

#[spacetimedb(table)]
pub struct ResourceDef {
    #[primary_key, foreign_key(Entities.id)]
    pub resource_id: u32,
    #[primary_key]
    pub walkable: bool,
}

#[spacetimedb(table)]
pub struct TerrainChunk {
    #[spacetimedb]
    pub chunk_x: u32,
    #[spacetimedb]
    pub chunk_z: u32,
    #[spacetimedb(bytes)]
    data: Vec<u8>,
}

#[spacetimedb(init)]
pub fn init() {

}

#[spacetimedb(reducer)]
pub fn gen_world(actor: u64) {
    // TODO
}

#[spacetimedb(reducer)]
pub fn set_static_data(actor: u64, static_data: StaticData) {
    // TODO: for each static data table, drop all old data and insert new data
}

#[spacetimedb(reducer)]
pub fn player_move(actor: u64, direction: i32, running: bool, expected_origin: HexCoordinates) {
    let direction = HexDirection::from(direction).unwrap_or_fail("Invalid direction.");
   
    let player = PlayerTable::table().where(actor, Cmp::Equal)
    
    
    .unwrap_or_fail("No player for actor.");
    let location = Location::where_entity_id_eq(player.entity_id).unwrap();

    let new_coords = location.coordinates().neighbor(direction);

    for loc in select!(`from Location where x = {new_coords.x} AND z = {new_coords.z}`) {
       if let Some(resource) = Resource::where_entity_id_eq(loc.entity_id) {
           let walkable = ResourceDef::where_resource_id_eq(resource.resource_id).unwrap().walkable;
           if !walkable {
               fail!("Cannot walk on this resource.");
           }
       }
    }

    let new_location = Location {
        entity_id: location.entity_id,
        x: new_coords.x(),
        z: new_coords.z(),
    };
    Location::update(new_location);
}

#[spacetimedb(reducer)]
pub fn sign_in(actor: u64) {
    // TODO: What do we do about their location entity and so forth?
    if let Some(player) = select!(`from PlayerStateSignedIn where actor_id = {actor}`) {
        fail!("Player already signed in.");
    }
    let player = PlayerStateSignedOut::delete_where_actor_id_eq(actor).unwrap_or_fail("No such player.");
    PlayerStateSignedIn::insert(player);
}

#[spacetimedb(reducer)]
pub fn sign_out(actor: u64) {
    if let Some(player) = select!(`from PlayerStateSignedOut where actor_id = {actor}`) {
        fail!("Player already signed out.");
    }
    let player = PlayerStateSignedIn::delete_where_actor_id_eq(actor).unwrap_or_fail("No such player.");
    PlayerStateSignedOut::insert(player);
}

#[spacetimedb(reducer, actor=self, repeat=50ms)]
pub fn health_regen() {
    for player in PlayerStateSignedIn::all() {
        for mut health in Health::where_player_id(player.id) {
            health.health = f32::min(health.health + 1, max_health(player.id));
            Health::update(health);
        }
    }
}

#[spacetimedb(reducer, actor=self, repeat=50ms)]
pub fn physics(state: GameState) {
    // TODO
}

#[spacetimedb(reducer, actor=self, repeat=50ms)]
pub fn delegate() {

}


pub fn get_schema_for_concract() -> Vec::<TableSchema> {
   return vec![
        TableSchema {
            name: "User",
            id: 0,
        }
    ] 
}

#[spacetimedb(migration)]
pub fn migrate() {
    let old_db_address = spacetimedb::previous_address();

    for row in spacetimedb::iter_table(old_db_address, "PlayerStateSignedIn") {
        PlayerStateSignedIn::insert(PlayerState {
            username: row[0],
            // etc.
        });
    }
}

mod CoolBitCraftAlliance {
    pub fn apply_to_my_bitcraft_alliance(actor: u32) {
        // look up player in public table
        // evaluate if they are qualified

        // TODO: make this code gen somehow?
        // NOTE: This call is fire and forget (see Actor model)
        call("clockworklabs.bitcraft", add_alliance_member(u64), actor);
    }
}

/*
# Concepts

module = a set of tables and a set of reducers (approx a "contract")
database = a set of tables 
table = a sql table
reducer = a function that operates on a particular version of the state
function = a pure function that calculates something based on the state and returns
a value

## Updating a module
A module can be updated by uploading a new module with the name name/namespace as the previous one

All reducers with the same name will replace the old reducer of the same name (no overloading allowed)
All new reducers will be created
All old reducers will be hidden

All tables with the same name will do the following:
    - If the schema is unchanged the table is unchanged
    - If columns were removed or added, the old table will be hidden and a new one created 
    (hiding all old data. Possibly in the future we can figure out to do for additional nullable columns)
All new tables will be created
All old tables will be hidden
To rename a table you must hide the old one and create a new one

You can specify a special migration reducer which will run when a new module with the
same name as an existing module is uploaded. From this reducer you can access the old
version of the tables via the dynamic API. In this reducer you can pass the module version
of the table you want to access.

## Intermodular interactions
Any contract can use the dynamic API to access data from other modules (if the module is published,
maybe we can even have them import the types?). Tables can be marked as public or private. Public tables
can be read by anyone (more advanced read permissions to follow). Modules can call other modules via
their reducers (delegate calls to be evaluated later).

See also: https://arxiv.org/pdf/1702.05511.pdf
See also: https://medium.com/coinmonks/exploring-smart-contracts-as-actors-eee23499a59d
See also: https://github.com/apple/swift-evolution/blob/main/proposals/0306-actors.md
See also: https://github.com/apple/swift-evolution/blob/main/proposals/0306-actors.md#actor-reentrancy
See also: https://quantstamp.com/blog/what-is-a-re-entrancy-attack
See also: https://medium.com/spadebuilders/actor-factor-2b0005fde786

// This one is basically spacetimedb but with akka actors
See also: https://doc.akka.io/docs/akka/current/persistence.html#relaxed-local-consistency-requirements-and-high-throughput-use-cases
See also: https://github.com/dk14/2pc

// Partial ordering of transactions with a DAG
A tx -> tx         tx -> 
            \    /
AB            tx
            /    \
B tx -> tx         tx ->

vs

Pure actor message passing with no 

// Consider also the following reentrancy problem
// If the transaction could rollback we've got the following situation
Foo {
    // This tx will fail becuase db::save is called in commit on the same
    // variable. This caused bar to commit, but it actually gets rolledback
    // so it's as though bar happened, but this never did.
    // Much simpler to just be fire and forget.
    fn propose_commit() {
        let bar_says = Bar::should_commit().await?
        if bar_says {
            db::save(true)
        }
    }

    fn commit() {
        db::save(true);
    }
}

Bar {
    fn should_commit() -> bool {
        db::save(true)
        Foo::commit().await
        return true;
    }
}

e.g.
Module: BitCraft
Module: MyBitCraftAlliance

pub fn apply_to_my_bitcraft_alliance(actor: u32) {
    // look up player in public table
    // evaluate if they are qualified

    // TODO: make this code gen somehow?
    // NOTE: This call is fire and forget (see Actor model)
    call("clockworklabs.bitcraft", add_alliance_member(u64), actor);
}

TODO:
Handle strings
Handle structs
Handle contract parameters supplied from host
Impl reading from the db
Impl schema code-gen
Impl stdb as a server
Impl uploading new contract
*/
