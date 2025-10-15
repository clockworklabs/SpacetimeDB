// STDB module used for benchmarks based on "realistic" workloads we are focusing in improving.

import { blackBox } from './load';
import { spacetimedb, type Position, } from './schema';
import {
    schema,
    table,
    t,
    type InferTypeOfRow,
} from 'spacetimedb/server';


function newPosition(entity_id: number, x: number, y: number, z: number): Position {
    return {
        entity_id,
        x,
        y,
        z,
        vx: x + 10.0,
        vy: y + 20.0,
        vz: z + 30.0,
    };
}

function momentMilliseconds(): bigint {
    return 1n;
}

function calculateHash(t: number): number {
    // Whatever, here's a hash function I guess.
    return (t >> 16) ^ t;
}

const insertBulkPosition = (ctx, { count }) => {
    for (let id = 0; id < count; id++) {
        ctx.db.position.insert(newPosition(id, id, id + 5, id * 5));
    }
    console.log(`INSERT POSITION: ${count}`);
};
spacetimedb.reducer('insert_bulk_position', { count: t.u32() }, insertBulkPosition);

const insertBulkVelocity = (ctx, { count }) => {
    for (let id = 0; id < count; id++) {
        ctx.db.velocity.insert(newVelocity(id, id, id + 5, id * 5));
    }
    console.log(`INSERT VELOCITY: ${count}`);
};
spacetimedb.reducer('insert_bulk_velocity', { count: t.u32() }, insertBulkVelocity);

const updatePositionAll = (ctx, { expected }) => {
    let count = 0;
    for (const position of ctx.db.position.iter()) {
        position.x += position.vx;
        position.y += position.vy;
        position.z += position.vz;

        ctx.db.position.entity_id.update(position);
        count += 1;
    }
    console.log(`UPDATE POSITION ALL: ${expected}, processed: ${count}`);
};
spacetimedb.reducer('update_position_all', { expected: t.u32() }, updatePositionAll);

const updatePositionWithVelocity = (ctx, { expected }) => {
    let count = 0;
    for (const velocity of ctx.db.velocity.iter()) {
        let position = ctx.db.position.entity_id.find(velocity.entity_id);
        if (position == null) {
            continue;
        }

        position.x += velocity.x;
        position.y += velocity.y;
        position.z += velocity.z;

        ctx.db.position.entity_id.update(position);
        count += 1;
    }
    console.log(`UPDATE POSITION BY VELOCITY: ${expected}, processed: ${count}`);
};
spacetimedb.reducer('update_position_by_velocity', { expected: t.u32() }, updatePositionWithVelocity);

const insertWorld = (ctx, { players }) => {
    for (let i = 0; i < players; i++) {
        const id = i;
        const nextActionTimestamp = (i & 2 == 2) ? momentMilliseconds() + 2000 : momentMilliseconds();

        ctx.db.game_enemy_ai_agent_state.insert({
            entity_id: id,
            next_action_timestamp: nextActionTimestamp,
            last_move_timestamps: [id, 0, id * 2],
            action: { tag: 'Idle', value: {} },
        });

        ctx.db.game_live_targetable_state.insert({
            entity_id: id,
            quad: id,
        });

        ctx.db.game_targetable_state.insert({
            entity_id: id,
            quad: id,
        });

        ctx.db.game_mobile_entity_state.insert({
            entity_id: id,
            location_x: id,
            location_y: id,
            timestamp: nextActionTimestamp,
        });

        ctx.db.game_enemy_state.insert({
            entity_id: id,
            herd_id: id,
        });

        ctx.db.game_herd_cache.insert({
            id,
            dimension_id: id,
            max_population: id * 4,
            spawn_eagerness: id,
            roaming_distance: id,
            location: {
                x: id,
                z: id,
                dimension: id * 2,
            },
        });
    }
    console.log(`INSERT WORLD PLAYERS: ${players}`);
};
spacetimedb.reducer('insert_world', { players: t.u64() }, insertWorld);

function getTargetablesNearQuad(ctx, entityId: number, numPlayers: number): GameTargetableState[] {
    let result = [];
    for (let id = entityId; id < numPlayers; id++) {
        for (const liveTargetable of ctx.db.game_live_targetable_state.quad.filter(id)) {
            const targetable = ctx.db.game_targetable_state.entity_id.find(liveTargetable.entity_id);
            if (targetable == null) {
                throw new Error("Identity not found");
            }
            result.push(targetable);
        }
    }
    return result;
}

const MAX_MOVE_TIMESTAMPS = 20;

function moveAgent(ctx, agent: GameEnemyAiAgentState, agentCoord: SmallHexTile, currentTimeMs: number) {
    const entityId = agent.entity_id;

    const enemy = ctx.db.game_enemy_state.entity_id.find(entityId);
    if (enemy == null) {
        throw new Error("GameEnemyState Entity ID not found");
    }
    ctx.db.game_enemy_state.entity_id.update(enemy);

    agent.next_action_timestamp = currentTimeMs + 2000;

    agent.last_move_timestamps.push(currentTimeMs);
    if (agent.last_move_timestamps.length > MAX_MOVE_TIMESTAMPS) {
        agent.last_move_timestamps.splice(0, 1);
    }

    let targetable = ctx.db.game_targetable_state.entity_id.find(entityId);
    if (targetable == null) {
        throw new Error("GameTargetableState Entity ID not found");
    }
    let newHash = calculateHash(targetable.quad);
    targetable.quad = newHash;
    ctx.db.game_targetable_state.entity_id.update(targetable);

    if (ctx.db.game_live_targetable_state.entity_id.find(entityId) != null) {
        ctx.db.game_live_targetable_state.entity_id.update({
            entity_id: entityId,
            quad: newHash,
        });
    }

    const mobileEntityRes = ctx.db.game_mobile_entity_state.entity_id.find(entityId);
    if (mobileEntityRes == null) {
        throw new Error("GameMobileEntityState Entity ID not found");
    }
    const mobileEntity = {
        entity_id: entityId,
        location_x: mobileEntityRes.location_x + 1,
        location_y: mobileEntityRes.location_y + 1,
        timestamp: agent.next_action_timestamp,
    };

    ctx.db.game_enemy_ai_agent_state.entity_id.update(agent);

    ctx.db.game_mobile_entity_state.entity_id.update(mobileEntity);
}

function agentLoop(
    ctx,
    agent: GameEnemyAiAgentState,
    agentTargetable: GameTargetableState,
    surroundingAgents: GameTargetableState[],
    currentTimeMs: number,
) {
    const entityId = agent.entity_id;

    const coordinates = ctx.db.game_mobile_entity_state.entity_id.find(entityId);
    if (coordinates == null) {
        throw new Error("GameMobileEntityState Entity ID not found");
    }

    const agentEntity = ctx.db.game_enemy_state.entity_id.find(entityId);
    if (agentEntity == null) {
        throw new Error("GameEnemyState Entity ID not found");
    }

    const agentHerd = ctx.db.game_herd_cache.id.find(agentEntity.herd_id);
    if (agentHerd == null) {
        throw new Error("GameHerdCache Entity ID not found");
    }

    const agentHerdCoordinates = agentHerd.location;

    moveAgent(ctx, agent, agentHerdCoordinates, currentTimeMs);
}

const gameLoopEnemyIa = (ctx, { players }) => {
    let count = 0;
    let currentTimeMs = momentMilliseconds();

    for (const agent of ctx.db.game_enemy_ai_agent_state.iter()) {
        const agentTargetable = ctx.db.game_targetable_state.entity_id.find(agent.entity_id);
        if (agentTargetable == null) {
            throw new Error("No TargetableState for AgentState entity");
        }

        let surroundingAgents = getTargetablesNearQuad(ctx, agentTargetable.entity_id, players);

        agent.action = { tag: 'Fighting', value: {} };

        agentLoop(ctx, agent, agentTargetable, surroundingAgents, currentTimeMs);

        count += 1;
    }

    console.log(`ENEMY IA LOOP PLAYERS: ${players}, processed: ${count}`);
};
spacetimedb.reducer('game_loop_enemy_ia', { players: t.u64() }, gameLoopEnemyIa);

const initGameIaLoop = (ctx, { initial_load }) => {
    const load = newLoad(initial_load);

    insertBulkPosition(ctx, load.biggestTable);
    insertBulkVelocity(ctx, load.bigTable);
    updatePositionAll(ctx, load.biggestTable);
    updatePositionWithVelocity(ctx, load.bigTable);

    insertWorld(ctx, load.numPlayers);
};
spacetimedb.reducer('init_game_ia_loop', { initial_load: t.u32() }, initGameIaLoop);

const runGameIaLoop = (ctx, { initial_load }) => {
    const load = newLoad(initial_load);

    gameLoopEnemyIa(ctx, load.numPlayers);
};
spacetimedb.reducer('run_game_ia_loop', { initial_load: t.u32() }, runGameIaLoop);
