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
 * Provides access to the JWT claims of the caller of the reducer, if any.
 * A call is internal when its sender is the database itself, as for scheduled reducers.
 * Other calls, including init, come from a client, the owner, or an HTTP request
 * (with potential JWT authentication).
 * 
 * This class uses lazy loading - the JWT is only fetched and parsed when accessed.
 */
class AuthCtx {
    friend struct HandlerContext;

private:
    // The sender, or none for internal(), whose sender is the database.
    std::optional<Identity> sender_;
    // Computed on first use, since comparing the sender with the database's Identity needs a host call.
    mutable std::optional<bool> is_internal_;
    mutable std::shared_ptr<std::optional<JwtClaims>> jwt_;
    std::function<std::optional<JwtClaims>()> jwt_loader_;

    // Private constructor used by factory methods
    AuthCtx(std::optional<Identity> sender, std::function<std::optional<JwtClaims>()> loader);

    // Creates the AuthCtx of an HTTP handler's sender, whose JWT payload the host wrote to jwt_source.
    // The payload is read now, because each transaction the handler opens copies this AuthCtx.
    static AuthCtx from_jwt_source(BytesSource jwt_source, Identity sender);

    // Reads the JWT payload that the host wrote to source, if there is one.
    static std::optional<std::string> read_jwt(BytesSource source);

    static Identity database_identity();

public:
    /**
     * @brief Creates the AuthCtx of an invocation by sender, from an optional ConnectionId.
     * 
     * It is internal when sender is this database.
     * If the connection_id is present, the AuthCtx will load the JWT for it.
     * If the connection_id is absent, there is no JWT.
     * 
     * @param connection_id Optional connection ID
     * @param sender The identity of the caller (already derived from JWT claims by the host)
     * @return An AuthCtx based on the sender and connection_id
     */
    static AuthCtx from_connection_id_opt(std::optional<ConnectionId> connection_id, Identity sender);

    /**
     * @brief Creates an AuthCtx for a call by the database itself, such as a scheduled reducer.
     * 
     * It has no JWT.
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
     * @brief Returns whether the sender of this invocation is this database,
     * as it is for scheduled reducers and procedures.
     * 
     * This is equivalent to `ctx.sender() == ctx.database_identity()`.
     * It is false in init, whose sender is the database's owner.
     * 
     * @return true if this is an internal call
     */
    bool is_internal() const;

    /**
     * @brief Checks if there is a JWT without loading it.
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
     * If is_internal() returns true, this will return an empty optional.
     * 
     * @return An optional containing the JwtClaims if available
     */
    const std::optional<JwtClaims>& get_jwt() const;

    /**
     * @brief Gets the caller's identity, which is the sender of this invocation.
     * 
     * For internal calls, this returns the database's identity.
     * 
     * @return The caller's Identity
     */
    Identity get_caller_identity() const;
};

// ============================================================================
// INLINE IMPLEMENTATIONS
// ============================================================================

inline AuthCtx::AuthCtx(std::optional<Identity> sender, std::function<std::optional<JwtClaims>()> loader)
    : sender_(std::move(sender)), jwt_loader_(std::move(loader)) {}

inline AuthCtx AuthCtx::from_connection_id_opt(std::optional<ConnectionId> connection_id, Identity sender) {
    if (connection_id.has_value()) {
        return from_connection_id(*connection_id, std::move(sender));
    } else {
        return AuthCtx(std::move(sender), []() -> std::optional<JwtClaims> { return std::nullopt; });
    }
}

inline AuthCtx AuthCtx::internal() {
    return AuthCtx(std::nullopt, []() -> std::optional<JwtClaims> { return std::nullopt; });
}

inline AuthCtx AuthCtx::from_jwt_payload(std::string jwt_payload, Identity identity) {
    return AuthCtx(identity, [payload = std::move(jwt_payload), id = std::move(identity)]() mutable -> std::optional<JwtClaims> {
        return JwtClaims(std::move(payload), std::move(id));
    });
}

inline AuthCtx AuthCtx::from_jwt_source(BytesSource jwt_source, Identity sender) {
    return AuthCtx(sender, [payload = read_jwt(jwt_source), sender]() -> std::optional<JwtClaims> {
        if (!payload.has_value()) return std::nullopt;
        return JwtClaims(*payload, sender);
    });
}

inline AuthCtx AuthCtx::from_connection_id(ConnectionId connection_id, Identity sender) {
    return AuthCtx(sender, [connection_id, sender]() -> std::optional<JwtClaims> {
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
        if (status != Status(0)) {
            return std::nullopt;
        }
        
        std::optional<std::string> jwt_payload = read_jwt(jwt_source);
        if (!jwt_payload.has_value()) return std::nullopt;
        // Use the provided sender identity (already computed by host from JWT claims)
        return JwtClaims(std::move(*jwt_payload), sender);
    });
}

inline std::optional<std::string> AuthCtx::read_jwt(BytesSource jwt_source) {
    if (jwt_source == BytesSource{0}) {
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
    return jwt_payload;
}

inline bool AuthCtx::is_internal() const {
    if (!is_internal_.has_value()) {
        is_internal_ = !sender_.has_value() || *sender_ == database_identity();
    }
    return *is_internal_;
}

inline bool AuthCtx::has_jwt() const {
    if (is_internal()) {
        return false;
    }
    
    // Load the JWT if not already loaded, then check if it has a value
    // This ensures has_jwt() and get_jwt() are consistent
    return get_jwt().has_value();
}

inline const std::optional<JwtClaims>& AuthCtx::get_jwt() const {
    if (!jwt_) {
        jwt_ = std::make_shared<std::optional<JwtClaims>>(is_internal() ? std::optional<JwtClaims>() : jwt_loader_());
    }
    return *jwt_;
}

inline Identity AuthCtx::get_caller_identity() const {
    if (sender_.has_value()) {
        return *sender_;
    }
    // Only internal() has no sender, because its sender is the database.
    return database_identity();
}

inline Identity AuthCtx::database_identity() {
    std::array<uint8_t, 32> identity_bytes;
    FFI::identity(identity_bytes.data());
    return Identity(identity_bytes);
}

} // namespace SpacetimeDB

#endif // SPACETIMEDB_AUTH_CTX_H
