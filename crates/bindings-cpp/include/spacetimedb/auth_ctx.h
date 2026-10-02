#ifndef SPACETIMEDB_AUTH_CTX_H
#define SPACETIMEDB_AUTH_CTX_H

#include "spacetimedb/jwt_claims.h"
#include "spacetimedb/bsatn/types.h"
#include "spacetimedb/abi/FFI.h"
#include "spacetimedb/abi/opaque_types.h"
#include <memory>
#include <optional>
#include <functional>
#include <vector>
#include <array>

namespace SpacetimeDB {

// Forward declarations
struct ConnectionId;

/**
 * @brief Authentication context for a reducer call.
 * 
 * Provides access to the JWT claims for the connection that triggered the reducer,
 * if any, and reports whether the sender is this database (see is_internal()).
 * 
 * This class uses lazy loading - the JWT is only fetched and parsed when accessed.
 */
class AuthCtx {
private:
    // Computed on first use, since it needs a host call to read the database's identity.
    mutable std::optional<bool> is_internal_;
    std::function<bool()> is_internal_loader_;
    mutable std::shared_ptr<std::optional<JwtClaims>> jwt_;
    std::function<std::optional<JwtClaims>()> jwt_loader_;
    // The sender verified by the host. Absent only for `internal()`, whose sender is the database.
    std::optional<Identity> sender_;

    // Private constructors used by factory methods
    AuthCtx(bool is_internal, std::function<std::optional<JwtClaims>()> loader);
    AuthCtx(std::function<bool()> is_internal_loader, std::function<std::optional<JwtClaims>()> loader);

public:
    /**
     * @brief Creates the AuthCtx of an invocation whose sender is `sender`.
     * 
     * The invocation is internal when `sender` is this database.
     * If the connection_id is present, the JWT recorded for it is loaded on demand.
     * 
     * @param connection_id Optional connection ID
     * @param sender The identity of the caller, as verified by the host
     * @return An AuthCtx for the invocation
     */
    static AuthCtx from_connection_id_opt(std::optional<ConnectionId> connection_id, Identity sender);

    /**
     * @brief Creates an AuthCtx whose is_internal() is true and which has no JWT.
     * 
     * Invocations get their AuthCtx from from_connection_id_opt, which computes
     * is_internal() from the sender. This is for contexts constructed outside
     * an invocation, such as a default-constructed ReducerContext.
     * 
     * @return An AuthCtx representing an internal call
     */
    static AuthCtx internal();

    /**
     * @brief Creates an AuthCtx from a JWT payload string.
     * 
     * This is primarily used for testing purposes, allowing you to create
     * an AuthCtx with specific JWT claims without needing a real connection.
     * 
     * Note: The Identity must be computed by calling the host function,
     * as we cannot compute Blake3 hashes in WASM.
     * 
     * @param jwt_payload The raw JWT payload (JSON claims)
     * @param identity The identity derived from the JWT's issuer and subject
     * @return An AuthCtx with the provided JWT
     */
    static AuthCtx from_jwt_payload(std::string jwt_payload, Identity identity);

    /**
     * @brief Creates an AuthCtx that reads the JWT for the given connection ID.
     * 
     * The JWT will be lazily loaded from the host when first accessed.
     * The identity parameter is the sender's identity, already derived from
     * JWT claims by the host (using Blake3 hashing).
     * 
     * @param connection_id The connection ID to load the JWT for
     * @param sender The identity of the caller (already derived from JWT claims by the host)
     * @return An AuthCtx that will load the JWT on demand
     */
    static AuthCtx from_connection_id(ConnectionId connection_id, Identity sender);

    /**
     * @brief Returns whether the sender of this invocation is this database.
     * 
     * This is true when the database acts on its own behalf, for example in a
     * scheduled reducer or procedure, and false for every other sender,
     * including the database's owner in `init`. It is equivalent to
     * `ctx.sender() == ctx.database_identity()`.
     * 
     * @return true if the sender is this database
     */
    bool is_internal() const;

    /**
     * @brief Checks if there is a JWT.
     * 
     * If is_internal() returns true, this will return false.
     * 
     * @return true if a JWT is available
     */
    bool has_jwt() const;

    /**
     * @brief Gets the JWT claims, loading them if necessary.
     * 
     * This will fetch the JWT from the host on the first call and cache it.
     * Internal invocations have no JWT, even when their sender presented one,
     * so this is empty whenever is_internal() returns true.
     * 
     * @return An optional containing the JwtClaims if available
     */
    const std::optional<JwtClaims>& get_jwt() const;

    /**
     * @brief Gets the caller's identity.
     * 
     * This is the sender verified by the host, the same as `ctx.sender()`,
     * whether or not the caller presented a JWT. For an internal invocation
     * it is the database's identity.
     * 
     * @return The caller's Identity
     */
    Identity get_caller_identity() const;
};

// ============================================================================
// INLINE IMPLEMENTATIONS
// ============================================================================

inline AuthCtx::AuthCtx(bool is_internal, std::function<std::optional<JwtClaims>()> loader)
    : is_internal_(is_internal), jwt_loader_(std::move(loader)) {}

inline AuthCtx::AuthCtx(std::function<bool()> is_internal_loader, std::function<std::optional<JwtClaims>()> loader)
    : is_internal_loader_(std::move(is_internal_loader)), jwt_loader_(std::move(loader)) {}

inline AuthCtx AuthCtx::from_connection_id_opt(std::optional<ConnectionId> connection_id, Identity sender) {
    auto is_self = [sender]() {
        std::array<uint8_t, 32> identity_bytes;
        FFI::identity(identity_bytes.data());
        return sender == Identity(identity_bytes);
    };
    AuthCtx auth = connection_id.has_value()
        ? AuthCtx(std::move(is_self), from_connection_id(*connection_id, sender).jwt_loader_)
        : AuthCtx(std::move(is_self), []() -> std::optional<JwtClaims> { return std::nullopt; });
    auth.sender_ = std::move(sender);
    return auth;
}

inline bool AuthCtx::is_internal() const {
    if (!is_internal_.has_value()) {
        is_internal_ = is_internal_loader_();
    }
    return *is_internal_;
}

inline AuthCtx AuthCtx::internal() {
    return AuthCtx(true, []() -> std::optional<JwtClaims> { return std::nullopt; });
}

inline AuthCtx AuthCtx::from_jwt_payload(std::string jwt_payload, Identity identity) {
    AuthCtx auth(false, [payload = std::move(jwt_payload), id = identity]() mutable -> std::optional<JwtClaims> {
        return JwtClaims(std::move(payload), std::move(id));
    });
    auth.sender_ = std::move(identity);
    return auth;
}

inline AuthCtx AuthCtx::from_connection_id(ConnectionId connection_id, Identity sender) {
    AuthCtx auth(false, [connection_id, sender]() -> std::optional<JwtClaims> {
        // Call the host FFI to get the JWT
        BytesSource jwt_source;
        
        // Convert ConnectionId to byte array (little-endian)
        std::array<uint8_t, 16> conn_id_bytes;
        for (int i = 0; i < 8; ++i) {
            conn_id_bytes[i] = (connection_id.id.low >> (i * 8)) & 0xFF;
        }
        for (int i = 0; i < 8; ++i) {
            conn_id_bytes[8 + i] = (connection_id.id.high >> (i * 8)) & 0xFF;
        }
        
        Status status = FFI::get_jwt(conn_id_bytes.data(), &jwt_source);
        if (status != Status(0) || jwt_source == BytesSource{0}) {
            return std::nullopt;
        }
        
        // Read the JWT payload from the BytesSource
        std::array<uint8_t, 4096> buffer;
        std::string jwt_payload;
        for (;;) {
            size_t buffer_len = buffer.size();
            const auto result = bytes_source_read(jwt_source, buffer.data(), &buffer_len);
            if (result != 0 && result != -1) return std::nullopt;
            jwt_payload.append(reinterpret_cast<const char*>(buffer.data()), buffer_len);
            // -1 is successful exhaustion and may include the final payload bytes.
            if (result == -1) break;
            if (buffer_len == 0) return std::nullopt;
        }
        if (jwt_payload.empty()) return std::nullopt;
        // Use the provided sender identity (already computed by host from JWT claims)
        return JwtClaims(std::move(jwt_payload), sender);
    });
    auth.sender_ = std::move(sender);
    return auth;
}

inline bool AuthCtx::has_jwt() const {
    return get_jwt().has_value();
}

inline const std::optional<JwtClaims>& AuthCtx::get_jwt() const {
    if (!jwt_) {
        jwt_ = std::make_shared<std::optional<JwtClaims>>(
            is_internal() ? std::nullopt : jwt_loader_());
    }
    return *jwt_;
}

inline Identity AuthCtx::get_caller_identity() const {
    if (sender_.has_value()) {
        return *sender_;
    }
    // Only `internal()` has no recorded sender, and its sender is the database.
    std::array<uint8_t, 32> identity_bytes;
    FFI::identity(identity_bytes.data());
    return Identity(identity_bytes);
}

} // namespace SpacetimeDB

#endif // SPACETIMEDB_AUTH_CTX_H
