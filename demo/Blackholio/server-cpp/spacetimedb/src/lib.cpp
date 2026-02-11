#include <spacetimedb.h>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

using namespace SpacetimeDB;
using SpacetimeDB::Private;
using SpacetimeDB::Public;

constexpr int32_t START_PLAYER_MASS = 15;
constexpr int32_t START_PLAYER_SPEED = 10;
constexpr int32_t FOOD_MASS_MIN = 2;
constexpr int32_t FOOD_MASS_MAX = 4;
constexpr size_t TARGET_FOOD_COUNT = 600;
constexpr float MINIMUM_SAFE_MASS_RATIO = 0.85f;

constexpr int32_t MIN_MASS_TO_SPLIT = START_PLAYER_MASS * 2;
constexpr int32_t MAX_CIRCLES_PER_PLAYER = 16;
constexpr float SPLIT_RECOMBINE_DELAY_SEC = 5.0f;
constexpr float SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC = 2.0f;
constexpr float ALLOWED_SPLIT_CIRCLE_OVERLAP_PCT = 0.9f;
constexpr float SELF_COLLISION_SPEED = 0.05f; // 1 == instantly separate circles. less means separation takes time

struct DbVector2 {
    float x;
    float y;

    DbVector2() : x(0.0f), y(0.0f) {}
    DbVector2(float x_in, float y_in) : x(x_in), y(y_in) {}

    float sqr_magnitude() const {
        return x * x + y * y;
    }

    float magnitude() const {
        return std::sqrt(sqr_magnitude());
    }

    DbVector2 normalized() const {
        float mag = magnitude();
        if (mag == 0.0f) {
            return DbVector2(0.0f, 0.0f);
        }
        return DbVector2(x / mag, y / mag);
    }
};
SPACETIMEDB_STRUCT(DbVector2, x, y)

inline DbVector2 operator+(const DbVector2& a, const DbVector2& b) {
    return DbVector2(a.x + b.x, a.y + b.y);
}

inline DbVector2 operator-(const DbVector2& a, const DbVector2& b) {
    return DbVector2(a.x - b.x, a.y - b.y);
}

inline DbVector2& operator+=(DbVector2& a, const DbVector2& b) {
    a.x += b.x;
    a.y += b.y;
    return a;
}

inline DbVector2& operator-=(DbVector2& a, const DbVector2& b) {
    a.x -= b.x;
    a.y -= b.y;
    return a;
}

inline DbVector2 operator*(const DbVector2& v, float scalar) {
    return DbVector2(v.x * scalar, v.y * scalar);
}

inline DbVector2 operator*(float scalar, const DbVector2& v) {
    return DbVector2(v.x * scalar, v.y * scalar);
}

inline DbVector2 operator/(const DbVector2& v, float scalar) {
    if (scalar == 0.0f) {
        return DbVector2(0.0f, 0.0f);
    }
    return DbVector2(v.x / scalar, v.y / scalar);
}

struct Config {
    int32_t id;
    int64_t world_size;
};
SPACETIMEDB_STRUCT(Config, id, world_size)
SPACETIMEDB_TABLE(Config, config, Public)
FIELD_PrimaryKey(config, id)

struct Entity {
    int32_t entity_id;
    DbVector2 position;
    int32_t mass;
};
SPACETIMEDB_STRUCT(Entity, entity_id, position, mass)
SPACETIMEDB_TABLE(Entity, entity, Public)
FIELD_PrimaryKeyAutoInc(entity, entity_id)
SPACETIMEDB_TABLE(Entity, logged_out_entity, Private)
FIELD_PrimaryKeyAutoInc(logged_out_entity, entity_id)

struct Circle {
    int32_t entity_id;
    int32_t player_id;
    DbVector2 direction;
    float speed;
    Timestamp last_split_time;
};
SPACETIMEDB_STRUCT(Circle, entity_id, player_id, direction, speed, last_split_time)
SPACETIMEDB_TABLE(Circle, circle, Public)
FIELD_PrimaryKey(circle, entity_id)
FIELD_Index(circle, player_id)
SPACETIMEDB_TABLE(Circle, logged_out_circle, Private)
FIELD_PrimaryKey(logged_out_circle, entity_id)
FIELD_Index(logged_out_circle, player_id)

struct Player {
    Identity identity;
    int32_t player_id;
    std::string name;
};
SPACETIMEDB_STRUCT(Player, identity, player_id, name)
SPACETIMEDB_TABLE(Player, player, Public)
FIELD_PrimaryKey(player, identity)
FIELD_UniqueAutoInc(player, player_id)
SPACETIMEDB_TABLE(Player, logged_out_player, Private)
FIELD_PrimaryKey(logged_out_player, identity)
FIELD_UniqueAutoInc(logged_out_player, player_id)

struct Food {
    int32_t entity_id;
};
SPACETIMEDB_STRUCT(Food, entity_id)
SPACETIMEDB_TABLE(Food, food, Public)
FIELD_PrimaryKey(food, entity_id)

struct MoveAllPlayersTimer {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
};
SPACETIMEDB_STRUCT(MoveAllPlayersTimer, scheduled_id, scheduled_at)
SPACETIMEDB_TABLE(MoveAllPlayersTimer, move_all_players_timer, Private)
FIELD_PrimaryKeyAutoInc(move_all_players_timer, scheduled_id)
SPACETIMEDB_SCHEDULE(move_all_players_timer, 1, move_all_players)

struct SpawnFoodTimer {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
};
SPACETIMEDB_STRUCT(SpawnFoodTimer, scheduled_id, scheduled_at)
SPACETIMEDB_TABLE(SpawnFoodTimer, spawn_food_timer, Private)
FIELD_PrimaryKeyAutoInc(spawn_food_timer, scheduled_id)
SPACETIMEDB_SCHEDULE(spawn_food_timer, 1, spawn_food)

struct CircleDecayTimer {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
};
SPACETIMEDB_STRUCT(CircleDecayTimer, scheduled_id, scheduled_at)
SPACETIMEDB_TABLE(CircleDecayTimer, circle_decay_timer, Private)
FIELD_PrimaryKeyAutoInc(circle_decay_timer, scheduled_id)
SPACETIMEDB_SCHEDULE(circle_decay_timer, 1, circle_decay)

struct CircleRecombineTimer {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
    int32_t player_id;
};
SPACETIMEDB_STRUCT(CircleRecombineTimer, scheduled_id, scheduled_at, player_id)
SPACETIMEDB_TABLE(CircleRecombineTimer, circle_recombine_timer, Private)
FIELD_PrimaryKeyAutoInc(circle_recombine_timer, scheduled_id)
SPACETIMEDB_SCHEDULE(circle_recombine_timer, 1, circle_recombine)

struct ConsumeEntityTimer {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
    int32_t consumed_entity_id;
    int32_t consumer_entity_id;
};
SPACETIMEDB_STRUCT(ConsumeEntityTimer, scheduled_id, scheduled_at, consumed_entity_id, consumer_entity_id)
SPACETIMEDB_TABLE(ConsumeEntityTimer, consume_entity_timer, Private)
FIELD_PrimaryKeyAutoInc(consume_entity_timer, scheduled_id)
SPACETIMEDB_SCHEDULE(consume_entity_timer, 1, consume_entity)

static float mass_to_radius(int32_t mass) {
    return std::sqrt(static_cast<float>(mass));
}

static float mass_to_max_move_speed(int32_t mass) {
    return 2.0f * static_cast<float>(START_PLAYER_SPEED)
        / (1.0f + std::sqrt(static_cast<float>(mass) / static_cast<float>(START_PLAYER_MASS)));
}

static bool is_overlapping(const Entity& a, const Entity& b) {
    float dx = a.position.x - b.position.x;
    float dy = a.position.y - b.position.y;
    float distance_sq = dx * dx + dy * dy;

    float radius_a = mass_to_radius(a.mass);
    float radius_b = mass_to_radius(b.mass);
    float max_radius = std::max(radius_a, radius_b);

    return distance_sq <= max_radius * max_radius;
}

static Outcome<Entity> spawn_circle_at(
    ReducerContext& ctx,
    int32_t player_id,
    int32_t mass,
    const DbVector2& position,
    const Timestamp& timestamp
) {
    Entity new_entity{0, position, mass};
    Entity inserted_entity = ctx.db[entity].insert(new_entity);
    Circle new_circle{
        inserted_entity.entity_id,
        player_id,
        DbVector2(0.0f, 1.0f),
        0.0f,
        timestamp
    };
    ctx.db[circle].insert(new_circle);

    return Ok(inserted_entity);
}

static Outcome<Entity> spawn_player_initial_circle(ReducerContext& ctx, int32_t player_id) {
    auto config_opt = ctx.db[config_id].find(0);
    if (!config_opt.has_value()) {
        return Err<Entity>("Config not found");
    }

    auto& rng = ctx.rng();
    float world_size = static_cast<float>(config_opt->world_size);
    float player_start_radius = mass_to_radius(START_PLAYER_MASS);
    float x = rng.gen_range(player_start_radius, world_size - player_start_radius);
    float y = rng.gen_range(player_start_radius, world_size - player_start_radius);
    return spawn_circle_at(ctx, player_id, START_PLAYER_MASS, DbVector2(x, y), ctx.timestamp);
}

static void schedule_consume_entity(ReducerContext& ctx, int32_t consumer_id, int32_t consumed_id) {
    ConsumeEntityTimer timer{
        0,
        ScheduleAt::time(ctx.timestamp),
        consumed_id,
        consumer_id
    };
    ctx.db[consume_entity_timer].insert(timer);
}

static Outcome<void> destroy_entity(ReducerContext& ctx, int32_t entity_id) {
    (void)ctx.db[food_entity_id].delete_by_key(entity_id);
    (void)ctx.db[circle_entity_id].delete_by_key(entity_id);
    (void)ctx.db[entity_entity_id].delete_by_key(entity_id);
    return Ok();
}

static DbVector2 calculate_center_of_mass(const std::vector<Entity>& entities) {
    int32_t total_mass = 0;
    DbVector2 center_of_mass(0.0f, 0.0f);
    for (const auto& entity_row : entities) {
        total_mass += entity_row.mass;
        center_of_mass += entity_row.position * static_cast<float>(entity_row.mass);
    }
    if (total_mass == 0) {
        return DbVector2(0.0f, 0.0f);
    }
    return center_of_mass / static_cast<float>(total_mass);
}

SPACETIMEDB_INIT(init, ReducerContext ctx) {
    LOG_INFO("Initializing...");
    ctx.db[config].insert(Config{0, 1000});

    ctx.db[circle_decay_timer].insert(
        CircleDecayTimer{0, ScheduleAt::interval(TimeDuration::from_seconds(5))}
    );

    ctx.db[spawn_food_timer].insert(
        SpawnFoodTimer{0, ScheduleAt::interval(TimeDuration::from_millis(500))}
    );

    ctx.db[move_all_players_timer].insert(
        MoveAllPlayersTimer{0, ScheduleAt::interval(TimeDuration::from_millis(50))}
    );

    return Ok();
}

SPACETIMEDB_CLIENT_CONNECTED(connect, ReducerContext ctx) {
    auto logged_out = ctx.db[logged_out_player_identity].find(ctx.sender);
    if (logged_out.has_value()) {
        ctx.db[player].insert(logged_out.value());
        (void)ctx.db[logged_out_player_identity].delete_by_key(logged_out->identity);

        auto logged_out_circles = ctx.db[logged_out_circle_player_id].filter(logged_out->player_id);
        for (const auto& circle_row : logged_out_circles) {
            auto entity_opt = ctx.db[logged_out_entity_entity_id].find(circle_row.entity_id);
            if (!entity_opt.has_value()) {
                return Err("Could not find logged out entity");
            }

            ctx.db[entity].insert(entity_opt.value());
            (void)ctx.db[logged_out_entity_entity_id].delete_by_key(entity_opt->entity_id);

            ctx.db[circle].insert(circle_row);
            (void)ctx.db[logged_out_circle_entity_id].delete_by_key(circle_row.entity_id);
        }
    } else {
        Player new_player{ctx.sender, 0, std::string()};
        ctx.db[player].insert(new_player);
    }
    return Ok();
}

SPACETIMEDB_CLIENT_DISCONNECTED(disconnect, ReducerContext ctx) {
    auto player_opt = ctx.db[player_identity].find(ctx.sender);
    if (!player_opt.has_value()) {
        return Err("Player not found");
    }
    Player player_row = player_opt.value();
    int32_t player_id = player_row.player_id;

    ctx.db[logged_out_player].insert(player_row);
    (void)ctx.db[player_identity].delete_by_key(player_row.identity);

    auto circles = ctx.db[circle_player_id].filter(player_id);
    for (const auto& circle_row : circles) {
        auto entity_opt = ctx.db[entity_entity_id].find(circle_row.entity_id);
        if (!entity_opt.has_value()) {
            return Err("Could not find circle entity");
        }

        ctx.db[logged_out_entity].insert(entity_opt.value());
        (void)ctx.db[entity_entity_id].delete_by_key(entity_opt->entity_id);

        ctx.db[logged_out_circle].insert(circle_row);
        (void)ctx.db[circle_entity_id].delete_by_key(circle_row.entity_id);
    }

    return Ok();
}

SPACETIMEDB_REDUCER(enter_game, ReducerContext ctx, std::string name) {
    LOG_INFO("Creating player with name " + name);
    auto player_opt = ctx.db[player_identity].find(ctx.sender);
    if (!player_opt.has_value()) {
        return Err("Player not found");
    }

    Player player_row = player_opt.value();
    player_row.name = std::move(name);
    (void)ctx.db[player_identity].update(player_row);

    auto spawn_result = spawn_player_initial_circle(ctx, player_row.player_id);
    if (spawn_result.is_err()) {
        return Err(spawn_result.error());
    }

    return Ok();
}

SPACETIMEDB_REDUCER(respawn, ReducerContext ctx) {
    auto player_opt = ctx.db[player_identity].find(ctx.sender);
    if (!player_opt.has_value()) {
        return Err("No such player found");
    }

    auto spawn_result = spawn_player_initial_circle(ctx, player_opt->player_id);
    if (spawn_result.is_err()) {
        return Err(spawn_result.error());
    }

    return Ok();
}

SPACETIMEDB_REDUCER(suicide, ReducerContext ctx) {
    auto player_opt = ctx.db[player_identity].find(ctx.sender);
    if (!player_opt.has_value()) {
        return Err("No such player found");
    }

    auto circles = ctx.db[circle_player_id].filter(player_opt->player_id);
    for (const auto& circle_row : circles) {
        auto destroy_result = destroy_entity(ctx, circle_row.entity_id);
        if (destroy_result.is_err()) {
            return Err(destroy_result.error());
        }
    }
    return Ok();
}

SPACETIMEDB_REDUCER(update_player_input, ReducerContext ctx, DbVector2 direction) {
    auto player_opt = ctx.db[player_identity].find(ctx.sender);
    if (!player_opt.has_value()) {
        return Err("Player not found");
    }

    auto circles = ctx.db[circle_player_id].filter(player_opt->player_id);
    for (auto circle_row : circles) {
        circle_row.direction = direction.normalized();
        circle_row.speed = std::clamp(direction.magnitude(), 0.0f, 1.0f);
        (void)ctx.db[circle_entity_id].update(circle_row);
    }
    return Ok();
}

SPACETIMEDB_REDUCER(move_all_players, ReducerContext ctx, MoveAllPlayersTimer) {
    auto config_opt = ctx.db[config_id].find(0);
    if (!config_opt.has_value()) {
        return Err("Config not found");
    }
    float world_size = static_cast<float>(config_opt->world_size);

    std::unordered_map<int32_t, DbVector2> circle_directions;
    circle_directions.reserve(static_cast<size_t>(ctx.db[circle].count()));
    for (const auto& circle_row : ctx.db[circle]) {
        circle_directions.emplace(circle_row.entity_id, circle_row.direction * circle_row.speed);
    }

    for (const auto& player_row : ctx.db[player]) {
        auto circles_range = ctx.db[circle_player_id].filter(player_row.player_id);
        std::vector<Circle> circles;
        circles.reserve(circles_range.size());
        for (const auto& circle_row : circles_range) {
            circles.push_back(circle_row);
        }

        std::vector<Entity> player_entities;
        player_entities.reserve(circles.size());
        for (const auto& circle_row : circles) {
            auto entity_opt = ctx.db[entity_entity_id].find(circle_row.entity_id);
            if (!entity_opt.has_value()) {
                return Err("Circle has no entity");
            }
            player_entities.push_back(entity_opt.value());
        }

        if (player_entities.size() <= 1) {
            continue;
        }
        float count = static_cast<float>(player_entities.size());

        for (size_t i = 0; i < player_entities.size(); ++i) {
            const Circle& circle_i = circles[i];
            float time_since_split =
                static_cast<float>(ctx.timestamp.duration_since(circle_i.last_split_time).micros())
                / 1000000.0f;
            float time_before_recombining =
                std::max(SPLIT_RECOMBINE_DELAY_SEC - time_since_split, 0.0f);
            if (time_before_recombining > SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC) {
                continue;
            }

            const Entity& entity_i = player_entities[i];
            for (size_t j = 0; j < player_entities.size(); ++j) {
                if (j == i) {
                    continue;
                }
                const Entity& entity_j = player_entities[j];
                DbVector2 diff = entity_i.position - entity_j.position;
                float distance_sqr = diff.sqr_magnitude();
                if (distance_sqr <= 0.0001f) {
                    diff = DbVector2(1.0f, 0.0f);
                    distance_sqr = 1.0f;
                }
                float radius_sum = mass_to_radius(entity_i.mass) + mass_to_radius(entity_j.mass);
                if (distance_sqr > radius_sum * radius_sum) {
                    float gravity_multiplier =
                        1.0f - time_before_recombining / SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC;
                    DbVector2 vec = diff.normalized()
                        * (radius_sum - std::sqrt(distance_sqr))
                        * gravity_multiplier
                        * 0.05f
                        / count;
                    auto it_i = circle_directions.find(entity_i.entity_id);
                    auto it_j = circle_directions.find(entity_j.entity_id);
                    if (it_i != circle_directions.end()) {
                        it_i->second += vec / 2.0f;
                    }
                    if (it_j != circle_directions.end()) {
                        it_j->second -= vec / 2.0f;
                    }
                }
            }
        }

        for (size_t i = 0; i < player_entities.size(); ++i) {
            const Entity& entity_i = player_entities[i];
            for (size_t j = i + 1; j < player_entities.size(); ++j) {
                const Entity& entity_j = player_entities[j];
                DbVector2 diff = entity_i.position - entity_j.position;
                float distance_sqr = diff.sqr_magnitude();
                if (distance_sqr <= 0.0001f) {
                    diff = DbVector2(1.0f, 0.0f);
                    distance_sqr = 1.0f;
                }
                float radius_sum = mass_to_radius(entity_i.mass) + mass_to_radius(entity_j.mass);
                float radius_sum_multiplied = radius_sum * ALLOWED_SPLIT_CIRCLE_OVERLAP_PCT;
                if (distance_sqr < radius_sum_multiplied * radius_sum_multiplied) {
                    DbVector2 vec = diff.normalized()
                        * (radius_sum - std::sqrt(distance_sqr))
                        * SELF_COLLISION_SPEED;
                    auto it_i = circle_directions.find(entity_i.entity_id);
                    auto it_j = circle_directions.find(entity_j.entity_id);
                    if (it_i != circle_directions.end()) {
                        it_i->second += vec / 2.0f;
                    }
                    if (it_j != circle_directions.end()) {
                        it_j->second -= vec / 2.0f;
                    }
                }
            }
        }
    }

    for (const auto& circle_row : ctx.db[circle]) {
        auto entity_opt = ctx.db[entity_entity_id].find(circle_row.entity_id);
        if (!entity_opt.has_value()) {
            continue;
        }
        Entity circle_entity = entity_opt.value();
        float circle_radius = mass_to_radius(circle_entity.mass);

        DbVector2 direction(0.0f, 0.0f);
        auto direction_it = circle_directions.find(circle_row.entity_id);
        if (direction_it != circle_directions.end()) {
            direction = direction_it->second;
        }
        DbVector2 new_pos = circle_entity.position + direction * mass_to_max_move_speed(circle_entity.mass);

        float min = circle_radius;
        float max = world_size - circle_radius;
        circle_entity.position.x = std::clamp(new_pos.x, min, max);
        circle_entity.position.y = std::clamp(new_pos.y, min, max);
        (void)ctx.db[entity_entity_id].update(circle_entity);
    }

    std::unordered_map<int32_t, Entity> entities;
    entities.reserve(static_cast<size_t>(ctx.db[entity].count()));
    for (const auto& entity_row : ctx.db[entity]) {
        entities.emplace(entity_row.entity_id, entity_row);
    }

    for (const auto& circle_row : ctx.db[circle]) {
        auto entity_it = entities.find(circle_row.entity_id);
        if (entity_it == entities.end()) {
            continue;
        }
        const Entity& circle_entity = entity_it->second;

        for (const auto& other_pair : entities) {
            const Entity& other_entity = other_pair.second;
            if (other_entity.entity_id == circle_entity.entity_id) {
                continue;
            }

            if (is_overlapping(circle_entity, other_entity)) {
                auto other_circle = ctx.db[circle_entity_id].find(other_entity.entity_id);
                if (other_circle.has_value()) {
                    if (other_circle->player_id != circle_row.player_id) {
                        float mass_ratio =
                            static_cast<float>(other_entity.mass) / static_cast<float>(circle_entity.mass);
                        if (mass_ratio < MINIMUM_SAFE_MASS_RATIO) {
                            schedule_consume_entity(ctx, circle_entity.entity_id, other_entity.entity_id);
                        }
                    }
                } else {
                    schedule_consume_entity(ctx, circle_entity.entity_id, other_entity.entity_id);
                }
            }
        }
    }

    return Ok();
}

SPACETIMEDB_REDUCER(consume_entity, ReducerContext ctx, ConsumeEntityTimer request) {
    auto consumed_opt = ctx.db[entity_entity_id].find(request.consumed_entity_id);
    if (!consumed_opt.has_value()) {
        return Err("Consumed entity doesn't exist");
    }
    auto consumer_opt = ctx.db[entity_entity_id].find(request.consumer_entity_id);
    if (!consumer_opt.has_value()) {
        return Err("Consumer entity doesn't exist");
    }

    Entity consumed_entity = consumed_opt.value();
    Entity consumer_entity = consumer_opt.value();
    consumer_entity.mass += consumed_entity.mass;

    auto destroy_result = destroy_entity(ctx, consumed_entity.entity_id);
    if (destroy_result.is_err()) {
        return Err(destroy_result.error());
    }
    (void)ctx.db[entity_entity_id].update(consumer_entity);

    return Ok();
}

SPACETIMEDB_REDUCER(player_split, ReducerContext ctx) {
    auto player_opt = ctx.db[player_identity].find(ctx.sender);
    if (!player_opt.has_value()) {
        return Err("Sender has no player");
    }

    auto circles = ctx.db[circle_player_id].filter(player_opt->player_id);
    int32_t circle_count = static_cast<int32_t>(ctx.db[circle_player_id].filter(player_opt->player_id).size());
    if (circle_count >= MAX_CIRCLES_PER_PLAYER) {
        LOG_WARN("Player has max circles already");
        return Ok();
    }

    for (auto circle_row : circles) {
        auto entity_opt = ctx.db[entity_entity_id].find(circle_row.entity_id);
        if (!entity_opt.has_value()) {
            return Err("Circle has no entity");
        }
        Entity circle_entity = entity_opt.value();
        if (circle_entity.mass >= MIN_MASS_TO_SPLIT * 2) {
            int32_t half_mass = circle_entity.mass / 2;
            auto spawn_result = spawn_circle_at(
                ctx,
                circle_row.player_id,
                half_mass,
                circle_entity.position + circle_row.direction,
                ctx.timestamp
            );
            if (spawn_result.is_err()) {
                return Err(spawn_result.error());
            }

            circle_entity.mass -= half_mass;
            circle_row.last_split_time = ctx.timestamp;
            (void)ctx.db[circle_entity_id].update(circle_row);
            (void)ctx.db[entity_entity_id].update(circle_entity);

            circle_count += 1;
            if (circle_count >= MAX_CIRCLES_PER_PLAYER) {
                break;
            }
        }
    }

    int64_t recombine_micros = static_cast<int64_t>(SPLIT_RECOMBINE_DELAY_SEC * 1000000.0f);
    Timestamp trigger_at = ctx.timestamp + TimeDuration::from_micros(recombine_micros);
    CircleRecombineTimer timer{0, ScheduleAt::time(trigger_at), player_opt->player_id};
    ctx.db[circle_recombine_timer].insert(timer);

    LOG_WARN("Player split!");
    return Ok();
}

SPACETIMEDB_REDUCER(spawn_food, ReducerContext ctx, SpawnFoodTimer) {
    if (ctx.db[player].count() == 0) {
        return Ok();
    }

    auto config_opt = ctx.db[config_id].find(0);
    if (!config_opt.has_value()) {
        return Err("Config not found");
    }
    float world_size = static_cast<float>(config_opt->world_size);

    auto& rng = ctx.rng();
    uint64_t food_count = ctx.db[food].count();
    while (food_count < static_cast<uint64_t>(TARGET_FOOD_COUNT)) {
        int32_t food_mass = rng.gen_range(FOOD_MASS_MIN, FOOD_MASS_MAX - 1);
        float food_radius = mass_to_radius(food_mass);
        float x = rng.gen_range(food_radius, world_size - food_radius);
        float y = rng.gen_range(food_radius, world_size - food_radius);

        Entity food_entity{0, DbVector2(x, y), food_mass};
        Entity inserted_entity = ctx.db[entity].insert(food_entity);
        ctx.db[food].insert(Food{inserted_entity.entity_id});

        food_count += 1;
        LOG_INFO("Spawned food! " + std::to_string(inserted_entity.entity_id));
    }

    return Ok();
}

SPACETIMEDB_REDUCER(circle_decay, ReducerContext ctx, CircleDecayTimer) {
    for (const auto& circle_row : ctx.db[circle]) {
        auto entity_opt = ctx.db[entity_entity_id].find(circle_row.entity_id);
        if (!entity_opt.has_value()) {
            return Err("Entity not found");
        }
        Entity circle_entity = entity_opt.value();
        if (circle_entity.mass <= START_PLAYER_MASS) {
            continue;
        }
        circle_entity.mass = static_cast<int32_t>(static_cast<float>(circle_entity.mass) * 0.99f);
        (void)ctx.db[entity_entity_id].update(circle_entity);
    }
    return Ok();
}

SPACETIMEDB_REDUCER(circle_recombine, ReducerContext ctx, CircleRecombineTimer timer) {
    auto circles = ctx.db[circle_player_id].filter(timer.player_id);
    std::vector<Entity> recombining_entities;
    for (const auto& circle_row : circles) {
        float time_since_split =
            static_cast<float>(ctx.timestamp.duration_since(circle_row.last_split_time).micros())
            / 1000000.0f;
        if (time_since_split >= SPLIT_RECOMBINE_DELAY_SEC) {
            auto entity_opt = ctx.db[entity_entity_id].find(circle_row.entity_id);
            if (!entity_opt.has_value()) {
                return Err("Circle has no entity");
            }
            recombining_entities.push_back(entity_opt.value());
        }
    }

    if (recombining_entities.size() <= 1) {
        return Ok();
    }

    int32_t base_entity_id = recombining_entities[0].entity_id;
    for (size_t i = 1; i < recombining_entities.size(); ++i) {
        schedule_consume_entity(ctx, base_entity_id, recombining_entities[i].entity_id);
    }

    return Ok();
}
