using System.Collections.Generic;
using System.Linq;
using Godot;
using SpacetimeDB.Types;

public partial class PlayerController : Node
{
    const int SEND_UPDATES_PER_SEC = 20;
    const float SEND_UPDATES_FREQUENCY = 1f / SEND_UPDATES_PER_SEC;

    public static PlayerController Local { get; private set; }

    private int _playerId;
    private float _lastMovementSendTimestamp;
    private Vector2? _lockInputPosition;
    private readonly List<CircleController> _ownedCircles = new();

    // Automated testing members.
    private bool _testInputEnabled;
    private Vector2 _testInput;
    private bool _lockInputTogglePressed;

    public string Username => GameManager.Conn.Db.Player.PlayerId.Find(_playerId).Name;
    public int NumberOfOwnedCircles => _ownedCircles.Count;
    public bool IsLocalPlayer => this == Local;

    public void Initialize(Player player)
    {
        _playerId = player.PlayerId;
        if (player.Identity == GameManager.LocalIdentity)
        {
            Local = this;
        }
    }

    public override void _ExitTree()
    {
        foreach (var circle in _ownedCircles.ToList())
        {
            if (GodotObject.IsInstanceValid(circle))
            {
                circle.QueueFree();
            }
        }

        _ownedCircles.Clear();
        if (Local == this)
        {
            Local = null;
        }
    }

    public void OnCircleSpawned(CircleController circle)
    {
        _ownedCircles.Add(circle);
    }

    public void OnCircleDeleted(CircleController deletedCircle)
    {
        _ownedCircles.Remove(deletedCircle);
    }

    public int TotalMass()
    {
        return (int)_ownedCircles
            .Select(circle => GameManager.Conn.Db.Entity.EntityId.Find(circle.EntityId))
            .Sum(entity => entity?.Mass ?? 0);
    }

    public Vector2? CenterOfMass()
    {
        if (_ownedCircles.Count == 0)
        {
            return null;
        }

        Vector2 totalPos = Vector2.Zero;
        float totalMass = 0.0f;
        foreach (var circle in _ownedCircles)
        {
            var entity = GameManager.Conn.Db.Entity.EntityId.Find(circle.EntityId);
            if (entity == null)
            {
                continue;
            }

            totalPos += circle.GlobalPosition * entity.Mass;
            totalMass += entity.Mass;
        }

        return totalMass > 0.0f ? totalPos / totalMass : null;
    }

    public override void _Process(double delta)
    {
        if (!IsLocalPlayer || NumberOfOwnedCircles == 0 || !GameManager.IsConnected())
        {
            return;
        }

        var lockTogglePressed = Input.IsPhysicalKeyPressed(Key.Q);
        if (lockTogglePressed && !_lockInputTogglePressed)
        {
            if (_lockInputPosition.HasValue)
            {
                _lockInputPosition = null;
            }
            else
            {
                _lockInputPosition = GetViewport().GetMousePosition();
            }
        }
        _lockInputTogglePressed = lockTogglePressed;

        var nowSeconds = Time.GetTicksMsec() / 1000.0f;
        if (nowSeconds - _lastMovementSendTimestamp < SEND_UPDATES_FREQUENCY)
        {
            return;
        }

        _lastMovementSendTimestamp = nowSeconds;

        var mousePosition = _lockInputPosition ?? GetViewport().GetMousePosition();
        var screenSize = GetViewport().GetVisibleRect().Size;
        var centerOfScreen = screenSize / 2.0f;
        var direction = (mousePosition - centerOfScreen) / (screenSize.Y / 3.0f);

        if (_testInputEnabled)
        {
            direction = _testInput;
        }

        GameManager.Conn.Reducers.UpdatePlayerInput(direction);
    }

    public void SetTestInput(Vector2 input) => _testInput = input;
    public void EnableTestInput() => _testInputEnabled = true;
}
