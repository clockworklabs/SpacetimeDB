use spacetimedb::log;
use spacetimedb::Identity;
use spacetimedb::ReducerContext;
use spacetimedb::SpacetimeType;
use spacetimedb::Table;
use spacetimedb::Timestamp;
use std::fmt::Display;

#[macro_export]
macro_rules! unwrap_or_err(
    ($e:expr, $($str:tt)+) => (
        match $e {
            Some(v) => v,
            None => {
                spacetimedb::log::error!($($str)+);
                return Err(format!($($str)+))
            }
        }
    );
);

const FLOAT_COORD_PRECISION: u32 = 3;
const FLOAT_COORD_PRECISION_MUL: i32 = 10i32.pow(FLOAT_COORD_PRECISION);
const OUTER_RADIUS: f32 = TERRAIN_OUTER_RADIUS / 3.0;
const INNER_RADIUS: f32 = OUTER_RADIUS * RADIUS_RATIO;
const RADIUS_RATIO: f32 = 0.866025404;
const TERRAIN_OUTER_RADIUS: f32 = 10.0;
const TERRAIN_INNER_RADIUS: f32 = TERRAIN_OUTER_RADIUS * RADIUS_RATIO;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, spacetimedb::SpacetimeType)]
#[sats(name = "CharacterStatType")]
#[repr(i32)]
enum CharacterStatType {
    MaxHealth,
    MaxStamina,
    PassiveHealthRegenRate,
    PassiveStaminaRegenRate,
    MovementMultiplier,
    SprintMultiplier,
    SprintStaminaDrain,
    Armor,
    CooldownMultiplier,
    HuntingWeaponPower,
    Strength,
    ColdProtection,
    HeatProtection,
    Evasion,
    ToolbeltSlots,
    CraftingSpeed,
    GatheringSpeed,
    BuildingSpeed,
    SatiationRegenRate,
    MaxSatiation,
    DefenseLevel,
    //DAB Note: values below are temporary, see comment inside `SkillType` definition
    //Profession stats
    ForestrySpeed,
    CarpentrySpeed,
    MasonrySpeed,
    MiningSpeed,
    SmithingSpeed,
    ScholarSpeed,
    LeatherworkingSpeed,
    HuntingSpeed,
    TailoringSpeed,
    FarmingSpeed,
    FishingSpeed,
    CookingSpeed,
    ForagingSpeed,
    ForestryPower,
    CarpentryPower,
    MasonryPower,
    MiningPower,
    SmithingPower,
    ScholarPower,
    LeatherworkingPower,
    HuntingPower,
    TailoringPower,
    FarmingPower,
    FishingPower,
    CookingPower,
    ForagingPower,
    //Move these values up once the temporary values get removed
    ActiveHealthRegenRate,
    ActiveStaminaRegenRate,
    ClimbProficiency,
    ExperienceRate,
    Accuracy,
    MaxTeleportationEnergy,
    TeleportationEnergyRegenRate,
}

#[derive(SpacetimeType, Clone, Debug)]
struct ActiveBuff {
    pub buff_id: i32,
    pub buff_start_timestamp: OnlineTimestamp,
    pub buff_duration: i32,
    pub values: Vec<f32>,
}

#[derive(SpacetimeType, Clone)]
struct ExperienceStackF32 {
    pub skill_id: i32,
    pub quantity: f32,
}

#[derive(SpacetimeType, Clone, Debug)]
struct OnlineTimestamp {
    pub value: i32,
}

#[derive(SpacetimeType, Debug, Clone)]
struct CsvStatEntry {
    pub id: CharacterStatType,
    pub value: f32,
    pub is_pct: bool,
}

#[spacetimedb::table(name = character_stats_state, public)]
#[derive(Clone, Debug)]
pub struct CharacterStatsState {
    #[primary_key]
    pub entity_id: u64,

    pub values: Vec<f32>,
}

#[derive(Default, Clone, SpacetimeType, Debug)]
pub struct TeleportLocation {
    pub location: OffsetCoordinatesSmallMessage,
    pub location_type: TeleportLocationType,
}

#[spacetimedb::table(name = player_state, public)]
#[derive(Default, Clone, Debug)]
pub struct PlayerState {
    pub teleport_location: TeleportLocation,
    #[primary_key]
    pub entity_id: u64,
    pub time_played: i32,
    pub session_start_timestamp: i32,
    pub time_signed_in: i32,
    pub sign_in_timestamp: i32,
    pub signed_in: bool, // Keeping this attribute for optimization even if the value could be found by filtering SignedInPlayerState by entityId
    pub traveler_tasks_expiration: i32,
}

#[derive(spacetimedb::SpacetimeType, Clone, Copy, PartialEq, Debug)]
#[sats(name = "PlayerActionType")]
#[repr(i32)]
pub enum PlayerActionType {
    None,
    Attack,
    DestroyPaving,
    StationaryEmote,
    Extract,
    PaveTile,
    SpawnCargo,
    Build,
    Deconstruct,
    RepairBuilding,
    ResupplyClaim,
    CargoPickUp,
    Terraform,
    DeployDeployable,
    StoreDeployable,
    Sleep,
    Teleport,
    Death,
    Climb,
    UseItem,
    Craft,
    ConvertItems,
    PlayerMove,
    DeployableMove,
    ResupplyEmpireNode,
    SetHome,
    UseElevator,
    MobileEmote,
    PlacePillarShaping,
    DestroyPillarShaping,
}

impl PlayerActionType {
    pub fn get_layer(&self, ctx: &ReducerContext) -> PlayerActionLayer {
        match ctx.db.player_action_desc().action_type_id().find(&(*self as i32)) {
            Some(x) => x.layer,
            None => PlayerActionLayer::Base,
        }
    }
}

#[derive(spacetimedb::SpacetimeType, Clone, Copy, PartialEq, Debug)]
#[sats(name = "PlayerActionLayer")]
#[repr(i32)]
pub enum PlayerActionLayer {
    Base,
    UpperBody,
}

#[spacetimedb::table(name = player_action_desc, public)]
pub struct PlayerActionDesc {
    #[primary_key]
    pub action_type_id: i32,
    pub layer: PlayerActionLayer,
    pub allowed_concurrent_action_ids: Vec<i32>,
}

#[derive(SpacetimeType, Clone)]
struct PlayerMoveRequest {
    pub timestamp: u64,
    pub destination: Option<OffsetCoordinatesFloat>,
    pub origin: Option<OffsetCoordinatesFloat>,
    pub duration: f32,
    pub move_type: i32,
    pub running: bool,
}

#[spacetimedb::reducer]
fn player_move_timestamp(ctx: &ReducerContext, _request: PlayerMoveRequest) -> Result<(), String> {
    let actor_id = game_state::actor_id(&ctx, true)?;
    PlayerTimestampState::refresh(ctx, actor_id, ctx.timestamp);
    Ok(())
}

#[spacetimedb::reducer]
fn player_move_move(ctx: &ReducerContext, request: PlayerMoveRequest) -> Result<(), String> {
    let actor_id = game_state::actor_id(&ctx, true)?;

    let target_coordinates: FloatHexTile =
        unwrap_or_err!(request.destination, "Expected destination in move request").into();
    let source_coordinates: FloatHexTile = unwrap_or_err!(request.origin, "Expected origin in move request").into();

    let stamina_used = 0.0;

    PlayerState::move_player_and_explore(
        ctx,
        actor_id,
        &source_coordinates,
        &target_coordinates,
        stamina_used,
        request.running,
        Some(request.timestamp),
    )?;

    Ok(())
}

#[spacetimedb::table(name = player_action_state, public, index(name = entity_id, btree(columns = [entity_id])))]
#[derive(Clone, Debug)]
pub struct PlayerActionState {
    #[primary_key]
    #[auto_inc]
    pub auto_id: u64,
    pub entity_id: u64,
    pub action_type: PlayerActionType,
    pub layer: PlayerActionLayer,
    pub last_action_result: PlayerActionResult,
    pub start_time: u64,
    pub duration: u64,
    pub target: Option<u64>,
    pub recipe_id: Option<i32>,
    pub client_cancel: bool, // don't interrupt the actoin again on the client upon receiving this state change
}

impl PlayerActionState {
    pub fn success(
        ctx: &ReducerContext,
        entity_id: u64,
        action_type: PlayerActionType,
        layer: PlayerActionLayer,
        duration: u64,
        target: Option<u64>,
        recipe_id: Option<i32>,
    ) {
        if let Err(e) = PlayerActionState::update_by_entity_id(
            ctx,
            &entity_id,
            PlayerActionState {
                auto_id: 0,
                entity_id: entity_id,
                action_type: action_type,
                layer: layer,
                last_action_result: PlayerActionResult::Success,
                start_time: game_state::unix_ms(ctx.timestamp),
                duration,
                target,
                recipe_id,
                client_cancel: false,
            },
        ) {
            log::error!("Couldn't call success on PlayerActionState, with error: {}", e);
        }
    }

    pub fn get_state(ctx: &ReducerContext, entity_id: &u64, layer: &PlayerActionLayer) -> Option<PlayerActionState> {
        return ctx
            .db
            .player_action_state()
            .entity_id()
            .filter(entity_id)
            .find(|x| x.layer == *layer);
    }

    pub fn get_auto_id(ctx: &ReducerContext, entity_id: &u64, layer: &PlayerActionLayer) -> Option<u64> {
        return PlayerActionState::get_state(ctx, &entity_id, &layer).map(|state| state.auto_id);
    }

    pub fn update_by_entity_id(
        ctx: &ReducerContext,
        entity_id: &u64,
        mut state: PlayerActionState,
    ) -> Result<(), String> {
        let id: u64 = unwrap_or_err!(
            PlayerActionState::get_auto_id(ctx, &entity_id, &state.layer),
            "Can't find base layer state, invalid player id."
        );
        state.auto_id = id;
        ctx.db.player_action_state().auto_id().update(state);
        Ok(())
    }
}

#[derive(spacetimedb::SpacetimeType, Clone, Copy, PartialEq, Debug)]
#[sats(name = "PlayerActionResult")]
pub enum PlayerActionResult {
    Success,
    TimingFail,
    Fail,
    Cancel,
}

#[spacetimedb::reducer]
fn player_move_action(ctx: &ReducerContext, request: PlayerMoveRequest) -> Result<(), String> {
    let actor_id = game_state::actor_id(&ctx, true)?;

    let target_coordinates: FloatHexTile =
        unwrap_or_err!(request.destination, "Expected destination in move request").into();
    let source_coordinates: FloatHexTile = unwrap_or_err!(request.origin, "Expected origin in move request").into();

    PlayerActionState::success(
        ctx,
        actor_id,
        if source_coordinates == target_coordinates {
            PlayerActionType::None
        } else {
            PlayerActionType::PlayerMove
        },
        PlayerActionType::PlayerMove.get_layer(ctx),
        (request.duration * 1000.0) as u64,
        None,
        None,
    );

    Ok(())
}

impl PlayerState {
    pub fn move_player_and_explore(
        ctx: &ReducerContext,
        entity_id: u64,
        start_coordinates: &FloatHexTile,
        target_coordinates: &FloatHexTile,
        _stamina_delta: f32,
        is_running: bool,
        timestamp: Option<u64>,
    ) -> Result<(), String> {
        let start_large = start_coordinates.parent_large_tile();
        let target_large = target_coordinates.parent_large_tile();
        // Technically Chunks are not the same as ExploredChunks but whatever
        let previous_chunk = ChunkCoordinates::from(start_large);
        let entered_chunk = ChunkCoordinates::from(target_large);

        let dimension_desc_start = ctx
            .db
            .dimension_description_state()
            .dimension_id()
            .find(&start_coordinates.dimension)
            .unwrap();
        let dimension_desc_target = if start_coordinates.dimension == target_coordinates.dimension {
            dimension_desc_start.clone()
        } else {
            ctx.db
                .dimension_description_state()
                .dimension_id()
                .find(&target_coordinates.dimension)
                .unwrap()
        };

        //DAB Note: temp hack to identify what's causing players to move out of bounds
        if (previous_chunk.x < dimension_desc_start.dimension_position_large_x as i32)
            | (previous_chunk.z < dimension_desc_start.dimension_position_large_z as i32)
            | (previous_chunk.x
                >= dimension_desc_start.dimension_position_large_x as i32
                    + dimension_desc_start.dimension_size_large_x as i32)
            | (previous_chunk.z
                >= dimension_desc_start.dimension_position_large_z as i32
                    + dimension_desc_start.dimension_size_large_z as i32)
        {
            return Err(format!(
                "Move origin outside of world bounds! Origin: ({} {})",
                start_coordinates.x, start_coordinates.z
            ));
        }
        if (entered_chunk.x < dimension_desc_target.dimension_position_large_x as i32)
            | (entered_chunk.z < dimension_desc_target.dimension_position_large_z as i32)
            | (entered_chunk.x
                >= dimension_desc_target.dimension_position_large_x as i32
                    + dimension_desc_target.dimension_size_large_x as i32)
            | (entered_chunk.z
                >= dimension_desc_target.dimension_position_large_z as i32
                    + dimension_desc_target.dimension_size_large_z as i32)
        {
            return Err(format!(
                "Move origin target of world bounds! Target: ({} {})",
                target_coordinates.x, target_coordinates.z
            ));
        }

        // update location
        let start_offset_coordinates = OffsetCoordinatesFloat::from(start_coordinates);
        let target_offset_coordinates = OffsetCoordinatesFloat::from(target_coordinates);
        let mobile_entity = MobileEntityState {
            entity_id,
            // IMPORTANT: currently having negative or zero coordinates in here causes weird issues.
            // One known one is that we can't add negative numbers in our subscription queries.
            // Being at exactly 0,0 may cause some floating point conversion issue or something not sure.
            chunk_index: FloatHexTile::from(OffsetCoordinatesFloat {
                x: start_offset_coordinates.x.clamp(1, i32::MAX),
                z: start_offset_coordinates.z.clamp(1, i32::MAX),
                dimension: target_offset_coordinates.dimension,
            })
            .chunk_coordinates()
            .chunk_index(),
            timestamp: timestamp.unwrap_or_else(|| game_state::unix_ms(ctx.timestamp)),
            location_x: start_offset_coordinates.x.clamp(1, i32::MAX),
            location_z: start_offset_coordinates.z.clamp(1, i32::MAX),
            destination_x: target_offset_coordinates.x.clamp(1, i32::MAX),
            destination_z: target_offset_coordinates.z.clamp(1, i32::MAX),
            dimension: target_offset_coordinates.dimension,
            is_running,
        };

        ctx.db.mobile_entity_state().entity_id().update(mobile_entity);

        Ok(())
    }
}

#[spacetimedb::table(name = stamina_state, public)]
#[derive(Clone, Debug)]
pub struct StaminaState {
    // Sort fields in order of decreasing size/alignment
    // to take advantage of a serialization fast-path in SpacetimeDB.
    #[primary_key]
    pub entity_id: u64,

    pub last_stamina_decrease_timestamp: Timestamp,
    pub stamina: f32,
}

#[spacetimedb::table(name = mobile_entity_state, public,
    index(name = chunk_index, btree(columns = [chunk_index])))]
#[derive(Clone, Debug)]
pub struct MobileEntityState {
    // Sort fields in order of decreasing size/alignment
    // to take advantage of a serialization fast-path in SpacetimeDB.
    #[primary_key]
    pub entity_id: u64,
    pub chunk_index: u64,
    pub timestamp: u64,
    pub location_x: i32,
    pub location_z: i32,
    pub destination_x: i32,
    pub destination_z: i32,
    pub dimension: u32,
    pub is_running: bool,
}

#[derive(Default, Clone, PartialEq)]
struct HexCoordinates {
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

impl HexCoordinates {
    pub fn from_position(position: Vector2, terrain: bool, dimension: u32) -> HexCoordinates {
        //Equivalent to HexCoordinates.FromPosition on client

        let inner_radius = if terrain { TERRAIN_INNER_RADIUS } else { INNER_RADIUS };
        let outer_radius = if terrain { TERRAIN_OUTER_RADIUS } else { OUTER_RADIUS };

        let mut x = position.x / (inner_radius * 2.0);
        let mut y = -x;
        let offset = position.y / (outer_radius * 3.0);
        x -= offset;
        y -= offset;
        let mut ix = x.round() as i32;
        let iy = y.round() as i32;
        let mut iz = (-x - y).round() as i32;

        if ix + iy + iz != 0 {
            let dx = (x - ix as f32).abs();
            let dy = (y - iy as f32).abs();
            let dz = (-x - y - iz as f32).abs();

            if dx > dy && dx > dz {
                ix = -iy - iz;
            } else if dz > dy {
                iz = -ix - iy;
            }
        }

        return HexCoordinates {
            x: ix,
            z: iz,
            dimension,
        };
    }
}

type LargeHexTile = LargeHexTileMessage;

impl LargeHexTile {
    pub fn chunk_coordinates(&self) -> ChunkCoordinates {
        return ChunkCoordinates::from(self);
    }

    pub fn from_position(position: Vector2, dimension: u32) -> LargeHexTile {
        return LargeHexTile::from(HexCoordinates::from_position(position, true, dimension));
    }
}

impl From<HexCoordinates> for LargeHexTile {
    fn from(coordinates: HexCoordinates) -> Self {
        return LargeHexTile {
            x: coordinates.x,
            z: coordinates.z,
            dimension: coordinates.dimension,
        };
    }
}

impl From<&OffsetCoordinatesLarge> for ChunkCoordinates {
    fn from(offset: &OffsetCoordinatesLarge) -> Self {
        ChunkCoordinates {
            x: offset.x / TerrainChunkState::WIDTH as i32,
            z: offset.z / TerrainChunkState::HEIGHT as i32,
            dimension: offset.dimension,
        }
    }
}

impl From<LargeHexTile> for OffsetCoordinatesLarge {
    fn from(coordinates: LargeHexTile) -> Self {
        Self {
            x: coordinates.x + coordinates.z / 2,
            z: coordinates.z,
            dimension: coordinates.dimension,
        }
    }
}

impl From<&LargeHexTile> for OffsetCoordinatesLarge {
    fn from(coordinates: &LargeHexTile) -> Self {
        Self {
            x: coordinates.x + coordinates.z / 2,
            z: coordinates.z,
            dimension: coordinates.dimension,
        }
    }
}

impl From<OffsetCoordinatesLarge> for ChunkCoordinates {
    fn from(offset: OffsetCoordinatesLarge) -> Self {
        ChunkCoordinates::from(&offset)
    }
}

impl From<LargeHexTile> for ChunkCoordinates {
    fn from(coordinates: LargeHexTile) -> Self {
        return ChunkCoordinates::from(OffsetCoordinatesLarge::from(coordinates));
    }
}

impl From<&LargeHexTile> for ChunkCoordinates {
    fn from(coordinates: &LargeHexTile) -> Self {
        return ChunkCoordinates::from(OffsetCoordinatesLarge::from(coordinates));
    }
}

impl From<&FloatHexTile> for ChunkCoordinates {
    fn from(coordinates: &FloatHexTile) -> Self {
        return LargeHexTile::from(coordinates).chunk_coordinates();
    }
}

impl From<&FloatHexTile> for LargeHexTile {
    fn from(coordinates: &FloatHexTile) -> Self {
        return LargeHexTile::from_position(coordinates.to_world_position(), coordinates.dimension);
    }
}

impl From<OffsetCoordinatesFloat> for FloatHexTile {
    fn from(offset: OffsetCoordinatesFloat) -> Self {
        Self {
            x: offset.x - offset.z / 2,
            z: offset.z,
            dimension: offset.dimension,
        }
    }
}

impl From<&FloatHexTile> for OffsetCoordinatesFloat {
    fn from(coordinates: &FloatHexTile) -> Self {
        Self {
            x: coordinates.x + coordinates.z / 2,
            z: coordinates.z,
            dimension: coordinates.dimension,
        }
    }
}

impl FloatHexTile {
    pub fn chunk_coordinates(&self) -> ChunkCoordinates {
        return ChunkCoordinates::from(self);
    }

    pub fn parent_large_tile(&self) -> LargeHexTile {
        return LargeHexTile::from(self);
    }

    pub fn to_world_position(&self) -> Vector2 {
        //Equivalent to FloatHexTile.ToCenterPositionVector2 on client

        let ix = (self.x as f32) / FLOAT_COORD_PRECISION_MUL as f32;
        let iz = (self.z as f32) / FLOAT_COORD_PRECISION_MUL as f32;

        let x = 2.0 * ix * INNER_RADIUS + iz * INNER_RADIUS;
        let z = 1.5 * iz * OUTER_RADIUS;

        return Vector2 { x, y: z };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, spacetimedb::SpacetimeType)]
#[sats(name = "TeleportLocationType")]
#[repr(i32)]
pub enum TeleportLocationType {
    BirthLocation,
    TradingPost,
    HomeLocation,
    CustomLocation,
    Waystone,
}

impl Default for TeleportLocationType {
    fn default() -> Self {
        TeleportLocationType::BirthLocation
    }
}

#[spacetimedb::table(name = terrain_chunk_state, public, index(name = dimension, btree(columns = [dimension])))]
#[derive(Default, Clone)]
pub struct TerrainChunkState {
    #[primary_key]
    // chunk_index = (dimension-1)*1000000 + chunk_z * 1000 + chunk_x + 1
    pub chunk_index: u64,

    pub chunk_x: i32,
    pub chunk_z: i32,
    pub dimension: u32,

    pub biomes: Vec<u32>,        // bitfield
    pub biome_density: Vec<u32>, // bitfield
    pub elevations: Vec<i16>,
    pub water_levels: Vec<i16>,
    pub water_body_types: Vec<u8>,
    pub zoning_types: Vec<u8>,
    pub original_elevations: Vec<i16>,
}

impl TerrainChunkState {
    pub const WIDTH: u32 = 32;
    pub const HEIGHT: u32 = 32;
}

#[spacetimedb::table(name = dimension_description_state, public, index(name = dimension_network_entity_id, btree(columns = [dimension_network_entity_id])))]
#[derive(Default, Clone, Debug)]
pub struct DimensionDescriptionState {
    // Sort fields in order of decreasing size/alignment
    // to take advantage of a serialization fast-path in SpacetimeDB.
    // Note that C-style enums count as size/align of 1,
    // regardless of declared `repr` in Rust.
    #[primary_key]
    pub entity_id: u64,
    pub dimension_network_entity_id: u64,
    pub collapse_timestamp: u64,
    pub interior_instance_id: i32,
    pub dimension_position_large_x: u32, //In large tiles
    pub dimension_position_large_z: u32, //In large tiles
    pub dimension_size_large_x: u32,     //In large tiles
    pub dimension_size_large_z: u32,     //In large tiles

    #[unique]
    pub dimension_id: u32,
    pub dimension_type: DimensionType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, spacetimedb::SpacetimeType)]
#[repr(i32)]
pub enum DimensionType {
    Unknown,
    Overworld,
    AncientRuin,
    BuildingInterior,
}

impl Default for DimensionType {
    fn default() -> Self {
        DimensionType::Unknown
    }
}

#[derive(SpacetimeType, Default, Copy, Clone, Debug, PartialEq, Eq)]
pub struct FloatHexTileMessage {
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

type FloatHexTile = FloatHexTileMessage;

impl Display for FloatHexTile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let oc = OffsetCoordinatesFloat::from(self);
        write!(
            f,
            "FloatHexTile ({}.{:.03}, {}.{:.03}, {})",
            oc.x / FLOAT_COORD_PRECISION_MUL,
            oc.x % FLOAT_COORD_PRECISION_MUL,
            oc.z / FLOAT_COORD_PRECISION_MUL,
            oc.z % FLOAT_COORD_PRECISION_MUL,
            oc.dimension
        )
    }
}

impl From<FloatHexTile> for OffsetCoordinatesFloat {
    fn from(coordinates: FloatHexTile) -> Self {
        Self {
            x: coordinates.x + coordinates.z / 2,
            z: coordinates.z,
            dimension: coordinates.dimension,
        }
    }
}

#[derive(SpacetimeType, Default, Copy, Clone, Debug, PartialEq, Eq)]
struct SmallHexTileMessage {
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

#[derive(SpacetimeType, Default, Copy, Clone, Debug, PartialEq, Eq)]
pub struct LargeHexTileMessage {
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

#[derive(SpacetimeType, Default, Copy, Clone, Debug, PartialEq, Eq)]
struct OffsetCoordinatesFloat {
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

#[derive(SpacetimeType, Default, Copy, Clone, Debug, PartialEq, Eq)]
pub struct OffsetCoordinatesSmallMessage {
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

#[derive(SpacetimeType, Default, Copy, Clone, Debug, PartialEq, Eq)]
struct OffsetCoordinatesLargeMessage {
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

type OffsetCoordinatesLarge = OffsetCoordinatesLargeMessage;

#[derive(SpacetimeType, PartialEq, Eq, Clone, Copy, Debug)]
pub struct ChunkCoordinatesMessage {
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

type ChunkCoordinates = ChunkCoordinatesMessage;

impl ChunkCoordinates {
    pub fn chunk_index(self) -> u64 {
        (self.dimension as u64 - 1) * 1000000 + self.z as u64 * 1000 + self.x as u64 + 1
        // 1000 is over the maximum chunk size and will skip a table access at runtime
    }
}

#[spacetimedb::table(name = player_timestamp_state)]
#[derive(Clone)]
pub struct PlayerTimestampState {
    #[primary_key]
    pub entity_id: u64,
    pub timestamp: Timestamp,
}

impl PlayerTimestampState {
    pub fn refresh(ctx: &ReducerContext, actor_id: u64, timestamp: Timestamp) {
        if let Some(mut entry) = ctx.db.player_timestamp_state().entity_id().find(&actor_id) {
            entry.timestamp = timestamp;
            ctx.db.player_timestamp_state().entity_id().update(entry);
        } else {
            let _ = ctx.db.player_timestamp_state().try_insert(PlayerTimestampState {
                entity_id: actor_id,
                timestamp,
            });
        }
    }
}

#[spacetimedb::table(name = signed_in_player_state, public)]
#[derive(Clone, Debug)]
pub struct SignedInPlayerState {
    #[primary_key]
    pub entity_id: u64,
}

#[spacetimedb::table(name = user_state, public)]
#[derive(Clone, Debug)]
pub struct UserState {
    #[unique]
    pub identity: Identity,
    #[primary_key]
    pub entity_id: u64,
    pub can_sign_in: bool,
}

mod game_state {
    use super::signed_in_player_state;
    use super::user_state;
    use spacetimedb::ReducerContext;
    use spacetimedb::Timestamp;

    pub fn unix_ms(now: Timestamp) -> u64 {
        return now.duration_since(Timestamp::UNIX_EPOCH).unwrap().as_millis() as u64;
    }

    pub fn ensure_signed_in(ctx: &ReducerContext, entity_id: u64) -> Result<(), String> {
        if ctx.db.signed_in_player_state().entity_id().find(&entity_id).is_none() {
            return Err("Not signed in".into());
        }
        return Ok(());
    }

    pub fn actor_id(ctx: &ReducerContext, must_be_signed_in: bool) -> Result<u64, String> {
        match ctx.db.user_state().identity().find(&ctx.sender) {
            Some(user) => {
                if must_be_signed_in {
                    ensure_signed_in(ctx, user.entity_id)?;
                }
                Ok(user.entity_id)
            }
            None => Err("Invalid sender".into()),
        }
    }
}

#[derive(SpacetimeType, Default, Debug, Clone)]
pub struct WorldGenVector2 {
    pub x: f32,
    pub y: f32,
}

type Vector2 = WorldGenVector2;

impl std::ops::Add<&Vector2> for Vector2 {
    type Output = Vector2;

    fn add(self, other: &Vector2) -> Vector2 {
        Vector2 {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl std::ops::Add<Vector2> for Vector2 {
    type Output = Vector2;

    fn add(self, other: Vector2) -> Vector2 {
        Vector2 {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl std::ops::Sub<&Vector2> for Vector2 {
    type Output = Vector2;

    fn sub(self, other: &Vector2) -> Vector2 {
        Vector2 {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl std::ops::Sub<Vector2> for Vector2 {
    type Output = Vector2;

    fn sub(self, other: Vector2) -> Vector2 {
        Vector2 {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl std::ops::Mul<f32> for Vector2 {
    type Output = Vector2;

    fn mul(self, other: f32) -> Vector2 {
        Vector2 {
            x: self.x * other,
            y: self.y * other,
        }
    }
}

impl std::ops::Div<f32> for Vector2 {
    type Output = Vector2;

    fn div(self, other: f32) -> Vector2 {
        Vector2 {
            x: self.x / other,
            y: self.y / other,
        }
    }
}

impl std::marker::Copy for Vector2 {}
