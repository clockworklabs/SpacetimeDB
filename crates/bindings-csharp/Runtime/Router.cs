namespace SpacetimeDB;

using System;
using System.Collections.Generic;
using System.Text;
using Internal;

public readonly record struct Handler(string FunctionName) { }

public sealed class Router
{
    internal readonly record struct RouteSpec(
        MethodOrAny Method,
        string Path,
        string HandlerFunction
    );

    private const string AcceptableRoutePathCharsHumanDescription =
        "ASCII lowercase letters, digits and `-_~/:*`";

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

    private List<RouteSpec> CloneRoutes() => [.. routes];

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
        if (!string.Equals(RouteSignature(a.Path), RouteSignature(b.Path), StringComparison.Ordinal))
        {
            return false;
        }

        return a.Method is MethodOrAny.Any
            || b.Method is MethodOrAny.Any
            || Equals(a.Method, b.Method);
    }

    private static string RouteSignature(string path)
    {
        var signature = new StringBuilder();
        var segments = path.Split('/');
        for (var index = 0; index < segments.Length; index++)
        {
            if (index > 0)
            {
                signature.Append('/');
            }
            var segment = segments[index];
            signature.Append(segment.Length > 0 && segment[0] == ':' ? ":" : segment);
        }
        return signature.ToString();
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
        var segments = path.Split('/');
        for (var index = 0; index < segments.Length; index++)
        {
            var segment = segments[index];
            if (segment.Length > 0 && segment[0] == ':')
            {
                if (segment.Length == 1 || !IsValidRouteParameterStart(segment[1]))
                {
                    throw new ArgumentException($"Invalid route parameter in path: {path}", nameof(path));
                }
                for (var parameterIndex = 1; parameterIndex < segment.Length; parameterIndex++)
                {
                    if (!IsValidRouteParameterCharacter(segment[parameterIndex]))
                    {
                        throw new ArgumentException($"Invalid route parameter in path: {path}", nameof(path));
                    }
                }
            }
            else if (segment == "*")
            {
                if (index == 0 || index != segments.Length - 1)
                {
                    throw new ArgumentException($"Wildcard must be the final route segment: {path}", nameof(path));
                }
            }
            else if (segment.Contains(':') || segment.Contains('*'))
            {
                throw new ArgumentException($"Invalid route pattern in path: {path}", nameof(path));
            }
        }
    }

    private static bool CharacterIsAcceptableForRoutePath(char c) =>
        c is (>= 'a' and <= 'z') or (>= '0' and <= '9') or '-' or '_' or '~' or '/' or ':' or '*';

    private static bool IsValidRouteParameterStart(char c) =>
        (c >= 'a' && c <= 'z') || c == '_';

    private static bool IsValidRouteParameterCharacter(char c) =>
        IsValidRouteParameterStart(c) || (c >= '0' && c <= '9');
}
