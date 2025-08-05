#if GODOT_PC
#nullable enable
using Godot;
using System;

namespace ImGuiGodot;

public static partial class ImGuiGD
{
    private static readonly Internal.IBackend _backend;

    /// <summary>
    /// Deadzone for all axes
    /// </summary>
    public static float JoyAxisDeadZone
    {
        get => _backend.JoyAxisDeadZone;
        set => _backend.JoyAxisDeadZone = value;
    }

    /// <summary>
    /// Setting this property will reload fonts and modify the ImGuiStyle.
    /// Can only be set outside of a process frame (eg, use CallDeferred)
    /// </summary>
    public static float Scale
    {
        get => _backend.Scale;
        set
        {
            if (_backend.Scale != value && value >= 0.25f)
            {
                _backend.Scale = value;
                RebuildFontAtlas();
            }
        }
    }

    public static bool Visible
    {
        get => _backend.Visible;
        set => _backend.Visible = value;
    }

    static ImGuiGD()
    {
        _backend = ClassDB.ClassExists("ImGuiGD")
            ? new Internal.BackendNative()
            : new Internal.BackendNet();
    }

    public static IntPtr BindTexture(Texture2D tex)
    {
        return (IntPtr)tex.GetRid().Id;
    }

    public static void ResetFonts()
    {
        _backend.ResetFonts();
    }

    public static void AddFont(
        FontFile fontData,
        int fontSize,
        bool merge = false,
        ushort[]? glyphRanges = null)
    {
        _backend.AddFont(fontData, fontSize, merge, glyphRanges);
    }

    /// <summary>
    /// Add a font using glyph ranges from ImGui.GetIO().Fonts.GetGlyphRanges*()
    /// </summary>
    /// <param name="glyphRanges">pointer to an array of ushorts terminated by 0</param>
    public static unsafe void AddFont(FontFile fontData, int fontSize, bool merge, nint glyphRanges)
    {
        ushort* p = (ushort*)glyphRanges;
        int len = 1;
        while (p[len++] != 0) ;
        ushort[] gr = new ushort[len];
        for (int i = 0; i < len; ++i)
            gr[i] = p[i];
        _backend.AddFont(fontData, fontSize, merge, gr);
    }

    public static void AddFontDefault()
    {
        _backend.AddFontDefault();
    }

    public static void RebuildFontAtlas()
    {
        _backend.RebuildFontAtlas();
    }

    public static void Connect(Callable callable)
    {
        _backend.Connect(callable);
    }

    public static void Connect(Action action)
    {
        Connect(Callable.From(action));
    }

    /// <summary>
    /// Changes the main viewport to either a new <see cref="Window"/>
    /// or a <see cref="SubViewport"/>.
    /// </summary>
    public static void SetMainViewport(Viewport vp)
    {
        _backend.SetMainViewport(vp);
    }

    /// <summary>
    /// Must call from a tool script before doing anything else
    /// </summary>
    public static bool ToolInit()
    {
        if (_backend is Internal.BackendNative nbe)
        {
            nbe.ToolInit();
            return true;
        }

        return false;
    }

    public static void SetIniFilename(string filename)
    {
        _backend.SetIniFilename(filename);
    }
}
#endif
