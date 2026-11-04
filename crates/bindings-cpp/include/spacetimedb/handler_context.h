#ifndef SPACETIMEDB_HANDLER_CONTEXT_H
#define SPACETIMEDB_HANDLER_CONTEXT_H

#ifndef SPACETIMEDB_UNSTABLE_FEATURES
#error "spacetimedb/handler_context.h requires SPACETIMEDB_UNSTABLE_FEATURES to be enabled"
#endif

#include <spacetimedb/abi/FFI.h>
#include <spacetimedb/bsatn/timestamp.h>
#include <spacetimedb/bsatn/uuid.h>
#include <spacetimedb/environment.h>
#include <spacetimedb/http.h>
#include <spacetimedb/internal/tx_execution.h>
#include <spacetimedb/random.h>
#include <spacetimedb/tx_context.h>
#include <array>
#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <type_traits>

namespace SpacetimeDB {

struct HandlerContext {
    Timestamp timestamp;
    Environment env;
    HttpClient http;

private:
    mutable std::shared_ptr<StdbRng> rng_instance;
    mutable uint32_t counter_uuid_ = 0;
    Identity sender_;
    AuthCtx sender_auth_ = AuthCtx::from_connection_id_opt(std::nullopt, Identity{});

public:
    HandlerContext() = default;
    explicit HandlerContext(Timestamp t) : timestamp(t) {}
    HandlerContext(Identity sender, BytesSource sender_jwt, Timestamp t)
        : timestamp(t), sender_(sender), sender_auth_(AuthCtx::from_jwt_source(sender_jwt, sender)) {}

    // The random Identity that the host created for this request.
    // Each request has a different sender, which is also the sender of the transactions it opens.
    Identity sender() const {
        return sender_;
    }

    Identity identity() const {
        std::array<uint8_t, 32> id_bytes;
        ::identity(id_bytes.data());
        return Identity(id_bytes);
    }

    StdbRng& rng() const {
        if (!rng_instance) {
            rng_instance = std::make_shared<StdbRng>(timestamp);
        }
        return *rng_instance;
    }

    Uuid new_uuid_v4() const {
        std::array<uint8_t, 16> random_bytes;
        rng().fill_bytes(random_bytes.data(), random_bytes.size());
        return Uuid::from_random_bytes_v4(random_bytes);
    }

    Uuid new_uuid_v7() const {
        std::array<uint8_t, 4> random_bytes;
        rng().fill_bytes(random_bytes.data(), random_bytes.size());
        return Uuid::from_counter_v7(counter_uuid_, timestamp, random_bytes);
    }

#ifdef SPACETIMEDB_UNSTABLE_FEATURES
    template<typename Func>
    auto with_tx(Func&& body) -> decltype(body(std::declval<TxContext&>())) {
        auto make_reducer_ctx = [this](Timestamp tx_timestamp) {
            return ReducerContext(
                sender_,
                std::nullopt,
                tx_timestamp,
                sender_auth_
            );
        };
        return Internal::with_tx(make_reducer_ctx, body);
    }

    template<typename Func>
    auto try_with_tx(Func&& body) -> decltype(body(std::declval<TxContext&>())) {
        auto make_reducer_ctx = [this](Timestamp tx_timestamp) {
            return ReducerContext(
                sender_,
                std::nullopt,
                tx_timestamp,
                sender_auth_
            );
        };
        return Internal::try_with_tx(make_reducer_ctx, body);
    }
#endif
};

} // namespace SpacetimeDB

#endif // SPACETIMEDB_HANDLER_CONTEXT_H
