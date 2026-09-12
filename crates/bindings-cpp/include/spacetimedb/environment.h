#ifndef SPACETIMEDB_ENVIRONMENT_H
#define SPACETIMEDB_ENVIRONMENT_H
#include <spacetimedb/abi/FFI.h>
#include <array>
#include <optional>
#include <spacetimedb/logger.h>
#include <string>
#include <string_view>
#include <type_traits>
#include <unordered_set>
#include <spacetimedb/internal/autogen/EnvironmentDeclaration.g.h>

namespace SpacetimeDB {
/// Read-only database environment. Reads use the current transaction, or a
/// short snapshot in a procedure outside a transaction. Values are not cached.
class EnvironmentBase {
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

namespace Internal {
inline std::vector<EnvironmentDeclaration>& environment_declarations() {
    static std::vector<EnvironmentDeclaration> declarations;
    return declarations;
}

template<typename T>
inline constexpr bool environment_string = std::is_same_v<T, std::string> || std::is_same_v<T, std::optional<std::string>>;

template<typename T>
T read_environment(std::string_view key) {
    static_assert(environment_string<T>, "Environment declarations require string or optional<string>");
    auto value = EnvironmentBase{}.get(key);
    if constexpr (std::is_same_v<T, std::string>) {
        if (!value) LOG_PANIC("required environment value is absent");
        return std::move(*value);
    } else { return value; }
}

template<typename T>
EnvironmentDeclaration declare_environment(std::string name) {
    static_assert(environment_string<T>, "Environment declarations require string or optional<string>");
    EnvironmentConstraint constraint;
    constraint.set<0>(std::monostate{});
    return {std::move(name), std::move(constraint), std::is_same_v<T, std::optional<std::string>>};
}

// Preserve the complete source literal, including embedded NUL bytes. Implicit
// conversion through std::string(const char*) would truncate those constraints.
struct EnvironmentLiteral {
    std::string value;
    template<size_t N>
    EnvironmentLiteral(const char (&text)[N]) : value(text, N - 1) {}
    EnvironmentLiteral(std::string text) : value(std::move(text)) {}
};

template<typename T>
EnvironmentDeclaration declare_environment(std::string name, std::initializer_list<EnvironmentLiteral> allowed) {
    auto declaration = declare_environment<T>(std::move(name));
    if (allowed.size() == 0) LOG_PANIC("environment literal union cannot be empty");
    std::unordered_set<std::string> unique;
    std::vector<std::string> values;
    values.reserve(allowed.size());
    for (const auto& literal : allowed) {
        const auto& value = literal.value;
        if (value.size() > 8192 || !unique.insert(value).second) LOG_PANIC("invalid environment literal union");
        values.push_back(value);
    }
    if (values.size() == 1) declaration.constraint.template set<1>(std::move(values.front()));
    else declaration.constraint.template set<2>(std::move(values));
    return declaration;
}

inline void validate_environment_declarations() {
    const auto& declarations = environment_declarations();
    if (declarations.size() > 256) LOG_PANIC("too many environment declarations");
    std::unordered_set<std::string> keys;
    const auto initial = [](char c) { return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || c == '_'; };
    for (const auto& declaration : declarations) {
        const auto& key = declaration.name;
        if (key.empty() || key.size() > 256 || !initial(key[0]) || !keys.insert(key).second) LOG_PANIC("invalid environment declaration name");
        for (char c : key) if (!initial(c) && !(c >= '0' && c <= '9')) LOG_PANIC("invalid environment declaration name");
    }
}
}

#ifndef SPACETIMEDB_ENV_DECLARATION
class Environment : public EnvironmentBase {};
#endif
}
#endif
