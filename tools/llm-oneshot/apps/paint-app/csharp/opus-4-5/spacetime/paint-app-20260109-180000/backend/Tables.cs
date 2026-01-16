using SpacetimeDB;

namespace PaintApp;

// ============================================================================
// USER & PRESENCE
// ============================================================================

[SpacetimeDB.Table(Name = "user", Public = true)]
public partial struct User
{
    [SpacetimeDB.PrimaryKey]
    public Identity Identity;
    
    public string DisplayName;
    public bool Online;
    public Timestamp LastActive;
}

[SpacetimeDB.Table(Name = "cursor", Public = true)]
public partial struct Cursor
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    public Identity UserId;
    
    [SpacetimeDB.Index.BTree]
    public ulong CanvasId;
    
    public double X;
    public double Y;
    public string Tool; // "brush", "eraser", "shape", "text", "select"
    public Timestamp LastUpdate;
}

// ============================================================================
// CANVAS & MEMBERSHIP
// ============================================================================

[SpacetimeDB.Table(Name = "canvas", Public = true)]
public partial struct Canvas
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    public string Name;
    public Identity CreatorId;
    public bool IsPrivate;
    public int Width;
    public int Height;
    public string BackgroundColor;
    public Timestamp CreatedAt;
    public Timestamp UpdatedAt;
}

[SpacetimeDB.Table(Name = "canvas_member", Public = true)]
public partial struct CanvasMember
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    [SpacetimeDB.Index.BTree]
    public ulong CanvasId;
    
    public Identity UserId;
    public string Role; // "owner", "editor", "viewer"
    public bool IsPresent;
    public Timestamp JoinedAt;
}

[SpacetimeDB.Table(Name = "invitation", Public = true)]
public partial struct Invitation
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    [SpacetimeDB.Index.BTree]
    public ulong CanvasId;
    
    public Identity InviterId;
    public Identity InviteeId;
    public string Status; // "pending", "accepted", "declined"
    public Timestamp CreatedAt;
}

// ============================================================================
// LAYERS
// ============================================================================

[SpacetimeDB.Table(Name = "layer", Public = true)]
public partial struct Layer
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    [SpacetimeDB.Index.BTree]
    public ulong CanvasId;
    
    public string Name;
    public int ZOrder;
    public bool Visible;
    public double Opacity;
    public bool Locked;
    public Timestamp CreatedAt;
}

// ============================================================================
// DRAWING ELEMENTS
// ============================================================================

[SpacetimeDB.Table(Name = "stroke", Public = true)]
public partial struct Stroke
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    [SpacetimeDB.Index.BTree]
    public ulong CanvasId;
    
    [SpacetimeDB.Index.BTree]
    public ulong LayerId;
    
    public Identity CreatorId;
    public string PointsJson; // JSON array of {x, y} points
    public string Color;
    public double Size;
    public double Opacity;
    public string Tool; // "brush", "eraser"
    public Timestamp CreatedAt;
}

[SpacetimeDB.Table(Name = "shape", Public = true)]
public partial struct Shape
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    [SpacetimeDB.Index.BTree]
    public ulong CanvasId;
    
    [SpacetimeDB.Index.BTree]
    public ulong LayerId;
    
    public Identity CreatorId;
    public string ShapeType; // "rectangle", "ellipse", "line"
    public double X;
    public double Y;
    public double Width;
    public double Height;
    public double Rotation;
    public string StrokeColor;
    public string FillColor;
    public double StrokeWidth;
    public Timestamp CreatedAt;
}

[SpacetimeDB.Table(Name = "text_element", Public = true)]
public partial struct TextElement
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    [SpacetimeDB.Index.BTree]
    public ulong CanvasId;
    
    [SpacetimeDB.Index.BTree]
    public ulong LayerId;
    
    public Identity CreatorId;
    public string Content;
    public double X;
    public double Y;
    public double FontSize;
    public string FontFamily;
    public string Color;
    public double Rotation;
    public Timestamp CreatedAt;
    public Timestamp UpdatedAt;
}

[SpacetimeDB.Table(Name = "image_element", Public = true)]
public partial struct ImageElement
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    [SpacetimeDB.Index.BTree]
    public ulong CanvasId;
    
    [SpacetimeDB.Index.BTree]
    public ulong LayerId;
    
    public Identity CreatorId;
    public string Base64Data; // Base64 encoded image data
    public string MimeType;
    public double X;
    public double Y;
    public double Width;
    public double Height;
    public double Rotation;
    public Timestamp CreatedAt;
}

// ============================================================================
// FILL OPERATIONS
// ============================================================================

[SpacetimeDB.Table(Name = "fill", Public = true)]
public partial struct Fill
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    [SpacetimeDB.Index.BTree]
    public ulong CanvasId;
    
    [SpacetimeDB.Index.BTree]
    public ulong LayerId;
    
    public Identity CreatorId;
    public double X;
    public double Y;
    public string Color;
    public int Tolerance;
    public Timestamp CreatedAt;
}

// ============================================================================
// SELECTION
// ============================================================================

[SpacetimeDB.Table(Name = "selection", Public = true)]
public partial struct Selection
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    public Identity UserId;
    
    [SpacetimeDB.Index.BTree]
    public ulong CanvasId;
    
    public string ElementType; // "stroke", "shape", "text", "image"
    public ulong ElementId;
    public Timestamp CreatedAt;
}

// ============================================================================
// UNDO/REDO
// ============================================================================

[SpacetimeDB.Table(Name = "undo_action", Public = true)]
public partial struct UndoAction
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    public Identity UserId;
    
    [SpacetimeDB.Index.BTree]
    public ulong CanvasId;
    
    public string ActionType; // "create", "delete", "update"
    public string ElementType; // "stroke", "shape", "text", "image", "fill"
    public ulong ElementId;
    public string PreviousStateJson; // JSON snapshot of element before action
    public int SequenceNumber;
    public bool IsUndone;
    public Timestamp CreatedAt;
}

// ============================================================================
// COMMENTS
// ============================================================================

[SpacetimeDB.Table(Name = "comment", Public = true)]
public partial struct Comment
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    [SpacetimeDB.Index.BTree]
    public ulong CanvasId;
    
    public Identity AuthorId;
    public double X;
    public double Y;
    public string Content;
    public bool IsResolved;
    public Timestamp CreatedAt;
    public Timestamp UpdatedAt;
}

[SpacetimeDB.Table(Name = "comment_reply", Public = true)]
public partial struct CommentReply
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    [SpacetimeDB.Index.BTree]
    public ulong CommentId;
    
    public Identity AuthorId;
    public string Content;
    public Timestamp CreatedAt;
}

// ============================================================================
// VERSION HISTORY
// ============================================================================

[SpacetimeDB.Table(Name = "canvas_version", Public = true)]
public partial struct CanvasVersion
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    [SpacetimeDB.Index.BTree]
    public ulong CanvasId;
    
    public Identity CreatorId;
    public string Name;
    public string SnapshotJson; // Full canvas state snapshot
    public bool IsAutoSave;
    public Timestamp CreatedAt;
}

// ============================================================================
// SCHEDULED AUTOSAVE
// ============================================================================

[SpacetimeDB.Table(Name = "autosave_job", Scheduled = "ProcessAutoSave")]
public partial struct AutoSaveJob
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong ScheduledId;
    
    public ScheduleAt ScheduledAt;
    public ulong CanvasId;
}

// ============================================================================
// TEMPLATES
// ============================================================================

[SpacetimeDB.Table(Name = "template", Public = true)]
public partial struct Template
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    public Identity CreatorId;
    public string Name;
    public string Description;
    public string Category;
    public string SnapshotJson; // Template canvas state
    public bool IsPublic;
    public Timestamp CreatedAt;
}

// ============================================================================
// VIEWPORT (for follow mode)
// ============================================================================

[SpacetimeDB.Table(Name = "viewport", Public = true)]
public partial struct Viewport
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    public Identity UserId;
    
    [SpacetimeDB.Index.BTree]
    public ulong CanvasId;
    
    public double PanX;
    public double PanY;
    public double Zoom;
    public Timestamp LastUpdate;
}
