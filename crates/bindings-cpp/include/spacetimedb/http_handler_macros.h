#pragma once

#ifndef SPACETIMEDB_UNSTABLE_FEATURES
#error "spacetimedb/http_handler_macros.h requires SPACETIMEDB_UNSTABLE_FEATURES to be enabled"
#endif

#include "spacetimedb/handler_context.h"
#include "spacetimedb/http.h"
#include "spacetimedb/internal/runtime_registration.h"
#include "spacetimedb/internal/template_utils.h"
#include "spacetimedb/internal/v10_builder.h"
#include "spacetimedb/macros.h"
#include "spacetimedb/router.h"

namespace SpacetimeDB::Internal {

template<typename Func>
inline void RegisterHttpHandlerMacro(const char* handler_name, Func func) {
    using traits = function_traits<Func>;
    static_assert(traits::arity == 2, "HTTP handlers must take exactly two arguments");
    using ContextType = typename traits::template arg_t<0>;
    using RequestType = typename traits::template arg_t<1>;
    using ReturnType = typename traits::result_type;
    static_assert(std::is_same_v<ContextType, HandlerContext>, "First parameter of HTTP handler must be HandlerContext");
    static_assert(std::is_same_v<RequestType, HttpRequest>, "Second parameter of HTTP handler must be HttpRequest");
    static_assert(std::is_same_v<ReturnType, HttpResponse>, "HTTP handlers must return HttpResponse");

    std::function<HttpResponse(HandlerContext&, HttpRequest)> handler =
        [func](HandlerContext& ctx, HttpRequest request) -> HttpResponse {
            return func(ctx, std::move(request));
        };
    RegisterHttpHandlerHandler(handler_name, reinterpret_cast<const void*>(func), std::move(handler));
    getV10Builder().RegisterHttpHandlerDef(handler_name);
}

} // namespace SpacetimeDB::Internal

#define SPACETIMEDB_HTTP_HANDLER(handler_name, ctx_param, request_param) \
    SpacetimeDB::HttpResponse handler_name(ctx_param, request_param); \
    __attribute__((export_name("__preinit__60_http_handler_" #handler_name))) \
    extern "C" void CONCAT(_spacetimedb_preinit_register_http_handler_, handler_name)() { \
        ::SpacetimeDB::Internal::RegisterHttpHandlerMacro(#handler_name, handler_name); \
    } \
    SpacetimeDB::HttpResponse handler_name(ctx_param, request_param)

#define SPACETIMEDB_HTTP_HANDLER_NAMED(handler_name, canonical_name, ctx_param, request_param) \
    SpacetimeDB::HttpResponse handler_name(ctx_param, request_param); \
    __attribute__((export_name("__preinit__60_http_handler_" #handler_name))) \
    extern "C" void CONCAT(_spacetimedb_preinit_register_http_handler_, handler_name)() { \
        ::SpacetimeDB::Internal::RegisterHttpHandlerMacro(#handler_name, handler_name); \
        SpacetimeDB::Module::RegisterExplicitFunctionName(#handler_name, canonical_name); \
    } \
    SpacetimeDB::HttpResponse handler_name(ctx_param, request_param)

#define SPACETIMEDB_HTTP_ROUTER(router_name) \
    SpacetimeDB::Router router_name(); \
    __attribute__((export_name("__preinit__61_http_router_" #router_name))) \
    extern "C" void CONCAT(_spacetimedb_preinit_register_http_router_, router_name)() { \
        ::SpacetimeDB::Internal::getV10Builder().RegisterHttpRouter(router_name()); \
    } \
    SpacetimeDB::Router router_name()
