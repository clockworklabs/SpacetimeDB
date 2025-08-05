#if GODOT_PC
#nullable enable
using Godot;

namespace ImGuiGodot.Internal;

internal interface IBackend
{
    bool Visible { get; set; }
    float JoyAxisDeadZone { get; set; }
    float Scale { get; set; }
    void ResetFonts();
    void AddFont(FontFile fontData, int fontSize, bool merge, ushort[]? glyphRanges);
    void AddFontDefault();
    void RebuildFontAtlas();
    void Connect(Callable callable);
    void SetMainViewport(Viewport vp);
    bool SubViewportWidget(SubViewport svp);
    void SetIniFilename(string filename);
}
#endif
