// Server module for regression tests.
// Everything we're testing for happens SDK-side so this module is very uninteresting.

using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Name = "ExampleData", Public = true)]
    public partial struct ExampleData
    {
        [SpacetimeDB.PrimaryKey]
        public uint Id;

        [SpacetimeDB.Index.BTree]
        public uint Indexed;
    }
    
    [SpacetimeDB.Table(Name = "User", Public = true)]
    public partial struct User
    {
        [PrimaryKey]
        public string IdentityString;
        public bool GeneratedByConnectedClient;
    }

    [SpacetimeDB.View(Name = "GetExampleDataById", Public = true)]
    public static ExampleData? GetExampleDataById(ViewContext ctx)//, uint id)
    {
        return ctx.Db.ExampleData.Id.Find(0);
    }

    [SpacetimeDB.View(Name = "GetAnonymousExampleDataById", Public = true)]
    public static ExampleData? GetAnonymousExampleDataById(AnonymousViewContext ctx) //, uint id)
    {
        return ctx.Db.ExampleData.Id.Find(0);
    }
    
    [SpacetimeDB.View(Name = "GetUserByContext", Public = true)]
    public static User? GetUserByContext(ViewContext ctx)
    {
        return ctx.Db.User.IdentityString.Find(ctx.Sender.ToString());
    }

    [SpacetimeDB.View(Name = "GetUserByString", Public = true)]
    public static User? GetUserByString(AnonymousViewContext ctx) //, string identityString)
    {
        return ctx.Db.User.IdentityString.Find("identityStringExample");
    }

    [SpacetimeDB.Reducer]
    public static void Delete(ReducerContext ctx, uint id)
    {
        ctx.Db.ExampleData.Id.Delete(id);
    }

    [SpacetimeDB.Reducer]
    public static void Add(ReducerContext ctx, uint id, uint indexed)
    {
        ctx.Db.ExampleData.Insert(new ExampleData { Id = id, Indexed = indexed });
    }

    [SpacetimeDB.Reducer]
    public static void ThrowError(ReducerContext ctx, string error)
    {
        throw new Exception(error);
    }
    
    [SpacetimeDB.Reducer]
    public static void CreateNewUser(ReducerContext ctx, string identityString)
    {
        ctx.Db.User.Insert(
            new User
            {
                IdentityString = identityString,
                GeneratedByConnectedClient = false,
            }
        );
    }
    
    [SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        Log.Info($"Connect {ctx.Sender}");

        if (ctx.Db.User.IdentityString.Find(ctx.Sender.ToString()!) is User thisUser)
        {
            ctx.Db.User.IdentityString.Update(thisUser);
        }
        else
        {
            // If this is a new User, create a `User` object for the `IdentityString`,
            ctx.Db.User.Insert(
                new User
                {
                    IdentityString = ctx.Sender.ToString()!,
                    GeneratedByConnectedClient = true,
                }
            );
        }
    }
}
