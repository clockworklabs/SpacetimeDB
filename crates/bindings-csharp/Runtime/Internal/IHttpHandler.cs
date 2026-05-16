namespace SpacetimeDB.Internal;

public interface IHttpHandler
{
    RawHttpHandlerDefV10 MakeHandlerDef();

    SpacetimeDB.HttpResponse Invoke(
        SpacetimeDB.HandlerContextBase ctx,
        SpacetimeDB.HttpRequest request
    );
}
