#ifndef SPACETIMEDB_RUNTIME_REGISTRATION_H
#define SPACETIMEDB_RUNTIME_REGISTRATION_H

#include <functional>
#include <optional>
#include <string>
#include <vector>
#include "../abi/opaque_types.h"
#include "autogen/Lifecycle.g.h"

namespace SpacetimeDB {

struct ReducerContext;
struct ViewContext;
struct AnonymousViewContext;
struct ProcedureContext;

namespace Internal {

void RegisterReducerHandler(const std::string& name,
                           std::function<void(ReducerContext&, BytesSource)> handler,
                           std::optional<Lifecycle> lifecycle = std::nullopt);
void RegisterViewHandler(const std::string& name,
                        std::function<std::vector<uint8_t>(ViewContext&, BytesSource)> handler);
void RegisterAnonymousViewHandler(const std::string& name,
                                 std::function<std::vector<uint8_t>(AnonymousViewContext&, BytesSource)> handler);
void RegisterProcedureHandler(const std::string& name,
                             std::function<std::vector<uint8_t>(ProcedureContext&, BytesSource)> handler);
size_t GetViewHandlerCount();
size_t GetAnonymousViewHandlerCount();
size_t GetProcedureHandlerCount();
std::vector<uint8_t> ConsumeBytes(BytesSource source);
void SetMultiplePrimaryKeyError(const std::string& table_name);
void SetConstraintRegistrationError(const std::string& code, const std::string& details);

} // namespace Internal
} // namespace SpacetimeDB

#endif // SPACETIMEDB_RUNTIME_REGISTRATION_H
