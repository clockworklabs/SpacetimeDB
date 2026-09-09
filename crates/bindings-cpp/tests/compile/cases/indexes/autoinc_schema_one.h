#pragma once

struct AutoIncSameLineOne {
    uint64_t id;
};

SPACETIMEDB_STRUCT(AutoIncSameLineOne, id)
SPACETIMEDB_TABLE(AutoIncSameLineOne, autoinc_same_line_one, Public)
#line 100
FIELD_PrimaryKeyAutoInc(autoinc_same_line_one, id)
