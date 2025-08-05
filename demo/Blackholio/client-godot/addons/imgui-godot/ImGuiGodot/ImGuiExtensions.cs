#if GODOT_PC
using Godot;
using ImGuiNET;
using Vector3 = System.Numerics.Vector3;
using Vector4 = System.Numerics.Vector4;

namespace ImGuiGodot;

public static class ImGuiExtensions
{
    /// <summary>
    /// Extension method to translate between <see cref="Key"/> and <see cref="ImGuiKey"/>
    /// </summary>
    public static ImGuiKey ToImGuiKey(this Key key)
    {
        return Internal.Input.ConvertKey(key);
    }

    /// <summary>
    /// Extension method to translate between <see cref="JoyButton"/> and <see cref="ImGuiKey"/>
    /// </summary>
    public static ImGuiKey ToImGuiKey(this JoyButton button)
    {
        return Internal.Input.ConvertJoyButton(button);
    }

    /// <summary>
    /// Convert <see cref="Color"/> to ImGui color RGBA
    /// </summary>
    public static Vector4 ToVector4(this Color color)
    {
        return new Vector4(color.R, color.G, color.B, color.A);
    }

    /// <summary>
    /// Convert <see cref="Color"/> to ImGui color RGB
    /// </summary>
    public static Vector3 ToVector3(this Color color)
    {
        return new Vector3(color.R, color.G, color.B);
    }

    /// <summary>
    /// Convert RGB <see cref="Vector3"/> to <see cref="Color"/>
    /// </summary>
    public static Color ToColor(this Vector3 vec)
    {
        return new Color(vec.X, vec.Y, vec.Z);
    }

    /// <summary>
    /// Convert RGBA <see cref="Vector4"/> to <see cref="Color"/>
    /// </summary>
    public static Color ToColor(this Vector4 vec)
    {
        return new Color(vec.X, vec.Y, vec.Z, vec.W);
    }

    /// <summary>
    /// Set IniFilename, converting Godot path to native
    /// </summary>
    public static void SetIniFilename(this ImGuiIOPtr io, string fileName)
    {
        _ = io;
        ImGuiGD.SetIniFilename(fileName);
    }
}
#endif
