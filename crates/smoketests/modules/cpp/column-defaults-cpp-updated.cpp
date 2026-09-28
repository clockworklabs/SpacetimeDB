
#include "spacetimedb.h"

using namespace SpacetimeDB;

struct DefaultsTestTable {
    uint32_t id;
    bool bool_value;
    int8_t i8_value;
    uint8_t u8_value;
    int16_t i16_value;
    uint16_t u16_value;
    int32_t i32_value;
    uint32_t u32_value;
    int64_t i64_value;
    uint64_t u64_value;
    float f32_positive_value;
    float f32_negative_value;
    double f64_positive_value;
    double f64_negative_value;
    std::string string_value;
};
SPACETIMEDB_STRUCT(
    DefaultsTestTable,
    id,
    bool_value,
    i8_value,
    u8_value,
    i16_value,
    u16_value,
    i32_value,
    u32_value,
    i64_value,
    u64_value,
    f32_positive_value,
    f32_negative_value,
    f64_positive_value,
    f64_negative_value,
    string_value
)
SPACETIMEDB_TABLE(DefaultsTestTable, defaults_test_table, Public)
FIELD_Default(defaults_test_table, bool_value, true)
FIELD_Default(defaults_test_table, i8_value, int8_t(-8))
FIELD_Default(defaults_test_table, u8_value, uint8_t(8))
FIELD_Default(defaults_test_table, i16_value, int16_t(-16))
FIELD_Default(defaults_test_table, u16_value, uint16_t(16))
FIELD_Default(defaults_test_table, i32_value, int32_t(-32))
FIELD_Default(defaults_test_table, u32_value, uint32_t(32))
FIELD_Default(defaults_test_table, i64_value, int64_t(-64))
FIELD_Default(defaults_test_table, u64_value, uint64_t(64))
FIELD_Default(defaults_test_table, f32_positive_value, float(32.5))
FIELD_Default(defaults_test_table, f32_negative_value, float(-32.5))
FIELD_Default(defaults_test_table, f64_positive_value, double(64.25))
FIELD_Default(defaults_test_table, f64_negative_value, double(-64.25))
FIELD_Default(defaults_test_table, string_value, std::string("default string"))
