namespace SpacetimeDB;

using System;
using System.Collections.Generic;
using Internal;

public readonly record struct Handler(string FunctionName)
{ }

public sealed class Router
{
    internal readonly record struct RouteSpec(MethodOrAny Method, string Path, string HandlerFunction);

    private const string AcceptableRoutePathCharsHumanDescription =
        "ASCII lowercase letters, digits and `-_~/`";

    private readonly List<RouteSpec> routes;

    private Router(List<RouteSpec> routes)
    {
        this.routes = routes;
    }

    public static Router New() => new([]);

    public Router Get(string path, Handler handler) =>
        AddRoute(new MethodOrAny.Method(new Internal.HttpMethod.Get(default)), path, handler);

    public Router Head(string path, Handler handler) =>
        AddRoute(new MethodOrAny.Method(new Internal.HttpMethod.Head(default)), path, handler);

    public Router Options(string path, Handler handler) =>
        AddRoute(new MethodOrAny.Method(new Internal.HttpMethod.Options(default)), path, handler);

    public Router Put(string path, Handler handler) =>
        AddRoute(new MethodOrAny.Method(new Internal.HttpMethod.Put(default)), path, handler);

    public Router Delete(string path, Handler handler) =>
        AddRoute(new MethodOrAny.Method(new Internal.HttpMethod.Delete(default)), path, handler);

    public Router Post(string path, Handler handler) =>
        AddRoute(new MethodOrAny.Method(new Internal.HttpMethod.Post(default)), path, handler);

    public Router Patch(string path, Handler handler) =>
        AddRoute(new MethodOrAny.Method(new Internal.HttpMethod.Patch(default)), path, handler);

    public Router Any(string path, Handler handler) =>
        AddRoute(new MethodOrAny.Any(default), path, handler);

    public Router Nest(string path, Router subRouter)
    {
        AssertValidPath(path);
        if (routes.Exists(route => route.Path.StartsWith(path, StringComparison.Ordinal)))
        {
            throw new ArgumentException(
                $"Cannot nest router at `{path}`; existing routes overlap with nested path",
                nameof(path)
            );
        }

        var merged = CloneRoutes();
        foreach (var route in subRouter.routes)
        {
            var nestedPath = JoinPaths(path, route.Path);
            AddRoute(merged, route.Method, nestedPath, route.HandlerFunction);
        }

        return new Router(merged);
    }

    public Router Merge(Router otherRouter)
    {
        var merged = CloneRoutes();
        foreach (var route in otherRouter.routes)
        {
            AddRoute(merged, route.Method, route.Path, route.HandlerFunction);
        }

        return new Router(merged);
    }

    internal IReadOnlyList<RouteSpec> GetRoutes() => routes;

    private Router AddRoute(MethodOrAny method, string path, Handler handler)
    {
        var merged = CloneRoutes();
        AddRoute(merged, method, path, handler.FunctionName);
        return new Router(merged);
    }

    private List<RouteSpec> CloneRoutes() => new(routes);

    private static void AddRoute(
        List<RouteSpec> routes,
        MethodOrAny method,
        string path,
        string handlerFunction
    )
    {
        AssertValidPath(path);
        ArgumentException.ThrowIfNullOrEmpty(handlerFunction);

        var candidate = new RouteSpec(method, path, handlerFunction);
        if (routes.Exists(route => RoutesOverlap(route, candidate)))
        {
            throw new ArgumentException($"Route conflict for `{path}`", nameof(path));
        }

        routes.Add(candidate);
    }
    private static string JoinPaths(string prefix, string suffix)
    {
        if (prefix == "/")
        {
            return suffix;
        }
        if (suffix == "/")
        {
            return prefix;
        }

        prefix = prefix.TrimEnd('/');
        suffix = suffix.TrimStart('/');
        return $"{prefix}/{suffix}";
    }

    private static bool RoutesOverlap(RouteSpec a, RouteSpec b)
    {
        if (!string.Equals(a.Path, b.Path, StringComparison.Ordinal))
        {
            return false;
        }

        return a.Method is MethodOrAny.Any
            || b.Method is MethodOrAny.Any
            || Equals(a.Method, b.Method);
    }

    private static void AssertValidPath(string path)
    {
        ArgumentNullException.ThrowIfNull(path);
        if (path.Length > 0 && path[0] != '/')
        {
            throw new ArgumentException($"Route paths must start with `/`: {path}", nameof(path));
        }
        foreach (var c in path)
        {
            if (!CharacterIsAcceptableForRoutePath(c))
            {
                throw new ArgumentException(
                    $"Route paths may contain only {AcceptableRoutePathCharsHumanDescription}: {path}",
                    nameof(path)
                );
            }
        }
    }

    private static bool CharacterIsAcceptableForRoutePath(char c) =>
        (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c is '-' or '_' or '~' or '/';
}
