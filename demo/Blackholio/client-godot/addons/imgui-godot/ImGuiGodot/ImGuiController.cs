#if GODOT_PC
#nullable enable
using Godot;
using ImGuiNET;

namespace ImGuiGodot;

public partial class ImGuiController : Node
{
    private Window _window = null!;
    public static ImGuiController Instance { get; private set; } = null!;
    private ImGuiControllerHelper _helper = null!;
    public Node Signaler { get; private set; } = null!;
    private readonly StringName _signalName = "imgui_layout";

    private sealed partial class ImGuiControllerHelper : Node
    {
        public override void _Ready()
        {
            Name = "ImGuiControllerHelper";
            ProcessPriority = int.MinValue;
            ProcessMode = ProcessModeEnum.Always;
        }

        public override void _Process(double delta)
        {
            Internal.State.Instance.InProcessFrame = true;
            var vpSize = Internal.State.Instance.Layer.UpdateViewport();
            Internal.State.Instance.Update(delta, new(vpSize.X, vpSize.Y));
        }
    }

    public override void _EnterTree()
    {
        Instance = this;
        _window = GetWindow();

        CheckContentScale();

        string cfgPath = (string)ProjectSettings.GetSetting("addons/imgui/config", "");
        Resource? cfg = null;
        if (ResourceLoader.Exists(cfgPath))
        {
            cfg = ResourceLoader.Load(cfgPath);
            float scale = (float)cfg.Get("Scale");
            bool cfgok = scale > 0.0f;

            if (!cfgok)
            {
                GD.PushError($"imgui-godot: config not a valid ImGuiConfig resource: {cfgPath}");
                cfg = null;
            }
        }
        else if (cfgPath.Length > 0)
        {
            GD.PushError($"imgui-godot: config does not exist: {cfgPath}");
        }

        Internal.State.Init(cfg ?? (Resource)((GDScript)GD.Load(
                "res://addons/imgui-godot/scripts/ImGuiConfig.gd")).New());

        _helper = new ImGuiControllerHelper();
        AddChild(_helper);

        Signaler = GetParent();
        SetMainViewport(_window);
    }

    public override void _Ready()
    {
        ProcessPriority = int.MaxValue;
        ProcessMode = ProcessModeEnum.Always;
    }

    public override void _ExitTree()
    {
        Internal.State.Instance.Dispose();
    }

    public override void _Process(double delta)
    {
        Signaler.EmitSignal(_signalName);
        Internal.State.Instance.Render();
        Internal.State.Instance.InProcessFrame = false;
    }

    public override void _Notification(int what)
    {
        Internal.Input.ProcessNotification(what);
    }

    public void OnLayerExiting()
    {
        // an ImGuiLayer is being destroyed without calling SetMainViewport
        if (Internal.State.Instance.Layer.GetViewport() != _window)
        {
            // revert to main window
            SetMainViewport(_window);
        }
    }

    public void SetMainViewport(Viewport vp)
    {
        ImGuiLayer? oldLayer = Internal.State.Instance.Layer;
        if (oldLayer != null)
        {
            oldLayer.TreeExiting -= OnLayerExiting;
            oldLayer.QueueFree();
        }

        var newLayer = new ImGuiLayer();
        newLayer.TreeExiting += OnLayerExiting;

        if (vp is Window window)
        {
            Internal.State.Instance.Input = new Internal.Input();
            if (window == _window)
                AddChild(newLayer);
            else
                window.AddChild(newLayer);
            ImGui.GetIO().BackendFlags |= ImGuiBackendFlags.PlatformHasViewports
                | ImGuiBackendFlags.HasMouseHoveredViewport;
        }
        else if (vp is SubViewport svp)
        {
            Internal.State.Instance.Input = new Internal.InputLocal();
            svp.AddChild(newLayer);
            ImGui.GetIO().BackendFlags &= ~ImGuiBackendFlags.PlatformHasViewports;
            ImGui.GetIO().BackendFlags &= ~ImGuiBackendFlags.HasMouseHoveredViewport;
        }
        else
        {
            throw new System.ArgumentException("secret third kind of viewport??", nameof(vp));
        }
        Internal.State.Instance.Layer = newLayer;
    }

    private void CheckContentScale()
    {
        if (_window.ContentScaleMode == Window.ContentScaleModeEnum.Viewport)
        {
            GD.PrintErr("imgui-godot: scale mode `viewport` is unsupported");
        }
    }

    public static void WindowInputCallback(InputEvent evt)
    {
        Internal.State.Instance.Input.ProcessInput(evt);
    }
}
#endif
