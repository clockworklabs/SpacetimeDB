#if GODOT_PC
#nullable enable
using Godot;

namespace ImGuiGodot.Internal;

internal sealed class BackendNative : IBackend
{
    private readonly GodotObject _gd = Engine.GetSingleton("ImGuiGD");

    private sealed class MethodName
    {
        public static readonly StringName AddFont = "AddFont";
        public static readonly StringName AddFontDefault = "AddFontDefault";
        public static readonly StringName Connect = "Connect";
        public static readonly StringName RebuildFontAtlas = "RebuildFontAtlas";
        public static readonly StringName ResetFonts = "ResetFonts";
        public static readonly StringName SetMainViewport = "SetMainViewport";
        public static readonly StringName SubViewport = "SubViewport";
        public static readonly StringName ToolInit = "ToolInit";
        public static readonly StringName SetIniFilename = "SetIniFilename";
    }

    private sealed class PropertyName
    {
        public static readonly StringName JoyAxisDeadZone = "JoyAxisDeadZone";
        public static readonly StringName Scale = "Scale";
        public static readonly StringName Visible = "Visible";
    }

    public float JoyAxisDeadZone
    {
        get => (float)_gd.Get(PropertyName.JoyAxisDeadZone);
        set => _gd.Set(PropertyName.JoyAxisDeadZone, value);
    }

    public float Scale
    {
        get => (float)_gd.Get(PropertyName.Scale);
        set => _gd.Set(PropertyName.Scale, value);
    }

    public bool Visible
    {
        get => (bool)_gd.Get(PropertyName.Visible);
        set => _gd.Set(PropertyName.Visible, value);
    }

    public void AddFont(FontFile fontData, int fontSize, bool merge, ushort[]? glyphRanges)
    {
        if (glyphRanges != null)
        {
            int[] gr = new int[glyphRanges.Length];
            for (int i = 0; i < glyphRanges.Length; ++i)
                gr[i] = glyphRanges[i];
            _gd.Call(MethodName.AddFont, fontData, fontSize, merge, gr);
        }
        else
        {
            _gd.Call(MethodName.AddFont, fontData, fontSize, merge);
        }
    }

    public void AddFontDefault()
    {
        _gd.Call(MethodName.AddFontDefault);
    }

    public void Connect(Callable callable)
    {
        _gd.Call(MethodName.Connect, callable);
    }

    public void RebuildFontAtlas()
    {
        _gd.Call(MethodName.RebuildFontAtlas);
    }

    public void ResetFonts()
    {
        _gd.Call(MethodName.ResetFonts);
    }
    public void SetMainViewport(Viewport vp)
    {
        _gd.Call(MethodName.SetMainViewport, vp);
    }

    public bool SubViewportWidget(SubViewport svp)
    {
        return (bool)_gd.Call(MethodName.SubViewport, svp);
    }

    public void ToolInit()
    {
        _gd.Call(MethodName.ToolInit);
        ImGuiSync.SyncPtrs();
    }

    public void SetIniFilename(string filename)
    {
        _gd.Call(MethodName.SetIniFilename, filename);
    }
}
#endif
