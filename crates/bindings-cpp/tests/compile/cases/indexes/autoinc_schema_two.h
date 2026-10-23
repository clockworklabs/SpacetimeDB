#pragma once

struct AutoIncSameLineTwo {
    uint64_t id;
};

SPACETIMEDB_STRUCT(AutoIncSameLineTwo, id)
SPACETIMEDB_TABLE(AutoIncSameLineTwo, autoinc_same_line_two, Public)
#line 100
FIELD_PrimaryKeyAutoInc(autoinc_same_line_two, id)
