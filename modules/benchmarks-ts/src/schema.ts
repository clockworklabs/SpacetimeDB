import { circleTable, entityTable, foodTable } from "./circles";
import { gameEnemyAiAgentStateTable, gameEnemyStateTable, gameHerdCacheTable, gameLiveTargetableStateTable, gameTargetableStateTable, positionTable, velocityTable } from "./ia_loop";
import { btree_each_column_u32_u64_strTable, btree_each_column_u32_u64_u64Table, no_index_u32_u64_strTable, no_index_u32_u64_u64Table, unique_0_u32_u64_strTable, unique_0_u32_u64_u64Table } from "./synthetic";

import { schema } from 'spacetimedb/server';

export const spacetimedb = schema(
    circleTable,
    entityTable,
    foodTable,
    unique_0_u32_u64_strTable,
    no_index_u32_u64_strTable,
    btree_each_column_u32_u64_strTable,
    unique_0_u32_u64_u64Table,
    no_index_u32_u64_u64Table,
    btree_each_column_u32_u64_u64Table,
    velocityTable,
    positionTable,
    gameEnemyAiAgentStateTable,
    gameTargetableStateTable,
    gameLiveTargetableStateTable,
    gameEnemyStateTable,
    gameHerdCacheTable,
);
