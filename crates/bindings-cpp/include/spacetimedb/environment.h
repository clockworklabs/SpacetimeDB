#ifndef SPACETIMEDB_ENVIRONMENT_H
#define SPACETIMEDB_ENVIRONMENT_H
#include <spacetimedb/abi/FFI.h>
#include <array>
#include <optional>
#include <spacetimedb/logger.h>
#include <string>
#include <string_view>

namespace SpacetimeDB {
/// Read-only database environment. Reads use the current transaction, or a
/// short snapshot in a procedure outside a transaction. Values are not cached.
class Environment {
public:
    std::optional<std::string> get(std::string_view key) const {
        if (key.empty() || key.size() > 256) LOG_PANIC("invalid environment variable name");
        BytesSource source{0};
        if (FFI::env_get(reinterpret_cast<const uint8_t*>(key.data()), static_cast<uint32_t>(key.size()), &source) != Status(0))
            LOG_PANIC("environment read failed");
        if (source == BytesSource{0}) return std::nullopt;
        std::array<uint8_t, 1024> buffer;
        std::string value;
        for (;;) {
            size_t len = buffer.size();
            const auto status = FFI::bytes_source_read(source, buffer.data(), &len);
            if ((status != 0 && status != -1) || len > buffer.size()) LOG_PANIC("environment source read failed");
            value.append(reinterpret_cast<const char*>(buffer.data()), len);
            if (status == -1) return value;
            if (len == 0) LOG_PANIC("environment source made no progress");
        }
    }
};
}
#endif
