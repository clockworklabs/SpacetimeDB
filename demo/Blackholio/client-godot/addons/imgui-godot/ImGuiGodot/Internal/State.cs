#if GODOT_PC
using Godot;
using ImGuiNET;
using System;
using System.Runtime.InteropServices;

namespace ImGuiGodot.Internal;

internal sealed class State : IDisposable
{
    private enum RendererType
    {
        Dummy,
        Canvas,
        RenderingDevice
    }

    private static readonly IntPtr _backendName = Marshal.StringToCoTaskMemAnsi("godot4_net");
    private static IntPtr _rendererName = IntPtr.Zero;
    private static nint _clipBuf = 0;
    private IntPtr _iniFilenameBuffer = IntPtr.Zero;

    internal Viewports Viewports { get; }
    internal Fonts Fonts { get; }
    internal Input Input { get; set; }
    internal IRenderer Renderer { get; }

    internal float Scale { get; set; } = 1.0f;
    internal float JoyAxisDeadZone { get; set; } = 0.15f;
    internal int LayerNum { get; private set; } = 128;
    internal ImGuiLayer Layer { get; set; } = null!;
    internal bool InProcessFrame { get; set; }

    internal static State Instance { get; set; } = null!;

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate void PlatformSetImeDataFn(
        nint ctx,
        ImGuiViewportPtr vp,
        ImGuiPlatformImeDataPtr data);
    private static readonly PlatformSetImeDataFn _setImeData = SetImeData;

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate void SetClipboardTextFn(nint ud, nint text);
    private static readonly SetClipboardTextFn _setClipboardText = SetClipboardText;

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate nint GetClipboardTextFn(nint ud);
    private static readonly GetClipboardTextFn _getClipboardText = GetClipboardText;

    public State(IRenderer renderer)
    {
        Renderer = renderer;
        Input = new Input();
        Fonts = new Fonts();

        if (ImGui.GetCurrentContext() != IntPtr.Zero)
        {
            ImGui.DestroyContext();
        }

        var context = ImGui.CreateContext();
        ImGui.SetCurrentContext(context);

        var io = ImGui.GetIO();
        io.BackendFlags =
            ImGuiBackendFlags.HasGamepad |
            ImGuiBackendFlags.HasSetMousePos |
            ImGuiBackendFlags.HasMouseCursors |
            ImGuiBackendFlags.RendererHasVtxOffset |
            ImGuiBackendFlags.RendererHasViewports;

        if (_rendererName == IntPtr.Zero)
        {
            _rendererName = Marshal.StringToCoTaskMemAnsi(Renderer.Name);
        }

        unsafe
        {
            io.NativePtr->BackendPlatformName = (byte*)_backendName;
            io.NativePtr->BackendRendererName = (byte*)_rendererName;

            var pio = ImGui.GetPlatformIO().NativePtr;
            pio->Platform_SetImeDataFn = Marshal.GetFunctionPointerForDelegate(_setImeData);
            pio->Platform_SetClipboardTextFn = Marshal.GetFunctionPointerForDelegate(
                _setClipboardText);
            pio->Platform_GetClipboardTextFn = Marshal.GetFunctionPointerForDelegate(
                _getClipboardText);
        }

        Viewports = new Viewports();
    }

    public void Dispose()
    {
        if (ImGui.GetCurrentContext() != IntPtr.Zero)
            ImGui.DestroyContext();
        Renderer.Dispose();
    }

    public static void Init(Resource cfg)
    {
        if (IntPtr.Size != sizeof(ulong))
            throw new PlatformNotSupportedException("imgui-godot requires 64-bit pointers");

        RendererType rendererType = Enum.Parse<RendererType>((string)cfg.Get("Renderer"));

        if (DisplayServer.GetName() == "headless")
            rendererType = RendererType.Dummy;

        // fall back to Canvas in OpenGL compatibility mode
        if (rendererType == RendererType.RenderingDevice
            && RenderingServer.GetRenderingDevice() == null)
        {
            rendererType = RendererType.Canvas;
        }

        // there's no way to get the actual current thread model, eg if --render-thread is used
        int threadModel = (int)ProjectSettings.GetSetting("rendering/driver/threads/thread_model");

        IRenderer renderer;
        try
        {
            renderer = rendererType switch
            {
                RendererType.Dummy => new DummyRenderer(),
                RendererType.Canvas => new CanvasRenderer(),
                RendererType.RenderingDevice => threadModel == 2
                    ? new RdRendererThreadSafe()
                    : new RdRenderer(),
                _ => throw new ArgumentException("Invalid renderer", nameof(cfg))
            };
        }
        catch (Exception e)
        {
            if (rendererType == RendererType.RenderingDevice)
            {
                GD.PushWarning($"imgui-godot: falling back to Canvas renderer ({e.Message})");
                renderer = new CanvasRenderer();
            }
            else
            {
                GD.PushError("imgui-godot: failed to init renderer");
                renderer = new DummyRenderer();
            }
        }

        Instance = new(renderer)
        {
            Scale = (float)cfg.Get("Scale"),
            LayerNum = (int)cfg.Get("Layer")
        };

        ImGui.GetIO().SetIniFilename((string)cfg.Get("IniFilename"));

        var fonts = (Godot.Collections.Array)cfg.Get("Fonts");

        for (int i = 0; i < fonts.Count; ++i)
        {
            var fontres = (Resource)fonts[i];
            var fontData = (FontFile)fontres.Get("FontData");
            int fontSize = (int)fontres.Get("FontSize");
            bool merge = (bool)fontres.Get("Merge");
            if (i == 0)
                ImGuiGD.AddFont(fontData, fontSize);
            else
                ImGuiGD.AddFont(fontData, fontSize, merge);
        }
        if ((bool)cfg.Get("AddDefaultFont"))
            ImGuiGD.AddFontDefault();
        ImGuiGD.RebuildFontAtlas();
    }

    public unsafe void SetIniFilename(string fileName)
    {
        var io = ImGui.GetIO();
        io.NativePtr->IniFilename = null;

        if (_iniFilenameBuffer != IntPtr.Zero)
        {
            Marshal.FreeCoTaskMem(_iniFilenameBuffer);
            _iniFilenameBuffer = IntPtr.Zero;
        }

        if (fileName?.Length > 0)
        {
            fileName = ProjectSettings.GlobalizePath(fileName);
            _iniFilenameBuffer = Marshal.StringToCoTaskMemUTF8(fileName);
            io.NativePtr->IniFilename = (byte*)_iniFilenameBuffer;
        }
    }

    public void Update(double delta, System.Numerics.Vector2 displaySize)
    {
        var io = ImGui.GetIO();
        io.DisplaySize = displaySize;
        io.DeltaTime = (float)delta;

        Input.Update(io);

        ImGui.NewFrame();
    }

    public void Render()
    {
        ImGui.Render();
        ImGui.UpdatePlatformWindows();
        Renderer.Render();
    }

    private static void SetImeData(nint ctx, ImGuiViewportPtr vp, ImGuiPlatformImeDataPtr data)
    {
        int windowID = (int)vp.PlatformHandle;

        DisplayServer.WindowSetImeActive(data.WantVisible, windowID);
        if (data.WantVisible)
        {
            Vector2I pos = new(
                (int)(data.InputPos.X - vp.Pos.X),
                (int)(data.InputPos.Y - vp.Pos.Y + data.InputLineHeight)
                );
            DisplayServer.WindowSetImePosition(pos, windowID);
        }
    }

    private static void SetClipboardText(nint ud, nint text)
    {
        DisplayServer.ClipboardSet(Marshal.PtrToStringUTF8(text));
    }

    private static nint GetClipboardText(nint ud)
    {
        if (_clipBuf != 0)
            Marshal.FreeCoTaskMem(_clipBuf);
        _clipBuf = Marshal.StringToCoTaskMemUTF8(DisplayServer.ClipboardGet());
        return _clipBuf;
    }
}
#endif
