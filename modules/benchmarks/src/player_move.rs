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
pub fn player_move(ctx: &ReducerContext, mut request: PlayerMoveRequest) -> Result<(), String> {
    let actor_id = game_state::actor_id(&ctx, true)?;
    PlayerTimestampState::refresh(ctx, actor_id, ctx.timestamp);

    if ctx.db.mounting_state().entity_id().find(&actor_id).is_some() {
        return Err("Can't walk while in a deployable.".into());
    }

    if request.running && InventoryState::get_player_cargo_id(ctx, actor_id) > 0 {
        return Err("Can't run with cargo.".into());
    }

    let player_stats = ctx.db.character_stats_state().entity_id().find(&actor_id).unwrap();
    let mut prev_mobile_entity = ctx.db.mobile_entity_state().entity_id().find(&actor_id).unwrap();

    let prev_origin = prev_mobile_entity.coordinates_float();
    let target_coordinates: FloatHexTile =
        unwrap_or_err!(request.destination, "Expected destination in move request").into();
    let source_coordinates: FloatHexTile = unwrap_or_err!(request.origin, "Expected origin in move request").into();

    let paving = if request.move_type <= 2 {
        PavedTileState::get_at_location(ctx, &prev_origin.parent_small_tile())
            .map(|t| ctx.db.paving_tile_desc().id().find(&t.tile_type_id).unwrap())
    } else {
        None
    };

    let source_large = source_coordinates.parent_large_tile();
    let terrain_chunk = unwrap_or_err!(
        ctx.db
            .terrain_chunk_state()
            .chunk_index()
            .find(source_large.chunk_coordinates().chunk_index()),
        "You can't go here!"
    );

    let water_body_type = terrain_chunk
        .get_water_body_type(source_large)
        .unwrap_or(SurfaceType::Ground as u8);
    let speed = game_state_filters::get_speed_on_water_type(
        &ctx.db
            .parameters_player_move_desc()
            .version()
            .find(&0)
            .unwrap()
            .default_speed,
        water_body_type,
    );

    let stamina_used = if prev_mobile_entity.is_running {
        let distance_traveled = prev_origin.distance_to(source_coordinates.clone());
        let mut run_stamina_use = player_stats.get(CharacterStatType::SprintStaminaDrain);
        if let Some(paving) = &paving {
            run_stamina_use = paving.apply_stat_to_value(ctx, run_stamina_use, CharacterStatType::SprintStaminaDrain);
        }
        let stamina_state = ctx.db.stamina_state().entity_id().find(&actor_id).unwrap();
        let s = distance_traveled * run_stamina_use;
        let s = if stamina_state.stamina + s >= 0.0 {
            s
        } else {
            -stamina_state.stamina
        };
        if (s < 0.0) & (stamina_state.stamina < 0.2) {
            //This is a rough approximation to avoid rubber-banding players from small errors
            // cancel the run, don't drain the stamina
            prev_mobile_entity.is_running = false;
            request.running = false;
            // used to be:
            // return move_validation_helpers::fail_validation(ctx, "Not enough stamina to sprint".into(), actor_id, prev_origin, None);
            0.0
        } else {
            s
        }
    } else {
        0.0
    };

    if !has_role(ctx, &ctx.sender, Role::Gm) {
        move_validation_helpers::validate_move_timestamp(
            prev_mobile_entity.timestamp,
            request.timestamp,
            ctx.timestamp,
        )?;
        move_validation_helpers::validate_move_basic(
            ctx,
            &prev_origin,
            &source_coordinates,
            &target_coordinates,
            request.duration,
        )?;
        validate_move(
            ctx,
            actor_id,
            &player_stats,
            speed,
            &prev_mobile_entity,
            &request,
            source_coordinates,
            target_coordinates,
            &paving,
        )?;
    }

    PlayerState::move_player_and_explore(
        ctx,
        actor_id,
        &source_coordinates,
        &target_coordinates,
        stamina_used,
        request.running,
        Some(request.timestamp),
    )?;

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

        if in_overworld & ((previous_chunk.x != entered_chunk.x) | (previous_chunk.z != entered_chunk.z)) {
            let mut exploration_chunks = ctx.db.exploration_chunks_state().entity_id().find(&entity_id).unwrap();
            if exploration_chunks.explore_chunk(ctx, &entered_chunk, None) {
                PlayerState::discover_ruins_in_chunk(ctx, entity_id, entered_chunk);
                ctx.db.exploration_chunks_state().entity_id().update(exploration_chunks);
            }
        }

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

        // For now, walking into a claim cures long term rez sickness
        if ctx.db.rez_sick_long_term_state().entity_id().find(entity_id).is_some() {
            if let Some(claim_tile) = claim_helper::get_claim_on_tile(ctx, mobile_entity.coordinates()) {
                let claim = ctx.db.claim_state().entity_id().find(claim_tile.claim_id).unwrap();
                let heals_rez_sickness;
                if claim.owner_player_entity_id == 0 {
                    // Check if owner building is the ancient ruins one.
                    let building = ctx
                        .db
                        .building_state()
                        .entity_id()
                        .find(claim.owner_building_entity_id)
                        .unwrap();
                    let building_desc = ctx
                        .db
                        .building_desc()
                        .id()
                        .find(building.building_description_id)
                        .unwrap();
                    heals_rez_sickness = building_desc.has_category(ctx, BuildingCategory::Bed);
                } else {
                    heals_rez_sickness = true;
                }
                if heals_rez_sickness {
                    let mut active_buff_state = ctx.db.active_buff_state().entity_id().find(entity_id).unwrap();
                    let debuff = active_buff_state
                        .active_buff_of_category(ctx, BuffCategory::RezSicknessLongTerm)
                        .unwrap();
                    active_buff_state.remove_active_buff(ctx, debuff.buff_id);
                    ctx.db.active_buff_state().entity_id().update(active_buff_state);
                    // remove rez sickness entry
                    ctx.db.rez_sick_long_term_state().entity_id().delete(entity_id);
                }
            }
        }

        ctx.db.mobile_entity_state().entity_id().update(mobile_entity);

        // Discover claim under feet
        if let Some(claim_tile) = claim_helper::get_claim_on_tile(ctx, target_coordinates.into()) {
            let mut discovery = Discovery::new(entity_id);
            discovery.acquire_claim(ctx, claim_tile.claim_id);
            discovery.commit(ctx);
        }

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

pub struct Discovery {
    pub player_entity_id: u64,
    pub knowledges: Option<Knowledges>,
    pub acquired_achievement: bool,
}

impl Discovery {
    pub fn new(player_entity_id: u64) -> Discovery {
        Discovery {
            player_entity_id,
            knowledges: None,
            acquired_achievement: false,
        }
    }

    pub fn initialize(&mut self, ctx: &ReducerContext) {
        let player_entity_id = self.player_entity_id;
        self.knowledges = Some(Knowledges {
            knowledge_achievement: ctx
                .db
                .knowledge_achievement_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_achievement_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_achievement_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_battle_action: ctx
                .db
                .knowledge_battle_action_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_battle_action_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_battle_action_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_building: ctx
                .db
                .knowledge_building_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_building_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_building_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_cargo: ctx
                .db
                .knowledge_cargo_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_cargo_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_cargo_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_claim: ctx
                .db
                .knowledge_claim_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_claim_hash: Self::entity_entry_array_hash(
                &ctx.db
                    .knowledge_claim_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_construction: ctx
                .db
                .knowledge_construction_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_construction_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_construction_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_craft: ctx
                .db
                .knowledge_craft_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_craft_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_craft_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_deployable: ctx
                .db
                .knowledge_deployable_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_deployable_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_deployable_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_enemy: ctx
                .db
                .knowledge_enemy_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_enemy_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_enemy_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_extract: ctx
                .db
                .knowledge_extract_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_extract_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_extract_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_item: ctx
                .db
                .knowledge_item_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_item_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_item_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_lore: ctx
                .db
                .knowledge_lore_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_lore_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_lore_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_npc: ctx
                .db
                .knowledge_npc_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_npc_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_npc_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_paving: ctx
                .db
                .knowledge_paving_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_paving_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_paving_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_pillar_shaping: ctx
                .db
                .knowledge_pillar_shaping_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_pillar_shaping_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_pillar_shaping_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_resource_placement: ctx
                .db
                .knowledge_resource_placement_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_resource_placement_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_resource_placement_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_resource: ctx
                .db
                .knowledge_resource_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_resource_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_resource_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_ruins: ctx
                .db
                .knowledge_ruins_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_ruins_hash: Self::location_entry_array_hash(
                &ctx.db
                    .knowledge_ruins_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_secondary: ctx
                .db
                .knowledge_secondary_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_secondary_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_secondary_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
            knowledge_vault: ctx
                .db
                .knowledge_vault_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .entries
                .clone(),
            knowledge_vault_hash: Self::knowledge_entry_array_hash(
                &ctx.db
                    .knowledge_vault_state()
                    .entity_id()
                    .find(player_entity_id)
                    .unwrap()
                    .entries,
            ),
        });
    }

    pub(super) fn knowledge_entry_array_hash(entries: &Vec<KnowledgeEntry>) -> i32 {
        entries.iter().filter(|e| e.state == KnowledgeState::Acquired).count() as i32 * 10000
            + entries.iter().filter(|e| e.state == KnowledgeState::Discovered).count() as i32
    }

    pub(super) fn location_entry_array_hash(entries: &Vec<KnowledgeLocationEntry>) -> i32 {
        entries.iter().filter(|e| e.state == KnowledgeState::Acquired).count() as i32 * 10000
            + entries.iter().filter(|e| e.state == KnowledgeState::Discovered).count() as i32
    }

    pub fn commit(&mut self, ctx: &ReducerContext) {
        if self.knowledges.is_none() {
            return;
        }

        self.on_knowledge_acquired(ctx);

        let knowledges = self.knowledges.as_mut().unwrap();

        let player_entity_id = self.player_entity_id;

        let knowledge_achievement_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_achievement);
        if knowledge_achievement_hash != knowledges.knowledge_achievement_hash {
            knowledges.knowledge_achievement_hash = knowledge_achievement_hash;
            let mut knowledge = ctx
                .db
                .knowledge_achievement_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_achievement.clone();
            ctx.db.knowledge_achievement_state().entity_id().update(knowledge);
        }
        let knowledge_battle_action_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_battle_action);
        if knowledge_battle_action_hash != knowledges.knowledge_battle_action_hash {
            knowledges.knowledge_battle_action_hash = knowledge_battle_action_hash;
            let mut knowledge = ctx
                .db
                .knowledge_battle_action_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_battle_action.clone();
            ctx.db.knowledge_battle_action_state().entity_id().update(knowledge);
        }
        let knowledge_building_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_building);
        if knowledge_building_hash != knowledges.knowledge_building_hash {
            knowledges.knowledge_building_hash = knowledge_building_hash;
            let mut knowledge = ctx
                .db
                .knowledge_building_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_building.clone();
            ctx.db.knowledge_building_state().entity_id().update(knowledge);
        }
        let knowledge_cargo_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_cargo);
        if knowledge_cargo_hash != knowledges.knowledge_cargo_hash {
            knowledges.knowledge_cargo_hash = knowledge_cargo_hash;
            let mut knowledge = ctx
                .db
                .knowledge_cargo_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_cargo.clone();
            ctx.db.knowledge_cargo_state().entity_id().update(knowledge);
        }
        let knowledge_claim_hash = Self::entity_entry_array_hash(&knowledges.knowledge_claim);
        if knowledge_claim_hash != knowledges.knowledge_claim_hash {
            knowledges.knowledge_claim_hash = knowledge_claim_hash;
            let mut knowledge = ctx
                .db
                .knowledge_claim_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_claim.clone();
            ctx.db.knowledge_claim_state().entity_id().update(knowledge);
        }
        let knowledge_construction_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_construction);
        if knowledge_construction_hash != knowledges.knowledge_construction_hash {
            knowledges.knowledge_construction_hash = knowledge_construction_hash;
            let mut knowledge = ctx
                .db
                .knowledge_construction_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_construction.clone();
            ctx.db.knowledge_construction_state().entity_id().update(knowledge);
        }
        let knowledge_craft_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_craft);
        if knowledge_craft_hash != knowledges.knowledge_craft_hash {
            knowledges.knowledge_craft_hash = knowledge_craft_hash;
            let mut knowledge = ctx
                .db
                .knowledge_craft_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_craft.clone();
            ctx.db.knowledge_craft_state().entity_id().update(knowledge);
        }
        let knowledge_deployable_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_deployable);
        if knowledge_deployable_hash != knowledges.knowledge_deployable_hash {
            knowledges.knowledge_deployable_hash = knowledge_deployable_hash;
            let mut knowledge = ctx
                .db
                .knowledge_deployable_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_deployable.clone();
            ctx.db.knowledge_deployable_state().entity_id().update(knowledge);
        }
        let knowledge_enemy_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_enemy);
        if knowledge_enemy_hash != knowledges.knowledge_enemy_hash {
            knowledges.knowledge_enemy_hash = knowledge_enemy_hash;
            let mut knowledge = ctx
                .db
                .knowledge_enemy_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_enemy.clone();
            ctx.db.knowledge_enemy_state().entity_id().update(knowledge);
        }
        let knowledge_extract_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_extract);
        if knowledge_extract_hash != knowledges.knowledge_extract_hash {
            knowledges.knowledge_extract_hash = knowledge_extract_hash;
            let mut knowledge = ctx
                .db
                .knowledge_extract_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_extract.clone();
            ctx.db.knowledge_extract_state().entity_id().update(knowledge);
        }
        let knowledge_item_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_item);
        if knowledge_item_hash != knowledges.knowledge_item_hash {
            knowledges.knowledge_item_hash = knowledge_item_hash;
            let mut knowledge = ctx
                .db
                .knowledge_item_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_item.clone();
            ctx.db.knowledge_item_state().entity_id().update(knowledge);
        }
        let knowledge_lore_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_lore);
        if knowledge_lore_hash != knowledges.knowledge_lore_hash {
            knowledges.knowledge_lore_hash = knowledge_lore_hash;
            let mut knowledge = ctx
                .db
                .knowledge_lore_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_lore.clone();
            ctx.db.knowledge_lore_state().entity_id().update(knowledge);
        }
        let knowledge_npc_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_npc);
        if knowledge_npc_hash != knowledges.knowledge_npc_hash {
            knowledges.knowledge_npc_hash = knowledge_npc_hash;
            let mut knowledge = ctx
                .db
                .knowledge_npc_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_npc.clone();
            ctx.db.knowledge_npc_state().entity_id().update(knowledge);
        }
        let knowledge_paving_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_paving);
        if knowledge_paving_hash != knowledges.knowledge_paving_hash {
            knowledges.knowledge_paving_hash = knowledge_paving_hash;
            let mut knowledge = ctx
                .db
                .knowledge_paving_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_paving.clone();
            ctx.db.knowledge_paving_state().entity_id().update(knowledge);
        }
        let knowledge_pillar_shaping_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_pillar_shaping);
        if knowledge_pillar_shaping_hash != knowledges.knowledge_pillar_shaping_hash {
            knowledges.knowledge_pillar_shaping_hash = knowledge_pillar_shaping_hash;
            let mut knowledge = ctx
                .db
                .knowledge_pillar_shaping_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_pillar_shaping.clone();
            ctx.db.knowledge_pillar_shaping_state().entity_id().update(knowledge);
        }
        let knowledge_resource_placement_hash =
            Self::knowledge_entry_array_hash(&knowledges.knowledge_resource_placement);
        if knowledge_resource_placement_hash != knowledges.knowledge_resource_placement_hash {
            knowledges.knowledge_resource_placement_hash = knowledge_resource_placement_hash;
            let mut knowledge = ctx
                .db
                .knowledge_resource_placement_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_resource_placement.clone();
            ctx.db
                .knowledge_resource_placement_state()
                .entity_id()
                .update(knowledge);
        }
        let knowledge_resource_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_resource);
        if knowledge_resource_hash != knowledges.knowledge_resource_hash {
            knowledges.knowledge_resource_hash = knowledge_resource_hash;
            let mut knowledge = ctx
                .db
                .knowledge_resource_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_resource.clone();
            ctx.db.knowledge_resource_state().entity_id().update(knowledge);
        }
        let knowledge_ruins_hash = Self::location_entry_array_hash(&knowledges.knowledge_ruins);
        if knowledge_ruins_hash != knowledges.knowledge_ruins_hash {
            knowledges.knowledge_ruins_hash = knowledge_ruins_hash;
            let mut knowledge = ctx
                .db
                .knowledge_ruins_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_ruins.clone();
            ctx.db.knowledge_ruins_state().entity_id().update(knowledge);
        }
        let knowledge_secondary_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_secondary);
        if knowledge_secondary_hash != knowledges.knowledge_secondary_hash {
            knowledges.knowledge_secondary_hash = knowledge_secondary_hash;
            let mut knowledge = ctx
                .db
                .knowledge_secondary_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_secondary.clone();
            ctx.db.knowledge_secondary_state().entity_id().update(knowledge);
            PlayerState::collect_stats(ctx, self.player_entity_id);
        }
        let knowledge_vault_hash = Self::knowledge_entry_array_hash(&knowledges.knowledge_vault);
        if knowledge_vault_hash != knowledges.knowledge_vault_hash {
            knowledges.knowledge_vault_hash = knowledge_vault_hash;
            let mut knowledge = ctx
                .db
                .knowledge_vault_state()
                .entity_id()
                .find(player_entity_id)
                .unwrap()
                .clone();
            knowledge.entries = knowledges.knowledge_vault.clone();
            ctx.db.knowledge_vault_state().entity_id().update(knowledge);
        }
        if self.acquired_achievement {
            self.acquired_achievement = false;
            AchievementDesc::evaluate_all(self.player_entity_id);
        }
    }

    pub fn acquire_claim(&mut self, ctx: &ReducerContext, claim_entity_id: u64) {
        if claim_entity_id != 0 {
            if self.knowledges.is_none() {
                if Self::already_acquired_claim(ctx, self.player_entity_id, claim_entity_id) {
                    return;
                }
                self.initialize(ctx);
            }
            if self.has_acquired_claim(claim_entity_id) {
                return;
            }
            if !self.has_discovered_claim(claim_entity_id) {
                self.discover_claim(ctx, claim_entity_id);
            }
            let knowledges = self.knowledges.as_mut().unwrap();
            if let Some(entry) = knowledges
                .knowledge_claim
                .iter_mut()
                .find(|e| e.entity_id == claim_entity_id)
            {
                entry.state = KnowledgeState::Acquired;
            }
        }
    }

    pub fn already_acquired_claim(ctx: &ReducerContext, player_entity_id: u64, claim_entity_id: u64) -> bool {
        if let Some(knowledge) = ctx.db.knowledge_claim_state().entity_id().find(player_entity_id) {
            if let Some(entry) = knowledge.entries.iter().find(|e| e.entity_id == claim_entity_id) {
                // acquired or discovered means discovered
                return entry.state == KnowledgeState::Acquired;
            }
        }
        false
    }

    pub fn has_discovered_claim(&self, claim_entity_id: u64) -> bool {
        // This should only be used internally if any change was found in the knowledges, therefore knowledges should yield a value.
        if let Some(knowledges) = &self.knowledges {
            return knowledges
                .knowledge_claim
                .iter()
                .any(|e| e.entity_id == claim_entity_id);
        }
        false
    }

    pub fn has_acquired_claim(&self, claim_entity_id: u64) -> bool {
        // This should only be used internally if any change was found in the knowledges, therefore knowledges should yield a value.
        if let Some(knowledges) = &self.knowledges {
            return knowledges
                .knowledge_claim
                .iter()
                .any(|e| e.entity_id == claim_entity_id && e.state == KnowledgeState::Acquired);
        }
        false
    }

    pub fn already_discovered_claim(ctx: &ReducerContext, player_entity_id: u64, claim_entity_id: u64) -> bool {
        if let Some(knowledge) = ctx.db.knowledge_claim_state().entity_id().find(player_entity_id) {
            if let Some(_entry) = knowledge.entries.iter().find(|e| e.entity_id == claim_entity_id) {
                // acquired or discovered means discovered
                return true;
            }
        }
        false
    }

    pub fn discover_claim(&mut self, ctx: &ReducerContext, claim_entity_id: u64) {
        if claim_entity_id != 0 {
            if self.knowledges.is_none() {
                if Self::already_discovered_claim(ctx, self.player_entity_id, claim_entity_id) {
                    return;
                }
                self.initialize(ctx);
            }
            if self.has_discovered_claim(claim_entity_id) {
                return;
            }
            let knowledges = self.knowledges.as_mut().unwrap();
            if !knowledges
                .knowledge_claim
                .iter()
                .any(|e| e.entity_id == claim_entity_id)
            {
                let knowledge_entry = KnowledgeEntityEntry {
                    entity_id: claim_entity_id,
                    state: KnowledgeState::Discovered,
                };
                knowledges.knowledge_claim.push(knowledge_entry);
            };
        }
    }
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

fn validate_move(
    ctx: &ReducerContext,
    actor_id: u64,
    player_stats: &CharacterStatsState,
    speed: f32,
    prev_mobile_entity: &MobileEntityState,
    request: &PlayerMoveRequest,
    source_coordinates: FloatHexTile,
    _target_coordinates: FloatHexTile,
    paving: &Option<PavingTileDesc>,
) -> Result<(), String> {
    let prev_origin = prev_mobile_entity.coordinates_float();

    // if source_coordinates.x != target_coordinates.x || source_coordinates.z != target_coordinates.z {
    if source_coordinates.x != prev_origin.x || source_coordinates.z != prev_origin.z {
        let base_speed = speed * player_stats.get(CharacterStatType::MovementMultiplier);
        let mut prev_speed = if prev_mobile_entity.is_running {
            base_speed * player_stats.get(CharacterStatType::SprintMultiplier)
        } else {
            base_speed
        };
        if request.move_type > 2 {
            prev_speed *= 2.0; //Transitions are above the law
        }

        if let Some(paving) = paving {
            prev_speed = paving.apply_stat_to_value_unclamped(prev_speed, CharacterStatType::MovementMultiplier);
            if prev_mobile_entity.is_running {
                prev_speed = paving.apply_stat_to_value_unclamped(prev_speed, CharacterStatType::SprintMultiplier);
            }
        }

        //let (cur_position, cur_distance) = prev_mobile_entity.cur_coord_and_distance_traveled(prev_speed);

        let timestamp_diff_ms = request.timestamp - prev_mobile_entity.timestamp;
        if let Err(error) = move_validation_helpers::validate_move_origin(
            &prev_origin,
            &source_coordinates,
            timestamp_diff_ms,
            prev_speed,
            actor_id,
        ) {
            //Can return Err or Ok
            return move_validation_helpers::move_validation_strike(
                ctx,
                actor_id,
                actor_id,
                prev_origin,
                format!("Player {actor_id}"),
                error,
            );
        }

        //DAB Note
        // TODO: enable this at some point
        //let par = ctx.db.parameters_desc().version().find(&0).unwrap();
        //if let Err(error) = reducer_helpers::validate_move(
        //    &prev_mobile_entity,
        //    &prev_origin,
        //    &source_coordinates,
        //    &target_coordinates,
        //    par.player_climb_height as i32,
        //    par.player_swim_height as i32,
        //    MovementType::Amphibious,
        //    prev_speed,
        //    new_speed,
        //    request.duration,
        //    actor_id,
        //) {
        //    //return fail_validation(error, actor_id, cur_position);
        //    return fail_validation(error, actor_id, prev_origin);
        //}
    }

    Ok(())
}

impl PavingTileDesc {
    pub fn try_get_stat(&self, stat_type: CharacterStatType) -> Option<&CsvStatEntry> {
        assert!(
            match stat_type {
                CharacterStatType::MovementMultiplier => true,
                CharacterStatType::SprintMultiplier => true,
                CharacterStatType::SprintStaminaDrain => true,
                _ => false,
            },
            "Stat {:?} is not supported in pavement",
            stat_type
        );

        for stat in &self.stat_effects {
            if stat.id == stat_type {
                return Some(stat);
            }
        }
        return None;
    }

    pub fn apply_stat_to_value(&self, ctx: &ReducerContext, val: f32, stat_type: CharacterStatType) -> f32 {
        if let Some(stat) = self.try_get_stat(stat_type) {
            let desc = ctx.db.character_stat_desc().stat_type().find(stat_type as i32).unwrap();
            return if stat.is_pct {
                val * (1.0 + stat.value)
            } else {
                val + stat.value
            }
            .clamp(desc.min_value, desc.max_value);
        } else {
            return val;
        }
    }

    pub fn apply_stat_to_value_unclamped(&self, val: f32, stat_type: CharacterStatType) -> f32 {
        if let Some(stat) = self.try_get_stat(stat_type) {
            return if stat.is_pct {
                val * (1.0 + stat.value)
            } else {
                val + stat.value
            };
        } else {
            return val;
        }
    }
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

    pub fn validate_move_basic(
        ctx: &ReducerContext,
        prev_origin: &FloatHexTile,
        source_coordinates: &FloatHexTile,
        target_coordinates: &FloatHexTile,
        duration: f32,
    ) -> Result<(), String> {
        //Blatant cheating checks
        const MAX_DURATION: f32 = 3.0;
        const MAX_DISTANCE: f32 = 7.0; //Realistically should never exceed 3
        const MAX_DISTANCE_FROM_PREV_STATE: f32 = 7.0;
        const MAX_SPEED: f32 = 100.0;
        if duration > MAX_DURATION || duration < 0.0 {
            return Err("Invalid duration.".into());
        }
        if source_coordinates.dimension != target_coordinates.dimension {
            return Err("Invalid dimension.".into());
        }

        if target_coordinates.dimension != prev_origin.dimension {
            return Err("Client sent wrong dimension".into());
        }

        if target_coordinates.distance_to(*source_coordinates) > MAX_DISTANCE {
            return Err("Can't move that far".into());
        }

        if prev_origin.distance_to(*source_coordinates) > MAX_DISTANCE_FROM_PREV_STATE {
            return Err("Can't move that far".into());
        }

        if target_coordinates.dimension != dimensions::OVERWORLD
            && !game_state_filters::is_interior_tile_walkable(ctx, target_coordinates.parent_small_tile())
        {
            return Err("Can't move outside interior bounds".into());
        }

        let distance = (target_coordinates.to_world_position() - source_coordinates.to_world_position()).magnitude();
        if duration == 0.0 {
            if distance > 0.0 {
                return Err("Can't move that fast".into());
            }
        } else {
            let speed = distance / duration;
            if speed > MAX_SPEED {
                return Err("Can't move that fast".into());
            }
        }

        Ok(())
    }

    pub fn validate_move_timestamp(prev_timestamp: u64, received_timestamp: u64, now: Timestamp) -> Result<(), String> {
        //Allow some leniency since clients don't currently have accurate ServerTime
        const MAX_OFFSET_INTO_PAST_MS: i64 = 8000; //Allow processing requests "from the past" (client sent a request that got delayed)
        const MAX_OFFSET_INTO_FUTURE_MS: i64 = 1000; //Allow receiving requests "from the future" (client recovering after a lag spike)

        let prev_timestamp = prev_timestamp as i64;
        let received_timestamp = received_timestamp as i64;
        let now_ms = game_state::unix_ms(now) as i64;
        if received_timestamp - now_ms > MAX_OFFSET_INTO_FUTURE_MS {
            log::warn!(
                "Invalid timestamp: too far into the future. Current time: {}, received time: {}",
                now_ms,
                received_timestamp
            );
            return Err("~Invalid timestamp".into());
        }

        if now_ms - received_timestamp > MAX_OFFSET_INTO_PAST_MS {
            log::warn!(
                "Invalid timestamp: too far in the past. Current time: {}, received time: {}",
                now_ms,
                received_timestamp
            );
            return Err("~Invalid timestamp".into());
        }

        if received_timestamp < prev_timestamp {
            log::warn!(
            "Invalid timestamp: previous timestamp is more recent. Current time: {}, previous timestamp: {}, received time: {}",
            now_ms,
            prev_timestamp,
            received_timestamp
        );
            return Err("~Invalid timestamp".into());
        }

        Ok(())
    }

    //Leaving this here for when we re-enable more comprehensivee move validation
    //#[allow(dead_code)]
    //pub fn validate_move_old(
    //    source_coordinates: &FloatHexTile,
    //    target_coordinates: &FloatHexTile,
    //    max_elevation: i32,
    //    movement_type: MovementType,
    //) -> Result<(), String> {
    //    let mut error: String = "".into();
    //
    //    let can_move_on_tile = |tile: SmallHexTile, error: &mut String| -> bool {
    //        // Can't move onto "hitbox" footprints
    //        if game_state_filters::has_hitbox_footprint(tile) {
    //            *error = "Can't walk through here!".into();
    //            return false;
    //        }
    //
    //        if movement_type != MovementType::Amphibious {
    //            let terrain = TerrainChunkState::get_terrain_cell(ctx, &tile.parent_large_tile()).unwrap();
    //            let is_submerged = terrain.is_submerged();
    //            if is_submerged && movement_type == MovementType::Ground {
    //                *error = "Can't move on water.".into();
    //                return false;
    //            }
    //            if !is_submerged && movement_type == MovementType::Water {
    //                *error = "Can't move on ground.".into();
    //                return false;
    //            }
    //        }
    //
    //        return true;
    //    };
    //
    //    let mut can_move_on_next_tile = !can_move_on_tile(source_coordinates.clone().into(), &mut error); //Check to make sure players don't get stuck inside buildings
    //
    //    let can_transition = |tile_from: SmallHexTile, tile_to: SmallHexTile| -> bool {
    //        if !can_move_on_tile(tile_to, &mut error) {
    //            return can_move_on_next_tile;
    //        }
    //
    //        //Prevent movement over elevation (unless in water - todo if we have waterfalls)
    //        if movement_type == MovementType::Ground {
    //            let terrain_source = unwrap_or_err!(TerrainChunkState::get_terrain_cell(ctx, &tile_from.parent_large_tile()), "Invalid source location");
    //            let terrain_target = unwrap_or_err!(TerrainChunkState::get_terrain_cell(ctx, &tile_to.parent_large_tile()), "Invalid destination");
    //            let elevation_diff = i32::abs(terrain_source.elevation - terrain_target.elevation);
    //            if elevation_diff > max_elevation {
    //                error = "Pathfinding error - move handler trying to climb.".into();
    //                return can_move_on_next_tile;
    //            }
    //        }
    //
    //        can_move_on_next_tile = false;
    //        return true;
    //    };
    //
    //    if !raycast(source_coordinates, target_coordinates, can_transition) {
    //        return Err(error.to_string());
    //    }
    //
    //    Ok(())
    //}

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
