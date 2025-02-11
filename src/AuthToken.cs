/*  This is an optional helper class to store your auth token in local storage
 *
    Example:

    AuthToken.Init(".my_app_name");

    SpacetimeDBClient.instance.onIdentityReceived += (token, identity) =>
    {
        AuthToken.SaveToken(token);

        ...
    };

    SpacetimeDBClient.instance.Connect(AuthToken.Token, "localhost:3000", "basicchat", false);
 */
#if !UNITY_5_3_OR_NEWER
using System;
using System.IO;
using System.Linq;

namespace SpacetimeDB
{
    public static class AuthToken
    {
        private static string? settingsPath;
        private static string? token;

        private const string PREFIX = "auth_token=";

        /// <summary>
        /// Initializes the AuthToken class. This must be called before any other methods.
        /// </summary>
        /// <param name="configFolder">The folder to store the config file in. Default is ".spacetime_csharp_sdk".</param>
        /// <param name="configFile">The name of the config file. Default is "settings.ini".</param>
        /// <param name="configRoot">The root folder to store the config file in. Default is the user's home directory.</param>
        /// </summary>
        public static void Init(string configFolder = ".spacetime_csharp_sdk", string configFile = "settings.ini", string? configRoot = null)
        {
            configRoot ??= Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);

            if (Environment.GetCommandLineArgs().Any(arg => arg == "--client"))
            {
                int clientIndex = Array.FindIndex(Environment.GetCommandLineArgs(), arg => arg == "--client");
                var configFileParts = configFile.Split(".");
                configFile = $"{configFileParts[0]}_{Environment.GetCommandLineArgs()[clientIndex + 1]}.{configFileParts[1]}";
            }

            settingsPath = Path.Combine(configRoot, configFolder, configFile);

            if (File.Exists(settingsPath))
            {
                token =
                    File.ReadLines(settingsPath)
                    .FirstOrDefault(line => line.StartsWith(PREFIX))
                    ?[PREFIX.Length..];
            }
        }

        /// <summary>
        /// This is the auth token that was saved to local storage. Null if not never saved.
        /// When you specify null to the SpacetimeDBClient, SpacetimeDB will generate a new identity for you.
        /// </summary>
        public static string? Token
        {
            get
            {
                if (settingsPath == null)
                {
                    throw new Exception("Token not initialized. Call AuthToken.Init() first.");
                }
                return token;
            }
        }

        /// <summary>
        /// Save the auth token to local storage.
        /// SpacetimeDBClient provides this token to you in the onIdentityReceived callback.
        /// </summary>
        public static void SaveToken(string token)
        {
            if (settingsPath == null)
            {
                throw new Exception("Token not initialized. Call AuthToken.Init() first.");
            }
            Directory.CreateDirectory(Path.GetDirectoryName(settingsPath)!);
            var newAuthLine = PREFIX + token;
            var lines = File.Exists(settingsPath) ? File.ReadAllLines(settingsPath).ToList() : new();
            var i = lines.FindIndex(line => line.StartsWith(PREFIX));
            if (i >= 0)
            {
                lines[i] = newAuthLine;
            }
            else
            {
                lines.Add(newAuthLine);
            }
            File.WriteAllLines(settingsPath, lines);
        }
    }
}
#else
using UnityEngine;

namespace SpacetimeDB
{
    // This is an optional helper class to store your auth token in PlayerPrefs
    // Override GetTokenKey() if you want to use a player pref key specific to your game
    public static class AuthToken
    {
        public static string Token => PlayerPrefs.GetString(GetTokenKey());

        public static void SaveToken(string token)
        {
            PlayerPrefs.SetString(GetTokenKey(), token);
        }

        private static string GetTokenKey()
        {
            var key = "spacetimedb.identity_token";
#if UNITY_EDITOR
            // Different editors need different keys
            key += $" - {Application.dataPath}";
#endif
            return key;
        }
    }
}
#endif
