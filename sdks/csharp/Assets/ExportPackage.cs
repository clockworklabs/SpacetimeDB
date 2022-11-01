using System.Collections;
using System.Collections.Generic;
using UnityEditor;
using UnityEngine;

public class ExportPackage
{
    public static void Export()
    {
        AssetDatabase.ExportPackage("Assets/SpacetimeDB", "SpacetimeDBUnitySDK.unitypackage", ExportPackageOptions.Recurse);
    }
}
