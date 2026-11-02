#nullable enable
using System;
using System.Collections.Generic;
using SpacetimeDB;

[SpacetimeDB.Env]
public partial struct EnvironmentSchema
{
    public string REGION;
    [SpacetimeDB.EnvValues("development", "production")]
    public string MODE;
    public string? BANNER;
}

public static partial class Module
{
    [Table(Accessor = "DeploymentNote", Public = true)]
    public partial struct DeploymentNote
    {
        [PrimaryKey] public uint Id;
        public string Region;
        public string Mode;
        public string Banner;
    }

    [SpacetimeDB.Type]
    public partial struct DeploymentInfo
    {
        public string Region;
        public string Mode;
    }

    [Reducer]
    public static void RecordDeployment(ReducerContext ctx, uint id)
    {
        if (ctx.Db.DeploymentNote.Id.Find(id) is not null) throw new Exception("note exists");
        ctx.Db.DeploymentNote.Insert(new DeploymentNote
        {
            Id = id,
            Region = ctx.Env.REGION,
            Mode = ctx.Env.MODE,
            Banner = ctx.Env.BANNER ?? "(none)",
        });
    }

    [Reducer]
    public static void RequireDevelopment(ReducerContext ctx)
    {
        if (ctx.Env.MODE == "production") throw new Exception("production locked");
    }

    [View(Accessor = "CurrentDeployment", Public = true)]
    public static List<DeploymentInfo> CurrentDeployment(AnonymousViewContext ctx) =>
        new() { new DeploymentInfo { Region = ctx.Env.REGION, Mode = ctx.Env.MODE } };
}
