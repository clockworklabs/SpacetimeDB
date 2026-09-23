using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Type]
    public partial struct Preferences
    {
        public string Theme;
        public bool EmailNotifications;
        public string Timezone;
    }

    [Table(Accessor = "Profile", Public = true)]
    public partial struct Profile { [PrimaryKey] public ulong Id; public Preferences Preferences; }

    [Reducer]
    public static void CreateProfile(ReducerContext ctx, ulong id, string theme, bool emailNotifications, string timezone) =>
        ctx.Db.Profile.Insert(new Profile
        {
            Id = id,
            Preferences = new Preferences { Theme = theme, EmailNotifications = emailNotifications, Timezone = timezone }
        });

    [Reducer]
    public static void UpdateTheme(ReducerContext ctx, ulong id, string theme)
    {
        var profile = ctx.Db.Profile.Id.Find(id) ?? throw new Exception("profile not found");
        var preferences = profile.Preferences;
        preferences.Theme = theme;
        ctx.Db.Profile.Id.Update(profile with { Preferences = preferences });
    }
}
