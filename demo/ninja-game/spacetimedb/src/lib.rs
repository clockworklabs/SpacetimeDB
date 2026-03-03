use std::collections::HashSet;

use spacetimedb::{log, ReducerContext, Table};

const WORLD_MIN: f32 = 0.0;
const WORLD_MAX: f32 = 1000.0;
const START_HEALTH: u32 = 100;
const PICKUP_RADIUS: f32 = 50.0;
const DAMAGE_PER_HIT: u32 = 5;
const HIT_COOLDOWN_MICROS: i64 = 125_000;
const MAX_PLAYERS_PER_LOBBY: usize = 30;

#[spacetimedb::table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    pub id: u64,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub health: u32,
    pub weapon_count: u32,
    pub kills: u32,
    pub respawn_at_micros: i64,
    pub is_ready: bool,
    pub lobby_id: Option<u64>,
}

#[spacetimedb::table(accessor = lobby, public)]
pub struct Lobby {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
    pub is_playing: bool,
}

#[spacetimedb::table(accessor = weapon_drop, public)]
pub struct WeaponDrop {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub damage: u32,
    pub lobby_id: u64,
}

#[spacetimedb::table(accessor = bot_player)]
pub struct BotPlayer {
    #[primary_key]
    pub id: u64,
    pub lobby_id: u64,
}

#[spacetimedb::table(accessor = combat_hit_cooldown)]
pub struct CombatHitCooldown {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub attacker_id: u64,
    pub target_id: u64,
    pub last_hit_micros: i64,
}

fn player_id_from_ctx(ctx: &ReducerContext) -> u64 {
    let bytes = ctx.sender().to_byte_array();
    u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ])
}

fn clamp_to_world(value: f32) -> f32 {
    value.clamp(WORLD_MIN, WORLD_MAX)
}

fn normalize_name(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.chars().take(24).collect()
    }
}

fn respawn_pos(seed: u64) -> (f32, f32) {
    let a = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let b = a
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let x = 50.0 + (a >> 32) as f32 / u32::MAX as f32 * 900.0;
    let y = 50.0 + (b >> 32) as f32 / u32::MAX as f32 * 900.0;
    (x, y)
}

fn lobby_player_count(ctx: &ReducerContext, lobby_id: u64) -> usize {
    ctx.db
        .player()
        .iter()
        .filter(|p| p.lobby_id == Some(lobby_id))
        .count()
}

fn is_bot_player(ctx: &ReducerContext, player_id: u64) -> bool {
    ctx.db.bot_player().id().find(player_id).is_some()
}

fn player_row_is_bot(ctx: &ReducerContext, player: &Player) -> bool {
    is_bot_player(ctx, player.id) || player.name.starts_with("Bot ")
}

fn lobby_human_player_count(ctx: &ReducerContext, lobby_id: u64) -> usize {
    ctx.db
        .player()
        .iter()
        .filter(|p| p.lobby_id == Some(lobby_id))
        .filter(|p| !player_row_is_bot(ctx, p))
        .count()
}

fn clear_combat_rows_for_player(ctx: &ReducerContext, player_id: u64) {
    let stale_rows: Vec<u64> = ctx
        .db
        .combat_hit_cooldown()
        .iter()
        .filter(|row| row.attacker_id == player_id || row.target_id == player_id)
        .map(|row| row.id)
        .collect();
    for row_id in stale_rows {
        ctx.db.combat_hit_cooldown().id().delete(row_id);
    }
}

fn cleanup_lobby_if_empty(ctx: &ReducerContext, lobby_id: u64) {
    // A lobby should only remain alive while at least one human player is present.
    if lobby_human_player_count(ctx, lobby_id) > 0 {
        return;
    }

    let player_ids: Vec<u64> = ctx
        .db
        .player()
        .iter()
        .filter(|p| p.lobby_id == Some(lobby_id))
        .map(|p| p.id)
        .collect();
    for player_id in player_ids {
        clear_combat_rows_for_player(ctx, player_id);
        ctx.db.player().id().delete(player_id);
        ctx.db.bot_player().id().delete(player_id);
    }

    let weapon_ids: Vec<u64> = ctx
        .db
        .weapon_drop()
        .iter()
        .filter(|w| w.lobby_id == lobby_id)
        .map(|w| w.id)
        .collect();
    for wid in weapon_ids {
        ctx.db.weapon_drop().id().delete(wid);
    }

    let bot_ids: Vec<u64> = ctx
        .db
        .bot_player()
        .iter()
        .filter(|b| b.lobby_id == lobby_id)
        .map(|b| b.id)
        .collect();
    for bot_id in bot_ids {
        ctx.db.bot_player().id().delete(bot_id);
    }

    ctx.db.lobby().id().delete(lobby_id);
}

fn remove_player(ctx: &ReducerContext, player_id: u64) {
    if let Some(player) = ctx.db.player().id().find(player_id) {
        let lobby_id = player.lobby_id;
        ctx.db.player().id().delete(player_id);
        clear_combat_rows_for_player(ctx, player_id);
        ctx.db.bot_player().id().delete(player_id);
        if let Some(lobby_id) = lobby_id {
            cleanup_lobby_if_empty(ctx, lobby_id);
        }
    }
}

fn upsert_player_name(ctx: &ReducerContext, player_id: u64, name: String) {
    if let Some(mut existing) = ctx.db.player().id().find(player_id) {
        existing.name = name;
        ctx.db.player().id().update(existing);
        return;
    }

    ctx.db.player().insert(Player {
        id: player_id,
        name,
        x: 500.0,
        y: 500.0,
        health: START_HEALTH,
        weapon_count: 0,
        kills: 0,
        respawn_at_micros: 0,
        is_ready: false,
        lobby_id: None,
    });
}

fn apply_combat(ctx: &ReducerContext, attacker_id: u64, target_id: u64) {
    if attacker_id == target_id {
        return;
    }

    let Some(attacker_current) = ctx.db.player().id().find(attacker_id) else {
        return;
    };
    if attacker_current.weapon_count == 0 || attacker_current.health == 0 {
        return;
    }

    let Some(mut target) = ctx.db.player().id().find(target_id) else {
        return;
    };
    if target.health == 0 {
        return;
    }

    let Some(lobby_id) = attacker_current.lobby_id else {
        return;
    };
    if target.lobby_id != Some(lobby_id) {
        return;
    }
    let Some(lobby) = ctx.db.lobby().id().find(lobby_id) else {
        return;
    };
    if !lobby.is_playing {
        return;
    }

    let now_micros = ctx.timestamp.to_micros_since_unix_epoch();
    let existing_cooldown = ctx
        .db
        .combat_hit_cooldown()
        .iter()
        .find(|row| row.attacker_id == attacker_id && row.target_id == target_id);

    if let Some(mut cooldown_row) = existing_cooldown {
        if now_micros - cooldown_row.last_hit_micros < HIT_COOLDOWN_MICROS {
            return;
        }
        cooldown_row.last_hit_micros = now_micros;
        ctx.db.combat_hit_cooldown().id().update(cooldown_row);
    } else {
        ctx.db.combat_hit_cooldown().insert(CombatHitCooldown {
            id: 0,
            attacker_id,
            target_id,
            last_hit_micros: now_micros,
        });
    }

    let mut attacker = attacker_current;
    let target_is_bot = player_row_is_bot(ctx, &target);
    if target.health <= DAMAGE_PER_HIT {
        attacker.kills += 1;
        if target_is_bot {
            // Bots are removed from the match when killed and do not respawn.
            ctx.db.player().id().update(attacker);
            remove_player(ctx, target_id);
            return;
        }
        target.health = 0;
        target.weapon_count = 0;
        target.respawn_at_micros = now_micros + 2_000_000;
    } else {
        target.health -= DAMAGE_PER_HIT;
    }

    ctx.db.player().id().update(attacker);
    ctx.db.player().id().update(target);
}

#[spacetimedb::reducer]
pub fn set_name(ctx: &ReducerContext, name: String) {
    let player_id = player_id_from_ctx(ctx);
    let fallback = format!("Player {:04X}", player_id & 0xFFFF);
    let normalized = normalize_name(&name, &fallback);
    upsert_player_name(ctx, player_id, normalized);

    // If the player isn't in a lobby yet, auto-assign them to one.
    // This allows "Quick Play" from the title screen by just setting a name.
    let mut player = ctx.db.player().id().find(player_id).unwrap();
    if player.lobby_id.is_none() {
        let lobby_id = if let Some(l) = ctx
            .db
            .lobby()
            .iter()
            .find(|l| !l.is_playing && lobby_player_count(ctx, l.id) < MAX_PLAYERS_PER_LOBBY)
        {
            l.id
        } else if let Some(l) = ctx
            .db
            .lobby()
            .iter()
            .find(|l| lobby_player_count(ctx, l.id) < MAX_PLAYERS_PER_LOBBY)
        {
            l.id
        } else {
            let lobby_name = format!("{}'s Lobby", player.name);
            ctx.db
                .lobby()
                .insert(Lobby {
                    id: 0,
                    name: lobby_name,
                    is_playing: false,
                })
                .id
        };
        player.lobby_id = Some(lobby_id);
        ctx.db.player().id().update(player);
    }
}

#[spacetimedb::reducer]
pub fn join(ctx: &ReducerContext, name: String) {
    set_name(ctx, name);
}

#[spacetimedb::reducer]
pub fn leave(ctx: &ReducerContext) {
    let player_id = player_id_from_ctx(ctx);
    remove_player(ctx, player_id);
}

#[spacetimedb::reducer]
pub fn create_lobby(ctx: &ReducerContext, name: String) {
    let player_id = player_id_from_ctx(ctx);
    if ctx.db.player().id().find(player_id).is_none() {
        let fallback = format!("Player {:04X}", player_id & 0xFFFF);
        upsert_player_name(ctx, player_id, fallback);
    }

    let Some(mut player) = ctx.db.player().id().find(player_id) else {
        return;
    };
    if player.lobby_id.is_some() {
        return;
    }

    let lobby_name = normalize_name(&name, "Quick Lobby");
    let lobby = ctx.db.lobby().insert(Lobby {
        id: 0,
        name: lobby_name,
        is_playing: false,
    });

    player.lobby_id = Some(lobby.id);
    player.is_ready = false;
    player.health = START_HEALTH;
    player.weapon_count = 0;
    player.respawn_at_micros = 0;
    player.x = 500.0;
    player.y = 500.0;
    ctx.db.player().id().update(player);
}

#[spacetimedb::reducer]
pub fn join_lobby(ctx: &ReducerContext, lobby_id: u64) {
    let player_id = player_id_from_ctx(ctx);
    if ctx.db.player().id().find(player_id).is_none() {
        let fallback = format!("Player {:04X}", player_id & 0xFFFF);
        upsert_player_name(ctx, player_id, fallback);
    }

    let Some(mut player) = ctx.db.player().id().find(player_id) else {
        return;
    };
    if player.lobby_id.is_some() {
        return;
    }

    if ctx.db.lobby().id().find(lobby_id).is_none() {
        return;
    }
    if lobby_player_count(ctx, lobby_id) >= MAX_PLAYERS_PER_LOBBY {
        return;
    }

    player.lobby_id = Some(lobby_id);
    player.is_ready = false;
    player.health = START_HEALTH;
    player.weapon_count = 0;
    player.respawn_at_micros = 0;
    player.x = 500.0;
    player.y = 500.0;
    ctx.db.player().id().update(player);
}

#[spacetimedb::reducer]
pub fn leave_lobby(ctx: &ReducerContext) {
    let player_id = player_id_from_ctx(ctx);
    let Some(mut player) = ctx.db.player().id().find(player_id) else {
        return;
    };
    let Some(lobby_id) = player.lobby_id else {
        return;
    };

    player.lobby_id = None;
    player.is_ready = false;
    player.health = START_HEALTH;
    player.weapon_count = 0;
    player.respawn_at_micros = 0;
    player.x = 500.0;
    player.y = 500.0;
    ctx.db.player().id().update(player);
    clear_combat_rows_for_player(ctx, player_id);

    cleanup_lobby_if_empty(ctx, lobby_id);
}

#[spacetimedb::reducer]
pub fn toggle_ready(ctx: &ReducerContext) {
    let player_id = player_id_from_ctx(ctx);
    let Some(mut player) = ctx.db.player().id().find(player_id) else {
        return;
    };
    let Some(lobby_id) = player.lobby_id else {
        return;
    };
    let Some(lobby) = ctx.db.lobby().id().find(lobby_id) else {
        return;
    };
    if lobby.is_playing {
        return;
    }

    player.is_ready = !player.is_ready;
    ctx.db.player().id().update(player);
}

#[spacetimedb::reducer]
pub fn start_match(ctx: &ReducerContext) {
    let caller_id = player_id_from_ctx(ctx);
    let Some(caller) = ctx.db.player().id().find(caller_id) else {
        return;
    };
    let Some(lobby_id) = caller.lobby_id else {
        return;
    };

    let Some(mut lobby) = ctx.db.lobby().id().find(lobby_id) else {
        return;
    };
    if lobby.is_playing {
        return;
    }
    lobby.is_playing = true;
    ctx.db.lobby().id().update(lobby);

    let now = ctx.timestamp.to_micros_since_unix_epoch() as u64;
    let players: Vec<Player> = ctx
        .db
        .player()
        .iter()
        .filter(|p| p.lobby_id == Some(lobby_id))
        .collect();
    for mut p in players {
        let (x, y) = respawn_pos(p.id.wrapping_add(now));
        p.x = x;
        p.y = y;
        p.health = START_HEALTH;
        p.weapon_count = 0;
        p.respawn_at_micros = 0;
        p.is_ready = false;
        ctx.db.player().id().update(p);
    }

    let weapon_ids: Vec<u64> = ctx
        .db
        .weapon_drop()
        .iter()
        .filter(|w| w.lobby_id == lobby_id)
        .map(|w| w.id)
        .collect();
    for wid in weapon_ids {
        ctx.db.weapon_drop().id().delete(wid);
    }
}

#[spacetimedb::reducer]
pub fn end_match(ctx: &ReducerContext) {
    let caller_id = player_id_from_ctx(ctx);
    let Some(caller) = ctx.db.player().id().find(caller_id) else {
        return;
    };
    let Some(lobby_id) = caller.lobby_id else {
        return;
    };

    // If caller is the last human, ending the match should dissolve the session.
    if lobby_human_player_count(ctx, lobby_id) <= 1 {
        remove_player(ctx, caller_id);
        return;
    }

    if let Some(mut lobby) = ctx.db.lobby().id().find(lobby_id) {
        lobby.is_playing = false;
        ctx.db.lobby().id().update(lobby);
    }

    let mut bot_ids: HashSet<u64> = ctx
        .db
        .bot_player()
        .iter()
        .filter(|b| b.lobby_id == lobby_id)
        .map(|b| b.id)
        .collect();
    for p in ctx.db.player().iter().filter(|p| p.lobby_id == Some(lobby_id)) {
        if player_row_is_bot(ctx, &p) {
            bot_ids.insert(p.id);
        }
    }

    let players: Vec<Player> = ctx
        .db
        .player()
        .iter()
        .filter(|p| p.lobby_id == Some(lobby_id))
        .collect();
    let player_ids: HashSet<u64> = players.iter().map(|p| p.id).collect();
    for mut p in players {
        if bot_ids.contains(&p.id) {
            ctx.db.player().id().delete(p.id);
            continue;
        }
        p.is_ready = false;
        p.health = START_HEALTH;
        p.weapon_count = 0;
        p.respawn_at_micros = 0;
        ctx.db.player().id().update(p);
    }

    let weapon_ids: Vec<u64> = ctx
        .db
        .weapon_drop()
        .iter()
        .filter(|w| w.lobby_id == lobby_id)
        .map(|w| w.id)
        .collect();
    for wid in weapon_ids {
        ctx.db.weapon_drop().id().delete(wid);
    }

    let cooldown_ids: Vec<u64> = ctx
        .db
        .combat_hit_cooldown()
        .iter()
        .filter(|row| player_ids.contains(&row.attacker_id) || player_ids.contains(&row.target_id))
        .map(|row| row.id)
        .collect();
    for cid in cooldown_ids {
        ctx.db.combat_hit_cooldown().id().delete(cid);
    }

    for bot_id in bot_ids {
        ctx.db.bot_player().id().delete(bot_id);
    }
}

#[spacetimedb::reducer]
pub fn spawn_test_player(ctx: &ReducerContext) {
    let caller_id = player_id_from_ctx(ctx);
    let Some(caller) = ctx.db.player().id().find(caller_id) else {
        return;
    };
    let Some(lobby_id) = caller.lobby_id else {
        return;
    };
    let Some(lobby) = ctx.db.lobby().id().find(lobby_id) else {
        return;
    };
    if !lobby.is_playing {
        return;
    }
    if lobby_player_count(ctx, lobby_id) >= MAX_PLAYERS_PER_LOBBY {
        return;
    }

    let now = ctx.timestamp.to_micros_since_unix_epoch() as u64;
    let mut bot_id = now.wrapping_mul(1103515245).wrapping_add(12345);
    while ctx.db.player().id().find(bot_id).is_some() {
        bot_id = bot_id.wrapping_add(1);
    }

    let bot_name = format!("Bot {}", bot_id % 10_000);
    let (x, y) = respawn_pos(bot_id.wrapping_add(now));
    ctx.db.player().insert(Player {
        id: bot_id,
        name: bot_name,
        x,
        y,
        health: START_HEALTH,
        weapon_count: 0,
        kills: 0,
        respawn_at_micros: 0,
        is_ready: true,
        lobby_id: Some(lobby_id),
    });
    ctx.db.bot_player().insert(BotPlayer { id: bot_id, lobby_id });
}

#[spacetimedb::reducer]
pub fn move_player(ctx: &ReducerContext, x: f32, y: f32) {
    let player_id = player_id_from_ctx(ctx);
    let Some(mut player) = ctx.db.player().id().find(player_id) else {
        return;
    };
    if player.health == 0 {
        return;
    }

    player.x = clamp_to_world(x);
    player.y = clamp_to_world(y);

    if let Some(lobby_id) = player.lobby_id {
        let nearby: Vec<u64> = ctx
            .db
            .weapon_drop()
            .iter()
            .filter(|w| w.lobby_id == lobby_id)
            .filter(|w| {
                let dx = player.x - w.x;
                let dy = player.y - w.y;
                dx * dx + dy * dy < PICKUP_RADIUS * PICKUP_RADIUS
            })
            .map(|w| w.id)
            .collect();

        if !nearby.is_empty() {
            for wid in &nearby {
                ctx.db.weapon_drop().id().delete(*wid);
            }
            player.weapon_count += nearby.len() as u32;
        }
    }

    ctx.db.player().id().update(player);
}

#[spacetimedb::reducer]
pub fn attack(ctx: &ReducerContext, target_id: u64) {
    let attacker_id = player_id_from_ctx(ctx);
    apply_combat(ctx, attacker_id, target_id);
}

#[spacetimedb::reducer]
pub fn spawn_weapon(ctx: &ReducerContext, x: f32, y: f32) {
    let player_id = player_id_from_ctx(ctx);
    let Some(player) = ctx.db.player().id().find(player_id) else {
        return;
    };
    let Some(lobby_id) = player.lobby_id else {
        return;
    };

    let Some(lobby) = ctx.db.lobby().id().find(lobby_id) else {
        return;
    };
    if !lobby.is_playing {
        return;
    }

    ctx.db.weapon_drop().insert(WeaponDrop {
        id: 0,
        x: clamp_to_world(x),
        y: clamp_to_world(y),
        damage: DAMAGE_PER_HIT,
        lobby_id,
    });
}

#[spacetimedb::reducer]
pub fn respawn(ctx: &ReducerContext) {
    let player_id = player_id_from_ctx(ctx);
    let Some(mut player) = ctx.db.player().id().find(player_id) else {
        return;
    };
    if player.health > 0 {
        return;
    }

    let now = ctx.timestamp.to_micros_since_unix_epoch();
    if player.respawn_at_micros > now {
        return;
    }

    let (x, y) = respawn_pos(player_id.wrapping_add(now as u64));
    player.x = x;
    player.y = y;
    player.health = START_HEALTH;
    player.respawn_at_micros = 0;
    ctx.db.player().id().update(player);
}

#[spacetimedb::reducer]
pub fn clear_server(ctx: &ReducerContext) {
    let player_ids: Vec<u64> = ctx.db.player().iter().map(|p| p.id).collect();
    let lobby_ids: Vec<u64> = ctx.db.lobby().iter().map(|l| l.id).collect();
    let weapon_ids: Vec<u64> = ctx.db.weapon_drop().iter().map(|w| w.id).collect();
    let bot_ids: Vec<u64> = ctx.db.bot_player().iter().map(|b| b.id).collect();
    let cooldown_ids: Vec<u64> = ctx
        .db
        .combat_hit_cooldown()
        .iter()
        .map(|row| row.id)
        .collect();

    for player_id in player_ids {
        ctx.db.player().id().delete(player_id);
    }
    for lobby_id in lobby_ids {
        ctx.db.lobby().id().delete(lobby_id);
    }
    for weapon_id in weapon_ids {
        ctx.db.weapon_drop().id().delete(weapon_id);
    }
    for bot_id in bot_ids {
        ctx.db.bot_player().id().delete(bot_id);
    }
    for cooldown_id in cooldown_ids {
        ctx.db.combat_hit_cooldown().id().delete(cooldown_id);
    }
    log::info!("Server state forcefully cleared");
}

#[spacetimedb::reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    let player_id = player_id_from_ctx(ctx);
    remove_player(ctx, player_id);
}
