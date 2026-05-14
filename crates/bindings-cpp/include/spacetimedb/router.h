#ifndef SPACETIMEDB_ROUTER_H
#define SPACETIMEDB_ROUTER_H

#ifndef SPACETIMEDB_UNSTABLE_FEATURES
#error "spacetimedb/router.h requires SPACETIMEDB_UNSTABLE_FEATURES to be enabled"
#endif

#include <spacetimedb/http.h>
#include <spacetimedb/internal/autogen/MethodOrAny.g.h>
#include <spacetimedb/internal/runtime_registration.h>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <utility>
#include <vector>

namespace SpacetimeDB {

class Router {
public:
    struct RouteSpec {
        Internal::MethodOrAny method;
        std::string path;
        std::string handler_name;
    };

    Router() = default;

    template<typename Func>
    Router get(std::string path, Func handler) const {
        return add_route(make_method(HttpMethod::get()), std::move(path), handler);
    }

    template<typename Func>
    Router head(std::string path, Func handler) const {
        return add_route(make_method(HttpMethod::head()), std::move(path), handler);
    }

    template<typename Func>
    Router options(std::string path, Func handler) const {
        return add_route(make_method(HttpMethod::options()), std::move(path), handler);
    }

    template<typename Func>
    Router put(std::string path, Func handler) const {
        return add_route(make_method(HttpMethod::put()), std::move(path), handler);
    }

    template<typename Func>
    Router delete_(std::string path, Func handler) const {
        return add_route(make_method(HttpMethod::del()), std::move(path), handler);
    }

    template<typename Func>
    Router post(std::string path, Func handler) const {
        return add_route(make_method(HttpMethod::post()), std::move(path), handler);
    }

    template<typename Func>
    Router patch(std::string path, Func handler) const {
        return add_route(make_method(HttpMethod::patch()), std::move(path), handler);
    }

    template<typename Func>
    Router any(std::string path, Func handler) const {
        return add_route(make_any(), std::move(path), handler);
    }

    Router nest(std::string path, const Router& sub_router) const {
        assert_valid_path(path);
        Router merged = *this;
        for (const auto& route : routes_) {
            if (route.path.starts_with(path)) {
                fail_router_registration("Cannot nest router at `" + path + "`; existing routes overlap with nested path");
            }
        }
        for (const auto& route : sub_router.routes_) {
            merged = merged.add_route(route.method, join_paths(path, route.path), route.handler_name);
        }
        return merged;
    }

    Router merge(const Router& other) const {
        Router merged = *this;
        for (const auto& route : other.routes_) {
            merged = merged.add_route(route.method, route.path, route.handler_name);
        }
        return merged;
    }

    const std::vector<RouteSpec>& routes() const {
        return routes_;
    }

private:
    std::vector<RouteSpec> routes_;

    [[noreturn]] static void fail_router_registration(const std::string& message) {
        std::fprintf(stderr, "Router registration failed: %s\n", message.c_str());
        std::abort();
    }

    static bool character_is_acceptable_for_route_path(char c) {
        return (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '-' || c == '_' || c == '~' || c == '/';
    }

    static void assert_valid_path(const std::string& path) {
        if (!path.empty() && path[0] != '/') {
            fail_router_registration("Route paths must start with `/`: " + path);
        }
        for (char c : path) {
            if (!character_is_acceptable_for_route_path(c)) {
                fail_router_registration("Route paths may contain only ASCII lowercase letters, digits and `-_~/`: " + path);
            }
        }
    }

    static std::string join_paths(const std::string& prefix, const std::string& suffix) {
        if (prefix == "/") {
            return suffix;
        }
        if (suffix == "/") {
            return prefix;
        }
        std::string trimmed_prefix = prefix;
        while (!trimmed_prefix.empty() && trimmed_prefix.back() == '/') {
            trimmed_prefix.pop_back();
        }
        size_t start = 0;
        while (start < suffix.size() && suffix[start] == '/') {
            ++start;
        }
        return trimmed_prefix + "/" + suffix.substr(start);
    }

    static bool routes_overlap(const RouteSpec& a, const RouteSpec& b) {
        if (a.path != b.path) {
            return false;
        }
        if (a.method.is<0>() || b.method.is<0>()) {
            return true;
        }
        return method_key(a.method.template get<1>()) == method_key(b.method.template get<1>());
    }

    static std::string method_key(const Internal::HttpMethod& method) {
        switch (method.get_tag()) {
        case 0:
            return "GET";
        case 1:
            return "HEAD";
        case 2:
            return "POST";
        case 3:
            return "PUT";
        case 4:
            return "DELETE";
        case 5:
            return "CONNECT";
        case 6:
            return "OPTIONS";
        case 7:
            return "TRACE";
        case 8:
            return "PATCH";
        case 9:
            return method.template get<9>();
        default:
            fail_router_registration("Unsupported internal HTTP method tag");
        }
    }

    static Internal::MethodOrAny make_any() {
        Internal::MethodOrAny method;
        method.set<0>(std::monostate{});
        return method;
    }

    static Internal::MethodOrAny make_method(const HttpMethod& method) {
        Internal::MethodOrAny result;
        result.set<1>(to_internal_http_method(method));
        return result;
    }

    static Internal::HttpMethod to_internal_http_method(const HttpMethod& method) {
        Internal::HttpMethod result;
        if (method.value == "GET") {
            result.set<0>(std::monostate{});
        } else if (method.value == "HEAD") {
            result.set<1>(Internal::HttpMethod_Head_Wrapper{});
        } else if (method.value == "POST") {
            result.set<2>(Internal::HttpMethod_Post_Wrapper{});
        } else if (method.value == "PUT") {
            result.set<3>(Internal::HttpMethod_Put_Wrapper{});
        } else if (method.value == "DELETE") {
            result.set<4>(Internal::HttpMethod_Delete_Wrapper{});
        } else if (method.value == "CONNECT") {
            result.set<5>(Internal::HttpMethod_Connect_Wrapper{});
        } else if (method.value == "OPTIONS") {
            result.set<6>(Internal::HttpMethod_Options_Wrapper{});
        } else if (method.value == "TRACE") {
            result.set<7>(Internal::HttpMethod_Trace_Wrapper{});
        } else if (method.value == "PATCH") {
            result.set<8>(Internal::HttpMethod_Patch_Wrapper{});
        } else {
            result.set<9>(method.value);
        }
        return result;
    }

    template<typename Func>
    Router add_route(Internal::MethodOrAny method, std::string path, Func handler) const {
        return add_route(std::move(method), std::move(path), resolve_handler_name(handler));
    }

    Router add_route(Internal::MethodOrAny method, std::string path, std::string handler_name) const {
        assert_valid_path(path);
        RouteSpec candidate{method, path, std::move(handler_name)};
        for (const auto& route : routes_) {
            if (routes_overlap(route, candidate)) {
                fail_router_registration("Route conflict for `" + candidate.path + "`");
            }
        }
        Router next = *this;
        next.routes_.push_back(std::move(candidate));
        return next;
    }

    template<typename Func>
    static std::string resolve_handler_name(Func handler) {
        const void* symbol = reinterpret_cast<const void*>(handler);
        return Internal::LookupHttpHandlerName(symbol);
    }
};

} // namespace SpacetimeDB

#endif // SPACETIMEDB_ROUTER_H
