using Godot;

namespace SpacetimeDB
{
    // This is an optional helper class to store your auth token in PlayerPrefs
    // You can use keySuffix to save and retrieve different tokens
    public static class AuthToken
    {
        private const string Path = "user://spacetimedb-token.cfg";
        private const string Section = "stdb";
        private static string Key => "identity_token";

        private static ConfigFile _config;
        private static ConfigFile Config => _config ??= new ConfigFile();

        public static string GetToken(string keySuffix = null)
        {
            var key = GetKey(keySuffix);
            if(!Config.HasSectionKey(Section, key))
            {
                Config.Load(Path);
            }
            return Config.GetValue(Section, key, default(string)).As<string>();
        }

        public static bool TryGetToken(out string token) => TryGetToken(null, out token);
        public static bool TryGetToken(string keySuffix, out string token)
        {
            token = GetToken(keySuffix);
            return !string.IsNullOrWhiteSpace(token);
        }

        public static void SaveToken(string token, string keySuffix = null)
        {
            Config.SetValue(Section, GetKey(keySuffix), token);
            Config.Save(Path);
        }

        private static string GetKey(string suffix) => string.IsNullOrWhiteSpace(suffix) ? Key : $"{Key}_{suffix}";
    }
}