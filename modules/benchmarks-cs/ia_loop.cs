using SpacetimeDB;

namespace Benchmarks;

public static partial class ia_loop
{
    [SpacetimeDB.Table]
    public partial struct Velocity(uint entity_id, float x, float y, float z)
    {
        [PrimaryKey]
        public uint entity_id = entity_id;
        public float x = x;
        public float y = y;
        public float z = z;
    }

    [SpacetimeDB.Table]
    public partial struct Position(uint entity_id, float x, float y, float z)
    {
        [PrimaryKey]
        public uint entity_id = entity_id;
        public float x = x;
        public float y = y;
        public float z = z;
        public float vx = x + 10.0f;
        public float vy = y + 20.0f;
        public float vz = z + 30.0f;
    }

    public static ulong MomentMilliseconds()
    {
        // Idk why Rust uses complicated math, but it seems like it should always return 1.
        return 1;
    }

    [SpacetimeDB.Type]
    public enum AgentAction
    {
        Inactive,
        Idle,
        Evading,
        Investigating,
        Retreating,
        Fighting,
    }

    [SpacetimeDB.Table]
    public partial struct GameEnemyAiAgentState(
        ulong entity_id,
        List<ulong> last_move_timestamps,
        ulong next_action_timestamp,
        AgentAction action
    )
    {
        [PrimaryKey]
        public ulong entity_id = entity_id;
        public List<ulong> last_move_timestamps = last_move_timestamps;
        public ulong next_action_timestamp = next_action_timestamp;
        public AgentAction action = action;
    }

    [SpacetimeDB.Table]
    public partial struct GameTargetableState(ulong entity_id, long quad)
    {
        [PrimaryKey]
        public ulong entity_id = entity_id;
        public long quad = quad;
    }

    [SpacetimeDB.Table]
    public partial struct GameLiveTargetableState(ulong entity_id, long quad)
    {
        [Unique]
        public ulong entity_id = entity_id;

        [Indexed]
        public long quad = quad;
    }

    [SpacetimeDB.Table]
    public partial struct GameMobileEntityState(
        ulong entity_id,
        int location_x,
        int location_y,
        ulong timestamp
    )
    {
        [PrimaryKey]
        public ulong entity_id = entity_id;

        [Indexed]
        public int location_x = location_x;
        public int location_y = location_y;
        public ulong timestamp = timestamp;
    }

    [SpacetimeDB.Table]
    public partial struct GameEnemyState(ulong entity_id, int herd_id)
    {
        [PrimaryKey]
        public ulong entity_id = entity_id;
        public int herd_id = herd_id;
    }

    [SpacetimeDB.Type]
    public partial struct SmallHexTile(int x, int z, uint dimension)
    {
        public int x = x;
        public int z = z;
        public uint dimension = dimension;
    }

    [SpacetimeDB.Table]
    public partial struct GameHerdCache(
        int id,
        uint dimension_id,
        int current_population,
        SmallHexTile location,
        int max_population,
        float spawn_eagerness,
        int roaming_distance
    )
    {
        [PrimaryKey]
        public int id = id;
        public uint dimension_id = dimension_id;
        public int current_population = current_population;
        public SmallHexTile location = location;
        public int max_population = max_population;
        public float spawn_eagerness = spawn_eagerness;
        public int roaming_distance = roaming_distance;
    }

    [SpacetimeDB.Reducer]
    public static void InsertBulkPosition(ReducerContext ctx, uint count)
    {
        for (uint id = 0; id < count; id++)
        {
            ctx.Db.Position.Insert(new(id, id, id + 5, id * 5));
        }
        Log.Info($"INSERT POSITION: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void InsertBulkVelocity(ReducerContext ctx, uint count)
    {
        for (uint id = 0; id < count; id++)
        {
            ctx.Db.Velocity.Insert(new(id, id, id + 5, id * 5));
        }
        Log.Info($"INSERT VELOCITY: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void update_position_all(ReducerContext ctx, uint expected)
    {
        uint count = 0;
        foreach (Position position in ctx.Db.Position.Iter())
        {
            Position newPosition = position;

            newPosition.x += position.vx;
            newPosition.y += position.vy;
            newPosition.z += position.vz;

            ctx.Db.Position.UpdateByentity_id(position.entity_id, newPosition);
            count++;
        }
        Log.Info($"UPDATE POSITION ALL: {expected}, processed: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void update_position_with_velocity(ReducerContext ctx, uint expected)
    {
        uint count = 0;
        foreach (Velocity velocity in ctx.Db.Velocity.Iter())
        {
            if (ctx.Db.Position.FindByentity_id(velocity.entity_id) is not { } position)
            {
                continue;
            }

            position.x += velocity.x;
            position.y += velocity.y;
            position.z += velocity.z;

            ctx.Db.Position.UpdateByentity_id(position.entity_id, position);
            count++;
        }
        Log.Info($"UPDATE POSITION BY VELOCITY: {expected}, processed: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void insert_world(ReducerContext ctx, ulong players)
    {
        for (ulong i = 0; i < players; i++)
        {
            ulong next_action_timestamp =
                (i & 2) == 2 ? MomentMilliseconds() + 2000 : MomentMilliseconds();

            ctx.Db.GameEnemyAiAgentState.Insert(
                new(i, [i, 0, i * 2], next_action_timestamp, AgentAction.Idle)
            );

            ctx.Db.GameLiveTargetableState.Insert(new(i, (long)i));

            ctx.Db.GameTargetableState.Insert(new(i, (long)i));

            ctx.Db.GameMobileEntityState.Insert(new(i, (int)i, (int)i, next_action_timestamp));

            ctx.Db.GameEnemyState.Insert(new(i, (int)i));

            ctx.Db.GameHerdCache.Insert(
                new(
                    (int)i,
                    (uint)i,
                    (int)(i * 2),
                    new SmallHexTile((int)i, (int)i, (uint)(i * 2)),
                    (int)(i * 4),
                    i,
                    (int)i
                )
            );
        }
        Log.Info($"INSERT WORLD PLAYERS: {players}");
    }

    public static List<GameTargetableState> GetTargetablesNearQuad(
        ReducerContext ctx,
        ulong entity_id,
        ulong num_players
    )
    {
        List<GameTargetableState> result = new(4);

        for (ulong id = entity_id; id < num_players; id++)
        {
            foreach (
                GameLiveTargetableState t in ctx.Db.GameLiveTargetableState.FilterByquad((long)id)
            )
            {
                result.Add(
                    ctx.Db.GameTargetableState.FindByentity_id(t.entity_id)
                        ?? throw new Exception("Identity not found")
                );
            }
        }

        return result;
    }

    private const int MAX_MOVE_TIMESTAMPS = 20;

    public static void MoveAgent(
        ReducerContext ctx,
        ref GameEnemyAiAgentState agent,
        SmallHexTile agent_coord,
        ulong current_time_ms
    )
    {
        ulong entity_id = agent.entity_id;

        GameEnemyState enemy =
            ctx.Db.GameEnemyState.FindByentity_id(entity_id)
            ?? throw new Exception("GameEnemyState Entity ID not found");
        ctx.Db.GameEnemyState.UpdateByentity_id(entity_id, enemy);

        agent.next_action_timestamp = current_time_ms + 2000;

        agent.last_move_timestamps.Add(current_time_ms);
        if (agent.last_move_timestamps.Count > MAX_MOVE_TIMESTAMPS)
        {
            agent.last_move_timestamps.RemoveAt(0);
        }

        GameTargetableState targetable =
            ctx.Db.GameTargetableState.FindByentity_id(entity_id)
            ?? throw new Exception("GameTargetableState Entity ID not found");
        int new_hash = targetable.quad.GetHashCode();
        targetable.quad = new_hash;
        ctx.Db.GameTargetableState.UpdateByentity_id(entity_id, targetable);

        if (ctx.Db.GameLiveTargetableState.FindByentity_id(entity_id) is not null)
        {
            ctx.Db.GameLiveTargetableState.UpdateByentity_id(entity_id, new(entity_id, new_hash));
        }

        GameMobileEntityState mobile_entity =
            ctx.Db.GameMobileEntityState.FindByentity_id(entity_id)
            ?? throw new Exception("GameMobileEntityState Entity ID not found");
        mobile_entity.location_x += 1;
        mobile_entity.location_y += 1;
        mobile_entity.timestamp = MomentMilliseconds();

        ctx.Db.GameEnemyAiAgentState.UpdateByentity_id(entity_id, agent);

        ctx.Db.GameMobileEntityState.UpdateByentity_id(entity_id, mobile_entity);
    }

    public static void AgentLoop(
        ReducerContext ctx,
        GameEnemyAiAgentState agent,
        GameTargetableState agent_targetable,
        List<GameTargetableState> surrounding_agents,
        ulong current_time_ms
    )
    {
        ulong entity_id = agent.entity_id;

        IEnumerable<GameMobileEntityState> coordinates =
            ctx.Db.GameMobileEntityState.FilterByentity_id(entity_id)
            ?? throw new Exception("GameMobileEntityState Entity ID not found");

        GameEnemyState agent_entity =
            ctx.Db.GameEnemyState.FindByentity_id(entity_id)
            ?? throw new Exception("GameEnemyState Entity ID not found");

        GameHerdCache agent_herd =
            ctx.Db.GameHerdCache.FindByid(agent_entity.herd_id)
            ?? throw new Exception("GameHerdCache Entity ID not found");

        SmallHexTile agent_herd_coordinates = agent_herd.location;

        MoveAgent(ctx, ref agent, agent_herd_coordinates, current_time_ms);
    }

    [SpacetimeDB.Reducer]
    public static void game_loop_enemy_ia(ReducerContext ctx, ulong players)
    {
        uint count = 0;
        ulong current_time_ms = MomentMilliseconds();

        foreach (GameEnemyAiAgentState agent in ctx.Db.GameEnemyAiAgentState.Iter())
        {
            if (agent.next_action_timestamp > current_time_ms)
            {
                continue;
            }

            GameTargetableState agent_targetable =
                ctx.Db.GameTargetableState.FindByentity_id(agent.entity_id)
                ?? throw new Exception("No TargetableState for AgentState entity");

            List<GameTargetableState> surrounding_agents = GetTargetablesNearQuad(
                ctx,
                agent_targetable.entity_id,
                players
            );

            GameEnemyAiAgentState newAgent = agent with { action = AgentAction.Fighting };

            AgentLoop(ctx, newAgent, agent_targetable, surrounding_agents, current_time_ms);

            count++;
        }

        Log.Info($"ENEMY IA LOOP PLAYERS: {players}, processed: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void init_game_ia_loop(ReducerContext ctx, uint initial_load)
    {
        Load load = new(initial_load);

        InsertBulkPosition(ctx, load.biggest_table);
        InsertBulkVelocity(ctx, load.big_table);
        update_position_all(ctx, load.biggest_table);
        update_position_with_velocity(ctx, load.big_table);

        insert_world(ctx, load.num_players);
    }

    [SpacetimeDB.Reducer]
    public static void run_game_ia_loop(ReducerContext ctx, uint initial_load)
    {
        Load load = new(initial_load);

        game_loop_enemy_ia(ctx, load.num_players);
    }
}
