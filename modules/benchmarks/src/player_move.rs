use spacetimedb::log;
use spacetimedb::ReducerContext;
use spacetimedb::SpacetimeType;
use spacetimedb::Timestamp;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, spacetimedb::SpacetimeType)]
#[sats(name = "CharacterStatType")]
#[repr(i32)]
pub enum CharacterStatType {
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
pub struct ActiveBuff {
    pub buff_id: i32,
    pub buff_start_timestamp: OnlineTimestamp,
    pub buff_duration: i32,
    pub values: Vec<f32>,
}

#[derive(SpacetimeType, Clone)]
pub struct ExperienceStackF32 {
    pub skill_id: i32,
    pub quantity: f32,
}

#[derive(SpacetimeType, Clone, Debug)]
pub struct InputItemStack {
    pub item_id: i32,
    pub quantity: i32,
    pub item_type: ItemType,
    pub discovery_score: i32,
    pub consumption_chance: f32,
}

#[derive(SpacetimeType, Clone, Debug)]
pub struct OnlineTimestamp {
    pub value: i32,
}

#[derive(SpacetimeType, Debug, Clone)]
pub struct CsvStatEntry {
    pub id: CharacterStatType,
    pub value: f32,
    pub is_pct: bool,
}

#[spacetimedb::table(name = paved_tile_state, public)]
#[derive(bitcraft_macro::Operations, Clone)]
#[operations(delete)]
pub struct PavedTileState {
    // Sort fields in order of decreasing size/alignment
    // to take advantage of a serialization fast-path in SpacetimeDB.
    #[primary_key]
    pub entity_id: u64,

    pub tile_type_id: i32,
    pub related_entity_id: u64, // optional : when this related entitiy is deleted, delete this paving instance as well
}

#[spacetimedb::table(name = mounting_state, public, index(name = deployable_entity_id, btree(columns = [deployable_entity_id])))]
#[derive(bitcraft_macro::Operations, Clone)]
#[operations(delete)]
pub struct MountingState {
    // Sort fields in order of decreasing size/alignment
    // to take advantage of a serialization fast-path in SpacetimeDB.
    #[primary_key]
    pub entity_id: u64,

    pub deployable_entity_id: u64,
    pub deployable_slot: i32,
}

#[spacetimedb::table(name = active_buff_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete)]
pub struct ActiveBuffState {
    #[primary_key]
    pub entity_id: u64,

    pub active_buffs: Vec<ActiveBuff>,
}

#[spacetimedb::table(name = claim_state, public,
    index(name = owner_player_entity_id, btree(columns = [owner_player_entity_id])),
    index(name = name, btree(columns = [name])),
    index(name = neutral, btree(columns = [neutral])))]
#[derive(bitcraft_macro::Operations, Clone, Debug)]
#[shared_table] //Owned by region, replicated to global module
#[operations(delete)]
pub struct ClaimState {
    #[primary_key]
    pub entity_id: u64,
    pub owner_player_entity_id: u64,
    #[unique]
    pub owner_building_entity_id: u64,
    pub name: String,
    pub neutral: bool,
}

// Keep all players affected by long term rez sickness to improve player_move
#[spacetimedb::table(name = rez_sick_long_term_state)]
#[derive(Clone, bitcraft_macro::Operations)]
#[operations(delete)]
pub struct RezSickLongTermState {
    #[primary_key]
    pub entity_id: u64,
}

#[spacetimedb::table(name = exploration_chunks_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete)]
pub struct ExplorationChunksState {
    #[primary_key]
    pub entity_id: u64,

    pub bitmap: Vec<u64>, //Essentially a bitfield. Index=(Z*W+X)/64, bit=(Z*W+X)%64
    pub explored_chunks_count: i32,
}

#[spacetimedb::table(name = character_stats_state, public)]
#[derive(bitcraft_macro::Operations, Clone, Debug)]
#[operations(delete)]
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
#[derive(Default, Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete)]
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

#[spacetimedb::table(name = paving_tile_desc, public)]
pub struct PavingTileDesc {
    #[primary_key]
    pub id: i32,
    pub name: String,
    pub consumed_item_stacks: Vec<InputItemStack>,
    pub input_cargo_id: i32,
    pub input_cargo_discovery_score: i32,
    pub experience_per_progress: Vec<ExperienceStackF32>,
    pub discovery_triggers: Vec<i32>,
    pub required_knowledges: Vec<i32>,
    pub full_discovery_score: i32,
    pub paving_duration: f32,
    pub prefab_address: String,
    pub tier: i32,
    pub stat_effects: Vec<CsvStatEntry>,
    pub icon_address: String,
    pub description: String,
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
pub struct PlayerMoveRequest {
    pub timestamp: u64,
    pub destination: Option<OffsetCoordinatesFloat>,
    pub origin: Option<OffsetCoordinatesFloat>,
    pub duration: f32,
    pub move_type: i32,
    pub running: bool,
}

#[derive(spacetimedb::SpacetimeType, Clone, Copy, PartialEq, Debug)]
#[repr(i32)]
// IMPORTANT: These are sorted in order of access level, from least to most access.
pub enum Role {
    Player,
    Mod,
    Gm,
    Admin,
    Relay,
}

#[spacetimedb::table(name = building_state, public,
    index(name = claim_entity_id, btree(columns = [claim_entity_id])),
    index(name = building_description_id, btree(columns = [building_description_id])))]
#[shared_table]
#[derive(Default, Clone, bitcraft_macro::Operations, Debug)]
pub struct BuildingState {
    #[primary_key]
    pub entity_id: u64,

    pub claim_entity_id: u64,
    pub direction_index: i32,
    pub building_description_id: i32,
    pub constructed_by_player_entity_id: u64,
}

#[spacetimedb::reducer]
pub fn player_move_timestamp(ctx: &ReducerContext, mut request: PlayerMoveRequest) -> Result<(), String> {
    let actor_id = game_state::actor_id(&ctx, true)?;
    PlayerTimestampState::refresh(ctx, actor_id, ctx.timestamp);
}

#[spacetimedb::reducer]
pub fn player_move_move(ctx: &ReducerContext, mut request: PlayerMoveRequest) -> Result<(), String> {
    let actor_id = game_state::actor_id(&ctx, true)?;

    if request.running && InventoryState::get_player_cargo_id(ctx, actor_id) > 0 {
        return Err("Can't run with cargo.".into());
    }

    let mut prev_mobile_entity = ctx.db.mobile_entity_state().entity_id().find(&actor_id).unwrap();

    let prev_origin = prev_mobile_entity.coordinates_float();
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

#[spacetimedb::reducer]
pub fn player_move_action(ctx: &ReducerContext, mut request: PlayerMoveRequest) -> Result<(), String> {
    let actor_id = game_state::actor_id(&ctx, true)?;

    if request.running && InventoryState::get_player_cargo_id(ctx, actor_id) > 0 {
        return Err("Can't run with cargo.".into());
    }

    let mut prev_mobile_entity = ctx.db.mobile_entity_state().entity_id().find(&actor_id).unwrap();

    let prev_origin = prev_mobile_entity.coordinates_float();
    let target_coordinates: FloatHexTile =
        unwrap_or_err!(request.destination, "Expected destination in move request").into();
    let source_coordinates: FloatHexTile = unwrap_or_err!(request.origin, "Expected origin in move request").into();

    let stamina_used = 0.0;

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

#[spacetimedb::table(name = building_desc, public)]
pub struct BuildingDesc {
    #[primary_key]
    pub id: i32,
    pub functions: Vec<BuildingFunction>,
    pub name: String,
    pub description: String,
    pub rested_buff_duration: i32,
    pub light_radius: i32,
    pub model_asset_name: String,
    pub icon_asset_name: String,
    pub unenterable: bool,
    pub wilderness: bool,
    pub footprint: Vec<FootprintTile>,
    pub max_health: i32,
    pub ignore_damage: bool,
    pub defense_level: i32,
    pub decay: f32,
    pub maintenance: f32,
    pub build_permission: BuildingInteractionLevel,
    pub interact_permission: BuildingInteractionLevel,
    pub has_action: bool,
    pub show_in_compendium: bool,
    pub is_ruins: bool,
    pub not_deconstructible: bool,
}

#[spacetimedb::table(name = parameters_player_move_desc)]
#[derive(Default)]
pub struct ParametersPlayerMoveDesc {
    #[primary_key]
    pub version: i32,
    pub default_speed: Vec<MovementSpeed>,
}

impl PlayerState {
    pub fn move_player_and_explore(
        ctx: &ReducerContext,
        entity_id: u64,
        start_coordinates: &FloatHexTile,
        target_coordinates: &FloatHexTile,
        stamina_delta: f32,
        is_running: bool,
        timestamp: Option<u64>,
    ) -> Result<(), String> {
        let start_large = start_coordinates.parent_large_tile();
        let target_large = target_coordinates.parent_large_tile();
        // Technically Chunks are not the same as ExploredChunks but whatever
        let previous_chunk = ChunkCoordinates::from(start_large);
        let entered_chunk = ChunkCoordinates::from(target_large);

        // Don't explore non-overworld dimensions
        let in_overworld = target_coordinates.dimension == dimensions::OVERWORLD;
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
            timestamp: timestamp.unwrap_or_else(|| unix_ms(ctx.timestamp)),
            location_x: start_offset_coordinates.x.clamp(1, i32::MAX),
            location_z: start_offset_coordinates.z.clamp(1, i32::MAX),
            destination_x: target_offset_coordinates.x.clamp(1, i32::MAX),
            destination_z: target_offset_coordinates.z.clamp(1, i32::MAX),
            dimension: target_offset_coordinates.dimension,
            is_running,
        };

        ctx.db.mobile_entity_state().entity_id().update(mobile_entity);

        StaminaState::add_player_stamina(ctx, entity_id, stamina_delta);

        Ok(())
    }
}

#[derive(spacetimedb::SpacetimeType, Clone, Copy, Debug, Default, PartialEq, Eq, EnumIter, PartialOrd, Ord, Hash)]
#[sats(name = "SurfaceType")]
#[repr(u8)]
pub enum SurfaceType {
    #[default]
    Ground,
    Lake,
    River,
    Ocean,
    OceanBiome,
    Swamp,
}

#[spacetimedb::table(name = move_validation_strike_counter_state)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete)]
pub struct MoveValidationStrikeCounterState {
    #[primary_key]
    pub entity_id: u64,
    pub validation_failure_timestamps: Vec<Timestamp>,
}

pub struct Knowledges {
    pub knowledge_achievement: Vec<KnowledgeEntry>,
    pub knowledge_achievement_hash: i32,
    pub knowledge_battle_action: Vec<KnowledgeEntry>,
    pub knowledge_battle_action_hash: i32,
    pub knowledge_building: Vec<KnowledgeEntry>,
    pub knowledge_building_hash: i32,
    pub knowledge_cargo: Vec<KnowledgeEntry>,
    pub knowledge_cargo_hash: i32,
    pub knowledge_claim: Vec<KnowledgeEntityEntry>,
    pub knowledge_claim_hash: i32,
    pub knowledge_construction: Vec<KnowledgeEntry>,
    pub knowledge_construction_hash: i32,
    pub knowledge_craft: Vec<KnowledgeEntry>,
    pub knowledge_craft_hash: i32,
    pub knowledge_deployable: Vec<KnowledgeEntry>,
    pub knowledge_deployable_hash: i32,
    pub knowledge_enemy: Vec<KnowledgeEntry>,
    pub knowledge_enemy_hash: i32,
    pub knowledge_extract: Vec<KnowledgeEntry>,
    pub knowledge_extract_hash: i32,
    pub knowledge_item: Vec<KnowledgeEntry>,
    pub knowledge_item_hash: i32,
    pub knowledge_lore: Vec<KnowledgeEntry>,
    pub knowledge_lore_hash: i32,
    pub knowledge_npc: Vec<KnowledgeEntry>,
    pub knowledge_npc_hash: i32,
    pub knowledge_paving: Vec<KnowledgeEntry>,
    pub knowledge_paving_hash: i32,
    pub knowledge_pillar_shaping: Vec<KnowledgeEntry>,
    pub knowledge_pillar_shaping_hash: i32,
    pub knowledge_resource_placement: Vec<KnowledgeEntry>,
    pub knowledge_resource_placement_hash: i32,
    pub knowledge_resource: Vec<KnowledgeEntry>,
    pub knowledge_resource_hash: i32,
    pub knowledge_ruins: Vec<KnowledgeLocationEntry>,
    pub knowledge_ruins_hash: i32,
    pub knowledge_secondary: Vec<KnowledgeEntry>,
    pub knowledge_secondary_hash: i32,
    pub knowledge_vault: Vec<KnowledgeEntry>,
    pub knowledge_vault_hash: i32,
}

#[derive(SpacetimeType, Clone, Debug)]
pub struct KnowledgeEntry {
    pub id: i32,
    pub state: KnowledgeState,
}

#[derive(SpacetimeType, Clone, Debug)]
pub struct KnowledgeEntityEntry {
    pub entity_id: u64,
    pub state: KnowledgeState,
}

#[derive(SpacetimeType, Clone, Debug)]
pub struct KnowledgeLocationEntry {
    pub location: OffsetCoordinatesSmallMessage,
    pub state: KnowledgeState,
}

#[spacetimedb::table(name = knowledge_achievement_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge)]
pub struct KnowledgeAchievementState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_battle_action_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge)]
pub struct KnowledgeBattleActionState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_building_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge)]
pub struct KnowledgeBuildingState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_cargo_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge_on_acquire_callback, achievement)]
pub struct KnowledgeCargoState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_construction_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge_recipe)]
pub struct KnowledgeConstructionState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_resource_placement_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge_recipe)]
pub struct KnowledgeResourcePlacementState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_craft_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge_recipe, achievement)]
pub struct KnowledgeCraftState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_enemy_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge)]
pub struct KnowledgeEnemyState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_extract_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge_recipe)]
pub struct KnowledgeExtractState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_item_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge_on_acquire_callback, achievement)]
pub struct KnowledgeItemState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_lore_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge)]
pub struct KnowledgeLoreState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_npc_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge)]
pub struct KnowledgeNpcState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_resource_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge, achievement)]
pub struct KnowledgeResourceState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_ruins_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge_location)]
pub struct KnowledgeRuinsState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeLocationEntry>,
}

#[spacetimedb::table(name = knowledge_claim_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge_entity)]
pub struct KnowledgeClaimState {
    #[primary_key]
    pub entity_id: u64,
    pub entries: Vec<KnowledgeEntityEntry>,
}

#[spacetimedb::table(name = knowledge_secondary_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge)]
pub struct KnowledgeSecondaryState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_vault_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge)]
pub struct KnowledgeVaultState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_deployable_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge)]
pub struct KnowledgeDeployableState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_pillar_shaping_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge_recipe)]
pub struct KnowledgePillarShapingState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[spacetimedb::table(name = knowledge_paving_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete, knowledge_recipe)]
pub struct KnowledgePavingState {
    #[primary_key]
    pub entity_id: u64,

    pub entries: Vec<KnowledgeEntry>,
}

#[derive(spacetimedb::SpacetimeType, Clone, Copy, PartialEq, Debug)]
#[sats(name = "KnowledgeState")]
#[repr(i32)]
pub enum KnowledgeState {
    Unknown,
    Discovered,
    Acquired,
}

#[spacetimedb::table(name = stamina_state, public)]
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete)]
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
#[derive(Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete)] // IMPORTANT: MOBILE_ENTITIES SHOULD NOT HAVE THE COMMIT ATTRIBUTE
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

impl MobileEntityState {
    pub fn coordinates_float(&self) -> FloatHexTile {
        FloatHexTile::from(self.offset_coordinates_float())
    }

    pub fn offset_coordinates_float(&self) -> OffsetCoordinatesFloat {
        return OffsetCoordinatesFloat {
            x: self.location_x,
            z: self.location_z,
            dimension: self.dimension,
        };
    }

    //pub fn cur_coord(&self, speed: f32) -> FloatHexTile {
    //    let origin = self.coordinates_float();
    //    let distination: crate::messages::util::FloatHexTileMessage = self.destination_float();
    //    if origin.x == distination.x && origin.z == distination.z {
    //        return origin.clone();
    //    } else {
    //        let travel_time = travel_time(&origin, &distination, speed);
    //        let time_diff = ((game_state::unix_ms() - self.timestamp) as f32 / 1000.0).clamp(0.0, travel_time);
    //        let t = time_diff / travel_time;
    //        return FloatHexTile::lerp(&origin, &distination, t);
    //    };
    //}
    //
    //pub fn cur_distance_traveled(&self, speed: f32) -> f32 {
    //    let origin = self.coordinates_float();
    //    let distination = self.destination_float();
    //    if origin.x == distination.x && origin.z == distination.z {
    //        return 0.0;
    //    } else {
    //        let travel_time = move_validation_helpers::travel_time(&origin, &distination, speed);
    //        let travel_time = travel_time;
    //        let time_diff = ((game_state::unix_ms() - self.timestamp) as f32 / 1000.0).clamp(0.0, travel_time);
    //        return speed * time_diff / travel_time;
    //    };
    //}
    //
    //pub fn cur_coord_and_distance_traveled(&self, speed: f32) -> (FloatHexTile, f32) {
    //    let origin = self.coordinates_float();
    //    let distination: crate::messages::util::FloatHexTileMessage = self.destination_float();
    //    if origin.x == distination.x && origin.z == distination.z {
    //        return (origin.clone(), 0.0);
    //    } else {
    //        let travel_time = move_validation_helpers::travel_time(&origin, &distination, speed);
    //        let time_diff = ((game_state::unix_ms() - self.timestamp) as f32 / 1000.0).clamp(0.0, travel_time);
    //        let t = time_diff / travel_time;
    //        return (FloatHexTile::lerp(&origin, &distination, t), speed * t);
    //    };
    //}
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

#[spacetimedb::table(name = character_stat_desc, public)]
pub struct CharacterStatDesc {
    #[primary_key]
    pub stat_type: i32,
    pub name: String,
    pub value: f32,
    pub min_value: f32,
    pub max_value: f32,
    pub suffix: String,
    pub desc: String,
}

#[spacetimedb::table(name = private_parameters_desc)]
#[derive(Debug)]
pub struct PrivateParametersDesc {
    #[primary_key]
    pub version: i32,

    pub move_validation: MoveValidationParamsDesc,
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

#[spacetimedb::table(name = dimension_description_state, public, index(name = dimension_network_entity_id, btree(columns = [dimension_network_entity_id])))]
#[derive(Default, Clone, bitcraft_macro::Operations, Debug)]
#[operations(delete)]
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

#[spacetimedb::table(name = reset_mobile_entity_timer, scheduled(reset_mobile_entity_position, at = scheduled_at))]
pub struct ResetMobileEntityTimer {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: spacetimedb::ScheduleAt,
    pub owner_entity_id: u64,
    pub position: Option<OffsetCoordinatesFloat>,
    pub strike_counter_to_update: Option<MoveValidationStrikeCounterState>,
}

#[spacetimedb::reducer]
pub fn reset_mobile_entity_position(ctx: &ReducerContext, timer: ResetMobileEntityTimer) -> Result<(), String> {
    ServerIdentity::validate(&ctx)?;

    ctx.db
        .mobile_entity_state()
        .entity_id()
        .update(MobileEntityState::for_location(
            timer.owner_entity_id,
            timer.position.clone().unwrap(),
            ctx.timestamp,
        ));

    if let Some(strike_counter) = timer.strike_counter_to_update {
        ctx.db
            .move_validation_strike_counter_state()
            .entity_id()
            .update(strike_counter);
    }

    PlayerActionState::clear_by_entity_id(ctx, timer.owner_entity_id)
}

mod move_validation_helpers {
    use super::move_validation_strike_counter_state;
    use super::private_parameters_desc;
    use super::reset_mobile_entity_timer;
    use super::MoveValidationStrikeCounterState;
    use super::OffsetCoordinatesFloat;
    use spacetimedb::log;
    use spacetimedb::ReducerContext;
    use spacetimedb::SpacetimeType;
    use spacetimedb::Timestamp;

    pub fn travel_time(source_coordinates: &FloatHexTile, target_coordinates: &FloatHexTile, speed: f32) -> f32 {
        let distance = (source_coordinates.to_world_position() - target_coordinates.to_world_position()).magnitude();
        return distance / speed;
    }

    pub fn validate_move_origin(
        prev_origin: &FloatHexTile,
        cur_origin: &FloatHexTile,
        timestamp_diff_ms: u64,
        move_speed: f32,
        player_id: u64,
    ) -> Result<(), String> {
        const DURATION_LENIENCY_FLAT_VALUE: f32 = 0.05;
        const DURATION_LENIENCY_MULTIPLIER: f32 = 0.9;

        let estimated_duration = travel_time(prev_origin, cur_origin, move_speed);
        let timestamp_diff = timestamp_diff_ms as f32 / 1000.0;
        if timestamp_diff < estimated_duration * DURATION_LENIENCY_MULTIPLIER - DURATION_LENIENCY_FLAT_VALUE {
            log::warn!(
                "Player {} tried to move too quickly from {} to {} (estimated duration: {}, received duration: {})",
                player_id,
                prev_origin,
                cur_origin,
                estimated_duration,
                timestamp_diff
            );
            return Err("~Tried to move too quickly".into());
        }

        Ok(())
    }

    pub fn move_validation_strike(
        ctx: &ReducerContext,
        actor_id: u64,
        entity_id_to_reset: u64,
        prev_origin: FloatHexTile,
        identifier: String,
        error: String,
    ) -> Result<(), String> {
        let params = ctx.db.private_parameters_desc().version().find(&0).unwrap();
        let mut strike_counter = ctx
            .db
            .move_validation_strike_counter_state()
            .entity_id()
            .find(&actor_id)
            .unwrap();

        let oldest_timestamp = Timestamp::from_micros_since_unix_epoch(
            ctx.timestamp.to_time_duration_since_unix_epoch().to_micros()
                - (params.move_validation.strike_counter_time_window_sec * 1_000_000) as i64,
        );
        strike_counter
            .validation_failure_timestamps
            .retain(|t| *t > oldest_timestamp); //Remove old timestamps
        strike_counter.validation_failure_timestamps.push(ctx.timestamp);

        let cur_strikes = strike_counter.validation_failure_timestamps.len() as i32;
        let max_strikes = params.move_validation.strike_count_before_move_validation_failure;
        if cur_strikes > max_strikes {
            log::error!(
                "{} failed move validation, move request is rejected (strike {}/{}, error: '{}')",
                identifier,
                cur_strikes,
                max_strikes,
                error
            );
            return fail_validation(ctx, error, entity_id_to_reset, prev_origin, Some(strike_counter));
        } else {
            log::warn!(
                "{} failed move validation, but is allowed to move (strike {}/{}, error: '{}')",
                identifier,
                cur_strikes,
                max_strikes,
                error
            );
            ctx.db
                .move_validation_strike_counter_state()
                .entity_id()
                .update(strike_counter);
        }

        Ok(())
    }

    pub fn fail_validation(
        ctx: &ReducerContext,
        error: String,
        entity_id: u64,
        coord: FloatHexTile,
        strike_counter: Option<MoveValidationStrikeCounterState>,
    ) -> Result<(), String> {
        let oc: OffsetCoordinatesFloat = coord.into();
        ctx.db
            .reset_mobile_entity_timer()
            .try_insert(ResetMobileEntityTimer {
                scheduled_id: 0,
                scheduled_at: ctx.timestamp.into(),
                owner_entity_id: entity_id,
                position: Some(oc),
                strike_counter_to_update: strike_counter,
            })
            .ok()
            .unwrap();
        return Err(error);
    }
}

mod dimensions {
    pub const OVERWORLD: u32 = 1;
}

#[derive(SpacetimeType, Default, Copy, Clone, Debug, PartialEq, Eq)]
pub struct FloatHexTileMessage {
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

#[derive(SpacetimeType, Default, Copy, Clone, Debug, PartialEq, Eq)]
pub struct SmallHexTileMessage {
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
pub struct OffsetCoordinatesFloat {
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
pub struct OffsetCoordinatesLargeMessage {
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

#[derive(SpacetimeType, PartialEq, Eq, Clone, Copy, Debug)]
pub struct ChunkCoordinatesMessage {
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

#[derive(SpacetimeType, Default, Copy, Clone, Debug, PartialEq)]
pub struct MovementSpeed {
    pub surface_type: SurfaceType,
    pub speed: f32,
}
