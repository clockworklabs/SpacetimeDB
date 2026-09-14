#pragma once
#include <spacetimedb/environment_declaration.h>
SPACETIMEDB_ENV(
    (MISSING, std::optional<std::string>),
    (EMPTY, std::optional<std::string>),
    (UTF8, std::optional<std::string>),
    (NUL, std::optional<std::string>),
    (MAXIMUM, std::optional<std::string>)
)
