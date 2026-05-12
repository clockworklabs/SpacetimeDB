#ifndef SPACETIMEDB_ROUTER_H
#define SPACETIMEDB_ROUTER_H

#ifndef SPACETIMEDB_UNSTABLE_FEATURES
#error "spacetimedb/router.h requires SPACETIMEDB_UNSTABLE_FEATURES to be enabled"
#endif

#include <spacetimedb/http.h>
#include <spacetimedb/http_convert.h>
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
        const auto& a_method = a.method.template get<1>().value;
        const auto& b_method = b.method.template get<1>().value;
        return a_method == b_method;
    }

    static Internal::MethodOrAny make_any() {
        Internal::MethodOrAny method;
        method.set<0>(std::monostate{});
        return method;
    }

    static Internal::MethodOrAny make_method(const HttpMethod& method) {
        Internal::MethodOrAny result;
        Internal::MethodOrAny_Method_Wrapper wrapper;
        wrapper.value = convert::to_wire(method);
        result.set<1>(wrapper);
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
