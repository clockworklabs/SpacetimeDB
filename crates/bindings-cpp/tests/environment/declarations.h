#pragma once
#include <spacetimedb/environment_declaration.h>
SPACETIMEDB_ENV(
    (FOOBAR, std::string),
    (ENABLE_EMAIL, std::string, ("true", "false")),
    (LOG_LEVEL, std::optional<std::string>, ("debug", "info", "error")),
    (DEPLOYMENT_KIND, std::string, ("production")),
    (get, std::optional<std::string>),
    (class, std::string),
    (NUL_LITERAL, std::string, ("a\0b"))
)
