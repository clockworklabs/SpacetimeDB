#include "spacetimedb/query_builder.h"
#include "spacetimedb/bsatn/timestamp.h"
#include "spacetimedb/bsatn/types.h"
#include "spacetimedb/bsatn/types_impl.h"
#include <array>
#include <cstdint>
#include <exception>
#include <functional>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

namespace qb = SpacetimeDB::query_builder;

namespace test_query_builder {

struct User {
    int32_t id;
    std::string name;
    bool online;
    SpacetimeDB::Identity identity;
    SpacetimeDB::Timestamp created_at;
    std::vector<uint8_t> bytes;
};

struct PlayerLevel {
    int32_t entity_id;
    int32_t level;
};

struct ConnectionRow {
    SpacetimeDB::ConnectionId connection_id;
};

struct LiteralRow {
    int32_t score;
    std::string name;
    bool active;
    SpacetimeDB::ConnectionId connection_id;
    SpacetimeDB::i256 cells;
    SpacetimeDB::Identity identity;
    SpacetimeDB::Timestamp ts;
    std::vector<uint8_t> bytes;
};

struct UserCols {
    qb::Col<User, int32_t> id;
    qb::Col<User, std::string> name;
    qb::Col<User, bool> online;
    qb::Col<User, SpacetimeDB::Identity> identity;
    qb::Col<User, SpacetimeDB::Timestamp> created_at;
    qb::Col<User, std::vector<uint8_t>> bytes;

    explicit UserCols(const char* table_name)
        : id(table_name, "id"),
          name(table_name, "name"),
          online(table_name, "online"),
          identity(table_name, "identity"),
          created_at(table_name, "created_at"),
          bytes(table_name, "bytes") {}
};

struct UserIxCols {
    qb::IxCol<User, int32_t> id;
    qb::IxCol<User, SpacetimeDB::Identity> identity;

    explicit UserIxCols(const char* table_name)
        : id(table_name, "id"),
          identity(table_name, "identity") {}
};

struct PlayerLevelCols {
    qb::Col<PlayerLevel, int32_t> entity_id;
    qb::Col<PlayerLevel, int32_t> level;

    explicit PlayerLevelCols(const char* table_name)
        : entity_id(table_name, "entity_id"),
          level(table_name, "level") {}
};

struct PlayerLevelIxCols {
    qb::IxCol<PlayerLevel, int32_t> entity_id;

    explicit PlayerLevelIxCols(const char* table_name)
        : entity_id(table_name, "entity_id") {}
};

struct ConnectionRowCols {
    qb::Col<ConnectionRow, SpacetimeDB::ConnectionId> connection_id;

    explicit ConnectionRowCols(const char* table_name)
        : connection_id(table_name, "connection_id") {}
};

struct ConnectionRowIxCols {
    explicit ConnectionRowIxCols(const char*) {}
};

struct LiteralRowCols {
    qb::Col<LiteralRow, int32_t> score;
    qb::Col<LiteralRow, std::string> name;
    qb::Col<LiteralRow, bool> active;
    qb::Col<LiteralRow, SpacetimeDB::ConnectionId> connection_id;
    qb::Col<LiteralRow, SpacetimeDB::i256> cells;
    qb::Col<LiteralRow, SpacetimeDB::Identity> identity;
    qb::Col<LiteralRow, SpacetimeDB::Timestamp> ts;
    qb::Col<LiteralRow, std::vector<uint8_t>> bytes;

    explicit LiteralRowCols(const char* table_name)
        : score(table_name, "score"),
          name(table_name, "name"),
          active(table_name, "active"),
          connection_id(table_name, "connection_id"),
          cells(table_name, "cells"),
          identity(table_name, "identity"),
          ts(table_name, "ts"),
          bytes(table_name, "bytes") {}
};

struct LiteralRowIxCols {
    explicit LiteralRowIxCols(const char*) {}
};

} // namespace test_query_builder

namespace SpacetimeDB::query_builder {

template<>
struct HasCols<test_query_builder::User> {
    static test_query_builder::UserCols get(const char* table_name) { return test_query_builder::UserCols(table_name); }
};

template<>
struct HasIxCols<test_query_builder::User> {
    static test_query_builder::UserIxCols get(const char* table_name) { return test_query_builder::UserIxCols(table_name); }
};

template<>
struct CanBeLookupTable<test_query_builder::User> : std::true_type {};

template<>
struct HasCols<test_query_builder::PlayerLevel> {
    static test_query_builder::PlayerLevelCols get(const char* table_name) { return test_query_builder::PlayerLevelCols(table_name); }
};

template<>
struct HasIxCols<test_query_builder::PlayerLevel> {
    static test_query_builder::PlayerLevelIxCols get(const char* table_name) { return test_query_builder::PlayerLevelIxCols(table_name); }
};

template<>
struct CanBeLookupTable<test_query_builder::PlayerLevel> : std::true_type {};

template<>
struct HasCols<test_query_builder::ConnectionRow> {
    static test_query_builder::ConnectionRowCols get(const char* table_name) { return test_query_builder::ConnectionRowCols(table_name); }
};

template<>
struct HasIxCols<test_query_builder::ConnectionRow> {
    static test_query_builder::ConnectionRowIxCols get(const char* table_name) { return test_query_builder::ConnectionRowIxCols(table_name); }
};

template<>
struct HasCols<test_query_builder::LiteralRow> {
    static test_query_builder::LiteralRowCols get(const char* table_name) { return test_query_builder::LiteralRowCols(table_name); }
};

template<>
struct HasIxCols<test_query_builder::LiteralRow> {
    static test_query_builder::LiteralRowIxCols get(const char* table_name) { return test_query_builder::LiteralRowIxCols(table_name); }
};

} // namespace SpacetimeDB::query_builder

namespace SpacetimeDB::bsatn {

template<>
struct algebraic_type_of<test_query_builder::User> {
    static AlgebraicType get() {
        ProductTypeBuilder builder;
        builder.with_field<int32_t>("id")
            .with_field<std::string>("name")
            .with_field<bool>("online")
            .with_field<SpacetimeDB::Identity>("identity")
            .with_field<SpacetimeDB::Timestamp>("created_at")
            .with_field<std::vector<uint8_t>>("bytes");
        return AlgebraicType::make_product(builder.build());
    }
};

} // namespace SpacetimeDB::bsatn

namespace test_query_builder {

template<typename TRow>
auto TableFor(const char* table_name) {
    return SpacetimeDB::QueryBuilder().table<TRow>(
        table_name,
        qb::HasCols<TRow>::get(table_name),
        qb::HasIxCols<TRow>::get(table_name));
}

void ExpectEq(const std::string& actual, const std::string& expected, const std::string& label) {
    if (actual != expected) {
        std::ostringstream out;
        out << label << "\nExpected: " << expected << "\nActual:   " << actual;
        throw std::runtime_error(out.str());
    }
}

void TestSimpleSelect() {
    auto users = TableFor<User>("users");
    ExpectEq(users.build().sql(), "SELECT * FROM \"users\"", "simple select");
}

void TestWhereLiteral() {
    auto users = TableFor<User>("users");
    const auto query = users.where([](const auto& user) { return user.id.eq(10); }).build();
    ExpectEq(query.sql(), "SELECT * FROM \"users\" WHERE (\"users\".\"id\" = 10)", "where literal");
}

void TestWhereMultiplePredicates() {
    auto users = TableFor<User>("users");
    const auto query = users
        .where([](const auto& user) { return user.id.eq(10); })
        .where([](const auto& user) { return user.id.gt(3); })
        .build();
    ExpectEq(
        query.sql(),
        "SELECT * FROM \"users\" WHERE ((\"users\".\"id\" = 10) AND (\"users\".\"id\" > 3))",
        "where multiple predicates");
}

void TestWhereAndFilter() {
    auto users = TableFor<User>("users");
    const auto query = users
        .where([](const auto& user) { return user.online; })
        .filter([](const auto& user) { return user.id.gt(10); })
        .build();
    ExpectEq(
        query.sql(),
        "SELECT * FROM \"users\" WHERE ((\"users\".\"online\" = TRUE) AND (\"users\".\"id\" > 10))",
        "where/filter composition");
}

void TestColumnComparisons() {
    auto users = TableFor<User>("users");

    ExpectEq(
        users.where([](const auto& user) { return user.id.eq(user.id); }).build().sql(),
        "SELECT * FROM \"users\" WHERE (\"users\".\"id\" = \"users\".\"id\")",
        "column eq comparison");

    ExpectEq(
        users.where([](const auto& user) { return user.id.gt(user.id); }).build().sql(),
        "SELECT * FROM \"users\" WHERE (\"users\".\"id\" > \"users\".\"id\")",
        "column gt comparison");
}

void TestComparisonOperators() {
    auto users = TableFor<User>("users");

    ExpectEq(
        users.where([](const auto& user) { return user.name.ne("Shub"); }).build().sql(),
        "SELECT * FROM \"users\" WHERE (\"users\".\"name\" <> 'Shub')",
        "ne comparison");

    ExpectEq(
        users.where([](const auto& user) { return user.id.gte(18); })
            .where([](const auto& user) { return user.id.lte(30); })
            .build()
            .sql(),
        "SELECT * FROM \"users\" WHERE ((\"users\".\"id\" >= 18) AND (\"users\".\"id\" <= 30))",
        "gte lte comparison");
}

void TestLogicalComposition() {
    auto users = TableFor<User>("users");
    const auto query = users
        .where([](const auto& user) {
            return user.name.eq("Alice").not_().and_(user.online.eq(true).or_(user.id.gte(7)));
        })
        .build();
    ExpectEq(
        query.sql(),
        "SELECT * FROM \"users\" WHERE ((NOT (\"users\".\"name\" = 'Alice')) AND ((\"users\".\"online\" = TRUE) OR (\"users\".\"id\" >= 7)))",
        "logical composition");
}

void TestNotAndOr() {
    auto users = TableFor<User>("users");

    ExpectEq(
        users.where([](const auto& user) { return user.name.eq("Alice").not_(); }).build().sql(),
        "SELECT * FROM \"users\" WHERE (NOT (\"users\".\"name\" = 'Alice'))",
        "not comparison");

    ExpectEq(
        users.where([](const auto& user) {
            return user.name.eq("Alice").not_().and_(user.id.gt(18));
        }).build().sql(),
        "SELECT * FROM \"users\" WHERE ((NOT (\"users\".\"name\" = 'Alice')) AND (\"users\".\"id\" > 18))",
        "not with and");

    ExpectEq(
        users.where([](const auto& user) {
            return user.name.ne("Shub").or_(user.name.ne("Pop"));
        }).build().sql(),
        "SELECT * FROM \"users\" WHERE ((\"users\".\"name\" <> 'Shub') OR (\"users\".\"name\" <> 'Pop'))",
        "or comparison");
}

void TestFilterAlias() {
    auto users = TableFor<User>("users");
    const auto query = users
        .filter([](const auto& user) { return user.id.eq(5); })
        .filter([](const auto& user) { return user.id.lt(30); })
        .build();
    ExpectEq(
        query.sql(),
        "SELECT * FROM \"users\" WHERE ((\"users\".\"id\" = 5) AND (\"users\".\"id\" < 30))",
        "filter alias");
}

void TestLiteralFormatting() {
    auto users = TableFor<User>("users");

    std::array<uint8_t, SpacetimeDB::Identity::SIZE> identity_bytes{};
    identity_bytes.front() = 1;
    const auto identity = SpacetimeDB::Identity(identity_bytes);
    const auto timestamp = SpacetimeDB::Timestamp::from_micros_since_epoch(1000);
    const auto connection_id = SpacetimeDB::ConnectionId(SpacetimeDB::u128(0, 0));

    ExpectEq(
        users.where([&](const auto& user) { return user.identity.eq(identity); }).build().sql(),
        "SELECT * FROM \"users\" WHERE (\"users\".\"identity\" = 0x0000000000000000000000000000000000000000000000000000000000000001)",
        "identity formatting");

    ExpectEq(
        users.where([&](const auto& user) { return user.created_at.eq(timestamp); }).build().sql(),
        "SELECT * FROM \"users\" WHERE (\"users\".\"created_at\" = '1970-01-01T00:00:00.001+00:00')",
        "timestamp formatting");

    ExpectEq(
        users.where([](const auto& user) { return user.bytes.eq(std::vector<uint8_t>{1, 2, 3, 255}); }).build().sql(),
        "SELECT * FROM \"users\" WHERE (\"users\".\"bytes\" = 0x010203ff)",
        "byte formatting");

    ExpectEq(
        users.where([](const auto& user) { return user.id.eq(100); }).build().sql(),
        "SELECT * FROM \"users\" WHERE (\"users\".\"id\" = 100)",
        "integer literal formatting");

    ExpectEq(
        users.where([](const auto& user) { return user.name.ne("Alice"); }).build().sql(),
        "SELECT * FROM \"users\" WHERE (\"users\".\"name\" <> 'Alice')",
        "string literal formatting");

    ExpectEq(
        users.where([](const auto& user) { return user.online.eq(true); }).build().sql(),
        "SELECT * FROM \"users\" WHERE (\"users\".\"online\" = TRUE)",
        "bool literal formatting");

    auto connections = TableFor<ConnectionRow>("player");
    ExpectEq(
        connections.where([&](const auto& row) { return row.connection_id.eq(connection_id); }).build().sql(),
        "SELECT * FROM \"player\" WHERE (\"player\".\"connection_id\" = 0x00000000000000000000000000000000)",
        "connection id formatting");
}

void TestLiteralMatrix() {
    auto table = TableFor<LiteralRow>("player");

    ExpectEq(
        table.where([](const auto& row) { return row.score.eq(100); }).build().sql(),
        "SELECT * FROM \"player\" WHERE (\"player\".\"score\" = 100)",
        "literal matrix int");

    ExpectEq(
        table.where([](const auto& row) { return row.name.ne("Alice"); }).build().sql(),
        "SELECT * FROM \"player\" WHERE (\"player\".\"name\" <> 'Alice')",
        "literal matrix string");

    ExpectEq(
        table.where([](const auto& row) { return row.active.eq(true); }).build().sql(),
        "SELECT * FROM \"player\" WHERE (\"player\".\"active\" = TRUE)",
        "literal matrix bool");

    ExpectEq(
        table.where([](const auto& row) { return row.connection_id.eq(SpacetimeDB::ConnectionId(SpacetimeDB::u128(0, 0))); }).build().sql(),
        "SELECT * FROM \"player\" WHERE (\"player\".\"connection_id\" = 0x00000000000000000000000000000000)",
        "literal matrix connection id");

    const auto big_int = SpacetimeDB::i256(
        0xffffffffffffffffull,
        0xffffffffffffffffull,
        0xff00000000000000ull,
        0x0000000000000000ull);
    ExpectEq(
        table.where([&](const auto& row) { return row.cells.gt(big_int); }).build().sql(),
        "SELECT * FROM \"player\" WHERE (\"player\".\"cells\" > -1329227995784915872903807060280344576)",
        "literal matrix i256");

    std::array<uint8_t, SpacetimeDB::Identity::SIZE> identity_bytes{};
    identity_bytes.front() = 1;
    const auto identity = SpacetimeDB::Identity(identity_bytes);
    ExpectEq(
        table.where([&](const auto& row) { return row.identity.ne(identity); }).build().sql(),
        "SELECT * FROM \"player\" WHERE (\"player\".\"identity\" <> 0x0000000000000000000000000000000000000000000000000000000000000001)",
        "literal matrix identity");

    const auto ts = SpacetimeDB::Timestamp::from_micros_since_epoch(1000);
    ExpectEq(
        table.where([&](const auto& row) { return row.ts.eq(ts); }).build().sql(),
        "SELECT * FROM \"player\" WHERE (\"player\".\"ts\" = '1970-01-01T00:00:00.001+00:00')",
        "literal matrix timestamp");

    ExpectEq(
        table.where([](const auto& row) { return row.bytes.eq(std::vector<uint8_t>{1, 2, 3, 4, 255}); }).build().sql(),
        "SELECT * FROM \"player\" WHERE (\"player\".\"bytes\" = 0x01020304ff)",
        "literal matrix bytes");
}

void TestDirectExprFormatting() {
    const auto expr = qb::Col<User, int32_t>("user", "id").eq(42);
    ExpectEq(expr.format(), "(\"user\".\"id\" = 42)", "direct expr formatting");
}

void TestQueryReturnWrapperShape() {
    const auto query_type = SpacetimeDB::bsatn::algebraic_type_of<qb::RawQuery<User>>::get();
    if (query_type.tag() != SpacetimeDB::bsatn::AlgebraicTypeTag::Product) {
        throw std::runtime_error("query return wrapper should be a product type");
    }

    const auto& product = query_type.as_product();
    if (product.elements.size() != 1) {
        throw std::runtime_error("query return wrapper should have exactly one field");
    }
    if (!product.elements[0].name.has_value() || product.elements[0].name.value() != "__query__") {
        throw std::runtime_error("query return wrapper field should be named __query__");
    }

    const auto& wrapped = *product.elements[0].algebraic_type;
    if (!wrapped.is_product()) {
        throw std::runtime_error("query return wrapper payload should be a product row type");
    }
}

void TestSemiJoins() {
    auto users = TableFor<User>("users");
    auto levels = TableFor<PlayerLevel>("player_level");

    const auto left = users.left_semijoin(levels, [](const auto& user, const auto& level) {
        return user.id.eq(level.entity_id);
    }).where([](const auto& user) {
        return user.id.eq(1);
    }).build();

    ExpectEq(
        left.sql(),
        "SELECT \"users\".* FROM \"users\" JOIN \"player_level\" ON \"users\".\"id\" = \"player_level\".\"entity_id\" WHERE (\"users\".\"id\" = 1)",
        "left semijoin");

    const auto left_from_where = users.where([](const auto& user) { return user.id.eq(1); }).left_semijoin(
        levels,
        [](const auto& user, const auto& level) {
            return user.id.eq(level.entity_id);
        }).where([](const auto& user) {
            return user.id.gt(10);
        }).build();

    ExpectEq(
        left_from_where.sql(),
        "SELECT \"users\".* FROM \"users\" JOIN \"player_level\" ON \"users\".\"id\" = \"player_level\".\"entity_id\" WHERE ((\"users\".\"id\" = 1) AND (\"users\".\"id\" > 10))",
        "left semijoin from-where");

    const auto right = users.where([](const auto& user) { return user.online; }).right_semijoin(
        levels,
        [](const auto& user, const auto& level) {
            return user.id.eq(level.entity_id);
        }).where([](const auto& level) {
            return level.level.eq(3);
        }).build();

    ExpectEq(
        right.sql(),
        "SELECT \"player_level\".* FROM \"users\" JOIN \"player_level\" ON \"users\".\"id\" = \"player_level\".\"entity_id\" WHERE (\"users\".\"online\" = TRUE) AND (\"player_level\".\"level\" = 3)",
        "right semijoin");

    const auto right_with_both = users.where([](const auto& user) { return user.id.eq(1); }).right_semijoin(
        levels,
        [](const auto& user, const auto& level) {
            return user.id.eq(level.entity_id);
        }).where([](const auto& level) {
            return level.level.gt(10);
        }).where([](const auto& level) {
            return level.level.lt(30);
        }).build();

    ExpectEq(
        right_with_both.sql(),
        "SELECT \"player_level\".* FROM \"users\" JOIN \"player_level\" ON \"users\".\"id\" = \"player_level\".\"entity_id\" WHERE (\"users\".\"id\" = 1) AND ((\"player_level\".\"level\" > 10) AND (\"player_level\".\"level\" < 30))",
        "right semijoin chained where");
}

} // namespace test_query_builder

int RunQueryBuilderSqlTests() {
    const std::vector<std::pair<const char*, std::function<void()>>> tests = {
        {"simple select", test_query_builder::TestSimpleSelect},
        {"where literal", test_query_builder::TestWhereLiteral},
        {"where multiple predicates", test_query_builder::TestWhereMultiplePredicates},
        {"where/filter", test_query_builder::TestWhereAndFilter},
        {"column comparisons", test_query_builder::TestColumnComparisons},
        {"comparison operators", test_query_builder::TestComparisonOperators},
        {"logical composition", test_query_builder::TestLogicalComposition},
        {"not and or", test_query_builder::TestNotAndOr},
        {"filter alias", test_query_builder::TestFilterAlias},
        {"literal formatting", test_query_builder::TestLiteralFormatting},
        {"literal matrix", test_query_builder::TestLiteralMatrix},
        {"direct expr formatting", test_query_builder::TestDirectExprFormatting},
        {"query return wrapper shape", test_query_builder::TestQueryReturnWrapperShape},
        {"semi joins", test_query_builder::TestSemiJoins},
    };

    for (const auto& [name, test] : tests) {
        try {
            test();
        } catch (const std::exception& ex) {
            std::cerr << "FAILED: " << name << "\n" << ex.what() << "\n";
            return 1;
        }
    }

    return 0;
}
