using SpacetimeDB;

namespace PaintApp;

public static partial class Module
{
    // ========================================================================
    // LIFECYCLE HOOKS
    // ========================================================================

    [SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        var existing = ctx.Db.user.Identity.Find(ctx.Sender);
        if (existing != null)
        {
            var user = existing.Value;
            ctx.Db.user.Identity.Update(new User
            {
                Identity = user.Identity,
                DisplayName = user.DisplayName,
                Online = true,
                LastActive = ctx.Timestamp
            });
        }
        else
        {
            ctx.Db.user.Insert(new User
            {
                Identity = ctx.Sender,
                DisplayName = "",
                Online = true,
                LastActive = ctx.Timestamp
            });
        }
        Log.Info($"Client connected: {ctx.Sender}");
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
    public static void ClientDisconnected(ReducerContext ctx)
    {
        var existing = ctx.Db.user.Identity.Find(ctx.Sender);
        if (existing != null)
        {
            var user = existing.Value;
            ctx.Db.user.Identity.Update(new User
            {
                Identity = user.Identity,
                DisplayName = user.DisplayName,
                Online = false,
                LastActive = ctx.Timestamp
            });
        }

        // Remove cursor
        foreach (var cursor in ctx.Db.cursor.Iter().Where(c => c.UserId == ctx.Sender).ToList())
        {
            ctx.Db.cursor.Id.Delete(cursor.Id);
        }

        // Mark as not present in canvases
        foreach (var member in ctx.Db.canvas_member.Iter().Where(m => m.UserId == ctx.Sender).ToList())
        {
            ctx.Db.canvas_member.Id.Update(new CanvasMember
            {
                Id = member.Id,
                CanvasId = member.CanvasId,
                UserId = member.UserId,
                Role = member.Role,
                IsPresent = false,
                JoinedAt = member.JoinedAt
            });
        }

        Log.Info($"Client disconnected: {ctx.Sender}");
    }

    // ========================================================================
    // USER MANAGEMENT
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void SetDisplayName(ReducerContext ctx, string displayName)
    {
        if (string.IsNullOrWhiteSpace(displayName))
        {
            throw new ArgumentException("Display name cannot be empty");
        }

        var existing = ctx.Db.user.Identity.Find(ctx.Sender);
        if (existing == null)
        {
            throw new Exception("User not found");
        }

        var user = existing.Value;
        ctx.Db.user.Identity.Update(new User
        {
            Identity = user.Identity,
            DisplayName = displayName.Trim(),
            Online = user.Online,
            LastActive = ctx.Timestamp
        });
    }

    // ========================================================================
    // CANVAS MANAGEMENT
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void CreateCanvas(ReducerContext ctx, string name, bool isPrivate, int width, int height, string backgroundColor)
    {
        if (string.IsNullOrWhiteSpace(name))
        {
            throw new ArgumentException("Canvas name cannot be empty");
        }

        var canvas = ctx.Db.canvas.Insert(new Canvas
        {
            Id = 0,
            Name = name.Trim(),
            CreatorId = ctx.Sender,
            IsPrivate = isPrivate,
            Width = width > 0 ? width : 1920,
            Height = height > 0 ? height : 1080,
            BackgroundColor = string.IsNullOrEmpty(backgroundColor) ? "#0a0a0f" : backgroundColor,
            CreatedAt = ctx.Timestamp,
            UpdatedAt = ctx.Timestamp
        });

        // Add creator as owner
        ctx.Db.canvas_member.Insert(new CanvasMember
        {
            Id = 0,
            CanvasId = canvas.Id,
            UserId = ctx.Sender,
            Role = "owner",
            IsPresent = true,
            JoinedAt = ctx.Timestamp
        });

        // Create default layer
        ctx.Db.layer.Insert(new Layer
        {
            Id = 0,
            CanvasId = canvas.Id,
            Name = "Layer 1",
            ZOrder = 0,
            Visible = true,
            Opacity = 1.0,
            Locked = false,
            CreatedAt = ctx.Timestamp
        });

        // Schedule auto-save every 5 minutes
        ScheduleAutoSave(ctx, canvas.Id, 300000);

        Log.Info($"Canvas created: {canvas.Id} by {ctx.Sender}");
    }

    [SpacetimeDB.Reducer]
    public static void CreateCanvasFromTemplate(ReducerContext ctx, ulong templateId, string name, bool isPrivate)
    {
        var template = ctx.Db.template.Id.Find(templateId);
        if (template == null)
        {
            throw new Exception("Template not found");
        }

        var tpl = template.Value;
        if (!tpl.IsPublic && tpl.CreatorId != ctx.Sender)
        {
            throw new Exception("Cannot use private template from another user");
        }

        var canvas = ctx.Db.canvas.Insert(new Canvas
        {
            Id = 0,
            Name = name.Trim(),
            CreatorId = ctx.Sender,
            IsPrivate = isPrivate,
            Width = 1920,
            Height = 1080,
            BackgroundColor = "#0a0a0f",
            CreatedAt = ctx.Timestamp,
            UpdatedAt = ctx.Timestamp
        });

        ctx.Db.canvas_member.Insert(new CanvasMember
        {
            Id = 0,
            CanvasId = canvas.Id,
            UserId = ctx.Sender,
            Role = "owner",
            IsPresent = true,
            JoinedAt = ctx.Timestamp
        });

        // Create default layer (template elements would be loaded client-side)
        ctx.Db.layer.Insert(new Layer
        {
            Id = 0,
            CanvasId = canvas.Id,
            Name = "Layer 1",
            ZOrder = 0,
            Visible = true,
            Opacity = 1.0,
            Locked = false,
            CreatedAt = ctx.Timestamp
        });

        ScheduleAutoSave(ctx, canvas.Id, 300000);
    }

    [SpacetimeDB.Reducer]
    public static void DeleteCanvas(ReducerContext ctx, ulong canvasId)
    {
        var canvas = ctx.Db.canvas.Id.Find(canvasId);
        if (canvas == null)
        {
            throw new Exception("Canvas not found");
        }

        if (canvas.Value.CreatorId != ctx.Sender)
        {
            throw new Exception("Only the creator can delete this canvas");
        }

        // Delete all related data
        foreach (var stroke in ctx.Db.stroke.CanvasId.Filter(canvasId).ToList())
            ctx.Db.stroke.Id.Delete(stroke.Id);
        foreach (var shape in ctx.Db.shape.CanvasId.Filter(canvasId).ToList())
            ctx.Db.shape.Id.Delete(shape.Id);
        foreach (var text in ctx.Db.text_element.CanvasId.Filter(canvasId).ToList())
            ctx.Db.text_element.Id.Delete(text.Id);
        foreach (var img in ctx.Db.image_element.CanvasId.Filter(canvasId).ToList())
            ctx.Db.image_element.Id.Delete(img.Id);
        foreach (var fill in ctx.Db.fill.CanvasId.Filter(canvasId).ToList())
            ctx.Db.fill.Id.Delete(fill.Id);
        foreach (var layer in ctx.Db.layer.CanvasId.Filter(canvasId).ToList())
            ctx.Db.layer.Id.Delete(layer.Id);
        foreach (var member in ctx.Db.canvas_member.CanvasId.Filter(canvasId).ToList())
            ctx.Db.canvas_member.Id.Delete(member.Id);
        foreach (var cursor in ctx.Db.cursor.CanvasId.Filter(canvasId).ToList())
            ctx.Db.cursor.Id.Delete(cursor.Id);
        foreach (var comment in ctx.Db.comment.CanvasId.Filter(canvasId).ToList())
        {
            foreach (var reply in ctx.Db.comment_reply.CommentId.Filter(comment.Id).ToList())
                ctx.Db.comment_reply.Id.Delete(reply.Id);
            ctx.Db.comment.Id.Delete(comment.Id);
        }
        foreach (var version in ctx.Db.canvas_version.CanvasId.Filter(canvasId).ToList())
            ctx.Db.canvas_version.Id.Delete(version.Id);
        foreach (var invitation in ctx.Db.invitation.CanvasId.Filter(canvasId).ToList())
            ctx.Db.invitation.Id.Delete(invitation.Id);

        ctx.Db.canvas.Id.Delete(canvasId);
    }

    [SpacetimeDB.Reducer]
    public static void JoinCanvas(ReducerContext ctx, ulong canvasId)
    {
        var canvas = ctx.Db.canvas.Id.Find(canvasId);
        if (canvas == null)
        {
            throw new Exception("Canvas not found");
        }

        var c = canvas.Value;

        // Check if private and user has access
        if (c.IsPrivate)
        {
            var member = ctx.Db.canvas_member.CanvasId.Filter(canvasId)
                .FirstOrDefault(m => m.UserId == ctx.Sender);
            if (member.Id == 0 && member.CanvasId == 0)
            {
                throw new Exception("This canvas is private. You need an invitation to join.");
            }
        }

        // Check if already a member
        var existingMember = ctx.Db.canvas_member.CanvasId.Filter(canvasId)
            .FirstOrDefault(m => m.UserId == ctx.Sender);

        if (existingMember.Id != 0 || existingMember.CanvasId != 0)
        {
            ctx.Db.canvas_member.Id.Update(new CanvasMember
            {
                Id = existingMember.Id,
                CanvasId = existingMember.CanvasId,
                UserId = existingMember.UserId,
                Role = existingMember.Role,
                IsPresent = true,
                JoinedAt = existingMember.JoinedAt
            });
        }
        else
        {
            ctx.Db.canvas_member.Insert(new CanvasMember
            {
                Id = 0,
                CanvasId = canvasId,
                UserId = ctx.Sender,
                Role = c.IsPrivate ? "viewer" : "editor",
                IsPresent = true,
                JoinedAt = ctx.Timestamp
            });
        }
    }

    [SpacetimeDB.Reducer]
    public static void LeaveCanvas(ReducerContext ctx, ulong canvasId)
    {
        var member = ctx.Db.canvas_member.CanvasId.Filter(canvasId)
            .FirstOrDefault(m => m.UserId == ctx.Sender);

        if (member.Id != 0 || member.CanvasId != 0)
        {
            ctx.Db.canvas_member.Id.Update(new CanvasMember
            {
                Id = member.Id,
                CanvasId = member.CanvasId,
                UserId = member.UserId,
                Role = member.Role,
                IsPresent = false,
                JoinedAt = member.JoinedAt
            });
        }

        // Remove cursor
        foreach (var cursor in ctx.Db.cursor.CanvasId.Filter(canvasId).Where(c => c.UserId == ctx.Sender).ToList())
        {
            ctx.Db.cursor.Id.Delete(cursor.Id);
        }

        // Clear selections
        foreach (var sel in ctx.Db.selection.CanvasId.Filter(canvasId).Where(s => s.UserId == ctx.Sender).ToList())
        {
            ctx.Db.selection.Id.Delete(sel.Id);
        }
    }

    // ========================================================================
    // PERMISSIONS
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void SetUserRole(ReducerContext ctx, ulong canvasId, Identity userId, string role)
    {
        if (role != "editor" && role != "viewer")
        {
            throw new ArgumentException("Role must be 'editor' or 'viewer'");
        }

        var canvas = ctx.Db.canvas.Id.Find(canvasId);
        if (canvas == null || canvas.Value.CreatorId != ctx.Sender)
        {
            throw new Exception("Only the canvas creator can change roles");
        }

        var member = ctx.Db.canvas_member.CanvasId.Filter(canvasId)
            .FirstOrDefault(m => m.UserId == userId);

        if (member.Id == 0 && member.CanvasId == 0)
        {
            throw new Exception("User is not a member of this canvas");
        }

        if (member.Role == "owner")
        {
            throw new Exception("Cannot change the owner's role");
        }

        ctx.Db.canvas_member.Id.Update(new CanvasMember
        {
            Id = member.Id,
            CanvasId = member.CanvasId,
            UserId = member.UserId,
            Role = role,
            IsPresent = member.IsPresent,
            JoinedAt = member.JoinedAt
        });
    }

    [SpacetimeDB.Reducer]
    public static void KickUser(ReducerContext ctx, ulong canvasId, Identity userId)
    {
        var canvas = ctx.Db.canvas.Id.Find(canvasId);
        if (canvas == null || canvas.Value.CreatorId != ctx.Sender)
        {
            throw new Exception("Only the canvas creator can kick users");
        }

        if (userId == ctx.Sender)
        {
            throw new Exception("Cannot kick yourself");
        }

        var member = ctx.Db.canvas_member.CanvasId.Filter(canvasId)
            .FirstOrDefault(m => m.UserId == userId);

        if (member.Id != 0 || member.CanvasId != 0)
        {
            ctx.Db.canvas_member.Id.Delete(member.Id);
        }

        // Remove cursor
        foreach (var cursor in ctx.Db.cursor.CanvasId.Filter(canvasId).Where(c => c.UserId == userId).ToList())
        {
            ctx.Db.cursor.Id.Delete(cursor.Id);
        }
    }

    // ========================================================================
    // INVITATIONS (Private Canvases)
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void InviteUser(ReducerContext ctx, ulong canvasId, Identity inviteeId)
    {
        var canvas = ctx.Db.canvas.Id.Find(canvasId);
        if (canvas == null)
        {
            throw new Exception("Canvas not found");
        }

        var c = canvas.Value;
        if (c.CreatorId != ctx.Sender)
        {
            throw new Exception("Only the canvas creator can invite users");
        }

        // Check if already invited
        var existing = ctx.Db.invitation.CanvasId.Filter(canvasId)
            .FirstOrDefault(i => i.InviteeId == inviteeId && i.Status == "pending");

        if (existing.Id != 0)
        {
            throw new Exception("User already has a pending invitation");
        }

        ctx.Db.invitation.Insert(new Invitation
        {
            Id = 0,
            CanvasId = canvasId,
            InviterId = ctx.Sender,
            InviteeId = inviteeId,
            Status = "pending",
            CreatedAt = ctx.Timestamp
        });
    }

    [SpacetimeDB.Reducer]
    public static void RespondToInvitation(ReducerContext ctx, ulong invitationId, bool accept)
    {
        var invitation = ctx.Db.invitation.Id.Find(invitationId);
        if (invitation == null)
        {
            throw new Exception("Invitation not found");
        }

        var inv = invitation.Value;
        if (inv.InviteeId != ctx.Sender)
        {
            throw new Exception("This invitation is not for you");
        }

        if (inv.Status != "pending")
        {
            throw new Exception("Invitation already responded to");
        }

        ctx.Db.invitation.Id.Update(new Invitation
        {
            Id = inv.Id,
            CanvasId = inv.CanvasId,
            InviterId = inv.InviterId,
            InviteeId = inv.InviteeId,
            Status = accept ? "accepted" : "declined",
            CreatedAt = inv.CreatedAt
        });

        if (accept)
        {
            // Add as member with editor role
            ctx.Db.canvas_member.Insert(new CanvasMember
            {
                Id = 0,
                CanvasId = inv.CanvasId,
                UserId = ctx.Sender,
                Role = "editor",
                IsPresent = false,
                JoinedAt = ctx.Timestamp
            });
        }
    }

    // ========================================================================
    // CURSOR / PRESENCE
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void UpdateCursor(ReducerContext ctx, ulong canvasId, double x, double y, string tool)
    {
        var existing = ctx.Db.cursor.CanvasId.Filter(canvasId)
            .FirstOrDefault(c => c.UserId == ctx.Sender);

        if (existing.Id != 0 || existing.CanvasId != 0)
        {
            ctx.Db.cursor.Id.Update(new Cursor
            {
                Id = existing.Id,
                UserId = ctx.Sender,
                CanvasId = canvasId,
                X = x,
                Y = y,
                Tool = tool,
                LastUpdate = ctx.Timestamp
            });
        }
        else
        {
            ctx.Db.cursor.Insert(new Cursor
            {
                Id = 0,
                UserId = ctx.Sender,
                CanvasId = canvasId,
                X = x,
                Y = y,
                Tool = tool,
                LastUpdate = ctx.Timestamp
            });
        }
    }

    // ========================================================================
    // LAYERS
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void CreateLayer(ReducerContext ctx, ulong canvasId, string name)
    {
        CheckCanEdit(ctx, canvasId);

        var maxOrder = ctx.Db.layer.CanvasId.Filter(canvasId)
            .Select(l => l.ZOrder)
            .DefaultIfEmpty(-1)
            .Max();

        ctx.Db.layer.Insert(new Layer
        {
            Id = 0,
            CanvasId = canvasId,
            Name = string.IsNullOrEmpty(name) ? $"Layer {maxOrder + 2}" : name,
            ZOrder = maxOrder + 1,
            Visible = true,
            Opacity = 1.0,
            Locked = false,
            CreatedAt = ctx.Timestamp
        });

        UpdateCanvasTimestamp(ctx, canvasId);
    }

    [SpacetimeDB.Reducer]
    public static void UpdateLayer(ReducerContext ctx, ulong layerId, string name, bool visible, double opacity, bool locked)
    {
        var layer = ctx.Db.layer.Id.Find(layerId);
        if (layer == null)
        {
            throw new Exception("Layer not found");
        }

        var l = layer.Value;
        CheckCanEdit(ctx, l.CanvasId);

        ctx.Db.layer.Id.Update(new Layer
        {
            Id = l.Id,
            CanvasId = l.CanvasId,
            Name = string.IsNullOrEmpty(name) ? l.Name : name,
            ZOrder = l.ZOrder,
            Visible = visible,
            Opacity = Math.Clamp(opacity, 0, 1),
            Locked = locked,
            CreatedAt = l.CreatedAt
        });

        UpdateCanvasTimestamp(ctx, l.CanvasId);
    }

    [SpacetimeDB.Reducer]
    public static void ReorderLayer(ReducerContext ctx, ulong layerId, int newZOrder)
    {
        var layer = ctx.Db.layer.Id.Find(layerId);
        if (layer == null)
        {
            throw new Exception("Layer not found");
        }

        var l = layer.Value;
        CheckCanEdit(ctx, l.CanvasId);

        ctx.Db.layer.Id.Update(new Layer
        {
            Id = l.Id,
            CanvasId = l.CanvasId,
            Name = l.Name,
            ZOrder = newZOrder,
            Visible = l.Visible,
            Opacity = l.Opacity,
            Locked = l.Locked,
            CreatedAt = l.CreatedAt
        });

        UpdateCanvasTimestamp(ctx, l.CanvasId);
    }

    [SpacetimeDB.Reducer]
    public static void DeleteLayer(ReducerContext ctx, ulong layerId)
    {
        var layer = ctx.Db.layer.Id.Find(layerId);
        if (layer == null)
        {
            throw new Exception("Layer not found");
        }

        var l = layer.Value;
        CheckCanEdit(ctx, l.CanvasId);

        // Delete all elements on this layer
        foreach (var stroke in ctx.Db.stroke.LayerId.Filter(layerId).ToList())
            ctx.Db.stroke.Id.Delete(stroke.Id);
        foreach (var shape in ctx.Db.shape.LayerId.Filter(layerId).ToList())
            ctx.Db.shape.Id.Delete(shape.Id);
        foreach (var text in ctx.Db.text_element.LayerId.Filter(layerId).ToList())
            ctx.Db.text_element.Id.Delete(text.Id);
        foreach (var img in ctx.Db.image_element.LayerId.Filter(layerId).ToList())
            ctx.Db.image_element.Id.Delete(img.Id);
        foreach (var fill in ctx.Db.fill.LayerId.Filter(layerId).ToList())
            ctx.Db.fill.Id.Delete(fill.Id);

        ctx.Db.layer.Id.Delete(layerId);

        UpdateCanvasTimestamp(ctx, l.CanvasId);
    }

    // ========================================================================
    // DRAWING - STROKES
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void AddStroke(ReducerContext ctx, ulong canvasId, ulong layerId, string pointsJson, string color, double size, double opacity, string tool)
    {
        CheckCanEdit(ctx, canvasId);
        CheckLayerNotLocked(ctx, layerId);

        var stroke = ctx.Db.stroke.Insert(new Stroke
        {
            Id = 0,
            CanvasId = canvasId,
            LayerId = layerId,
            CreatorId = ctx.Sender,
            PointsJson = pointsJson,
            Color = color,
            Size = size,
            Opacity = opacity,
            Tool = tool,
            CreatedAt = ctx.Timestamp
        });

        AddUndoAction(ctx, canvasId, "create", "stroke", stroke.Id, "");
        UpdateCanvasTimestamp(ctx, canvasId);
    }

    [SpacetimeDB.Reducer]
    public static void DeleteStroke(ReducerContext ctx, ulong strokeId)
    {
        var stroke = ctx.Db.stroke.Id.Find(strokeId);
        if (stroke == null)
        {
            throw new Exception("Stroke not found");
        }

        var s = stroke.Value;
        CheckCanEdit(ctx, s.CanvasId);

        var snapshotJson = $"{{\"canvasId\":{s.CanvasId},\"layerId\":{s.LayerId},\"pointsJson\":\"{EscapeJson(s.PointsJson)}\",\"color\":\"{s.Color}\",\"size\":{s.Size},\"opacity\":{s.Opacity},\"tool\":\"{s.Tool}\"}}";
        AddUndoAction(ctx, s.CanvasId, "delete", "stroke", strokeId, snapshotJson);

        ctx.Db.stroke.Id.Delete(strokeId);
        UpdateCanvasTimestamp(ctx, s.CanvasId);
    }

    // ========================================================================
    // DRAWING - SHAPES
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void AddShape(ReducerContext ctx, ulong canvasId, ulong layerId, string shapeType, double x, double y, double width, double height, double rotation, string strokeColor, string fillColor, double strokeWidth)
    {
        CheckCanEdit(ctx, canvasId);
        CheckLayerNotLocked(ctx, layerId);

        var shape = ctx.Db.shape.Insert(new Shape
        {
            Id = 0,
            CanvasId = canvasId,
            LayerId = layerId,
            CreatorId = ctx.Sender,
            ShapeType = shapeType,
            X = x,
            Y = y,
            Width = width,
            Height = height,
            Rotation = rotation,
            StrokeColor = strokeColor,
            FillColor = fillColor,
            StrokeWidth = strokeWidth,
            CreatedAt = ctx.Timestamp
        });

        AddUndoAction(ctx, canvasId, "create", "shape", shape.Id, "");
        UpdateCanvasTimestamp(ctx, canvasId);
    }

    [SpacetimeDB.Reducer]
    public static void UpdateShape(ReducerContext ctx, ulong shapeId, double x, double y, double width, double height, double rotation)
    {
        var shape = ctx.Db.shape.Id.Find(shapeId);
        if (shape == null)
        {
            throw new Exception("Shape not found");
        }

        var s = shape.Value;
        CheckCanEdit(ctx, s.CanvasId);
        CheckLayerNotLocked(ctx, s.LayerId);

        var snapshotJson = $"{{\"x\":{s.X},\"y\":{s.Y},\"width\":{s.Width},\"height\":{s.Height},\"rotation\":{s.Rotation}}}";
        AddUndoAction(ctx, s.CanvasId, "update", "shape", shapeId, snapshotJson);

        ctx.Db.shape.Id.Update(new Shape
        {
            Id = s.Id,
            CanvasId = s.CanvasId,
            LayerId = s.LayerId,
            CreatorId = s.CreatorId,
            ShapeType = s.ShapeType,
            X = x,
            Y = y,
            Width = width,
            Height = height,
            Rotation = rotation,
            StrokeColor = s.StrokeColor,
            FillColor = s.FillColor,
            StrokeWidth = s.StrokeWidth,
            CreatedAt = s.CreatedAt
        });

        UpdateCanvasTimestamp(ctx, s.CanvasId);
    }

    [SpacetimeDB.Reducer]
    public static void DeleteShape(ReducerContext ctx, ulong shapeId)
    {
        var shape = ctx.Db.shape.Id.Find(shapeId);
        if (shape == null)
        {
            throw new Exception("Shape not found");
        }

        var s = shape.Value;
        CheckCanEdit(ctx, s.CanvasId);

        var snapshotJson = $"{{\"canvasId\":{s.CanvasId},\"layerId\":{s.LayerId},\"shapeType\":\"{s.ShapeType}\",\"x\":{s.X},\"y\":{s.Y},\"width\":{s.Width},\"height\":{s.Height},\"rotation\":{s.Rotation},\"strokeColor\":\"{s.StrokeColor}\",\"fillColor\":\"{s.FillColor}\",\"strokeWidth\":{s.StrokeWidth}}}";
        AddUndoAction(ctx, s.CanvasId, "delete", "shape", shapeId, snapshotJson);

        ctx.Db.shape.Id.Delete(shapeId);
        UpdateCanvasTimestamp(ctx, s.CanvasId);
    }

    // ========================================================================
    // DRAWING - TEXT
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void AddText(ReducerContext ctx, ulong canvasId, ulong layerId, string content, double x, double y, double fontSize, string fontFamily, string color)
    {
        CheckCanEdit(ctx, canvasId);
        CheckLayerNotLocked(ctx, layerId);

        var text = ctx.Db.text_element.Insert(new TextElement
        {
            Id = 0,
            CanvasId = canvasId,
            LayerId = layerId,
            CreatorId = ctx.Sender,
            Content = content,
            X = x,
            Y = y,
            FontSize = fontSize > 0 ? fontSize : 16,
            FontFamily = string.IsNullOrEmpty(fontFamily) ? "Arial" : fontFamily,
            Color = string.IsNullOrEmpty(color) ? "#ffffff" : color,
            Rotation = 0,
            CreatedAt = ctx.Timestamp,
            UpdatedAt = ctx.Timestamp
        });

        AddUndoAction(ctx, canvasId, "create", "text", text.Id, "");
        UpdateCanvasTimestamp(ctx, canvasId);
    }

    [SpacetimeDB.Reducer]
    public static void UpdateText(ReducerContext ctx, ulong textId, string content, double x, double y, double fontSize, string color, double rotation)
    {
        var text = ctx.Db.text_element.Id.Find(textId);
        if (text == null)
        {
            throw new Exception("Text not found");
        }

        var t = text.Value;
        CheckCanEdit(ctx, t.CanvasId);
        CheckLayerNotLocked(ctx, t.LayerId);

        var snapshotJson = $"{{\"content\":\"{EscapeJson(t.Content)}\",\"x\":{t.X},\"y\":{t.Y},\"fontSize\":{t.FontSize},\"color\":\"{t.Color}\",\"rotation\":{t.Rotation}}}";
        AddUndoAction(ctx, t.CanvasId, "update", "text", textId, snapshotJson);

        ctx.Db.text_element.Id.Update(new TextElement
        {
            Id = t.Id,
            CanvasId = t.CanvasId,
            LayerId = t.LayerId,
            CreatorId = t.CreatorId,
            Content = content,
            X = x,
            Y = y,
            FontSize = fontSize,
            FontFamily = t.FontFamily,
            Color = color,
            Rotation = rotation,
            CreatedAt = t.CreatedAt,
            UpdatedAt = ctx.Timestamp
        });

        UpdateCanvasTimestamp(ctx, t.CanvasId);
    }

    [SpacetimeDB.Reducer]
    public static void DeleteText(ReducerContext ctx, ulong textId)
    {
        var text = ctx.Db.text_element.Id.Find(textId);
        if (text == null)
        {
            throw new Exception("Text not found");
        }

        var t = text.Value;
        CheckCanEdit(ctx, t.CanvasId);

        var snapshotJson = $"{{\"canvasId\":{t.CanvasId},\"layerId\":{t.LayerId},\"content\":\"{EscapeJson(t.Content)}\",\"x\":{t.X},\"y\":{t.Y},\"fontSize\":{t.FontSize},\"fontFamily\":\"{t.FontFamily}\",\"color\":\"{t.Color}\",\"rotation\":{t.Rotation}}}";
        AddUndoAction(ctx, t.CanvasId, "delete", "text", textId, snapshotJson);

        ctx.Db.text_element.Id.Delete(textId);
        UpdateCanvasTimestamp(ctx, t.CanvasId);
    }

    // ========================================================================
    // DRAWING - IMAGES
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void AddImage(ReducerContext ctx, ulong canvasId, ulong layerId, string base64Data, string mimeType, double x, double y, double width, double height)
    {
        CheckCanEdit(ctx, canvasId);
        CheckLayerNotLocked(ctx, layerId);

        var img = ctx.Db.image_element.Insert(new ImageElement
        {
            Id = 0,
            CanvasId = canvasId,
            LayerId = layerId,
            CreatorId = ctx.Sender,
            Base64Data = base64Data,
            MimeType = mimeType,
            X = x,
            Y = y,
            Width = width,
            Height = height,
            Rotation = 0,
            CreatedAt = ctx.Timestamp
        });

        AddUndoAction(ctx, canvasId, "create", "image", img.Id, "");
        UpdateCanvasTimestamp(ctx, canvasId);
    }

    [SpacetimeDB.Reducer]
    public static void UpdateImage(ReducerContext ctx, ulong imageId, double x, double y, double width, double height, double rotation)
    {
        var image = ctx.Db.image_element.Id.Find(imageId);
        if (image == null)
        {
            throw new Exception("Image not found");
        }

        var i = image.Value;
        CheckCanEdit(ctx, i.CanvasId);
        CheckLayerNotLocked(ctx, i.LayerId);

        var snapshotJson = $"{{\"x\":{i.X},\"y\":{i.Y},\"width\":{i.Width},\"height\":{i.Height},\"rotation\":{i.Rotation}}}";
        AddUndoAction(ctx, i.CanvasId, "update", "image", imageId, snapshotJson);

        ctx.Db.image_element.Id.Update(new ImageElement
        {
            Id = i.Id,
            CanvasId = i.CanvasId,
            LayerId = i.LayerId,
            CreatorId = i.CreatorId,
            Base64Data = i.Base64Data,
            MimeType = i.MimeType,
            X = x,
            Y = y,
            Width = width,
            Height = height,
            Rotation = rotation,
            CreatedAt = i.CreatedAt
        });

        UpdateCanvasTimestamp(ctx, i.CanvasId);
    }

    [SpacetimeDB.Reducer]
    public static void DeleteImage(ReducerContext ctx, ulong imageId)
    {
        var image = ctx.Db.image_element.Id.Find(imageId);
        if (image == null)
        {
            throw new Exception("Image not found");
        }

        var i = image.Value;
        CheckCanEdit(ctx, i.CanvasId);

        // Note: We don't store full base64 in undo as it's too large
        var snapshotJson = $"{{\"canvasId\":{i.CanvasId},\"layerId\":{i.LayerId},\"mimeType\":\"{i.MimeType}\",\"x\":{i.X},\"y\":{i.Y},\"width\":{i.Width},\"height\":{i.Height},\"rotation\":{i.Rotation}}}";
        AddUndoAction(ctx, i.CanvasId, "delete", "image", imageId, snapshotJson);

        ctx.Db.image_element.Id.Delete(imageId);
        UpdateCanvasTimestamp(ctx, i.CanvasId);
    }

    // ========================================================================
    // DRAWING - FILL
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void AddFill(ReducerContext ctx, ulong canvasId, ulong layerId, double x, double y, string color, int tolerance)
    {
        CheckCanEdit(ctx, canvasId);
        CheckLayerNotLocked(ctx, layerId);

        var fill = ctx.Db.fill.Insert(new Fill
        {
            Id = 0,
            CanvasId = canvasId,
            LayerId = layerId,
            CreatorId = ctx.Sender,
            X = x,
            Y = y,
            Color = color,
            Tolerance = tolerance,
            CreatedAt = ctx.Timestamp
        });

        AddUndoAction(ctx, canvasId, "create", "fill", fill.Id, "");
        UpdateCanvasTimestamp(ctx, canvasId);
    }

    // ========================================================================
    // SELECTION
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void SelectElement(ReducerContext ctx, ulong canvasId, string elementType, ulong elementId)
    {
        // Clear previous selections by this user on this canvas
        foreach (var sel in ctx.Db.selection.CanvasId.Filter(canvasId).Where(s => s.UserId == ctx.Sender).ToList())
        {
            ctx.Db.selection.Id.Delete(sel.Id);
        }

        ctx.Db.selection.Insert(new Selection
        {
            Id = 0,
            UserId = ctx.Sender,
            CanvasId = canvasId,
            ElementType = elementType,
            ElementId = elementId,
            CreatedAt = ctx.Timestamp
        });
    }

    [SpacetimeDB.Reducer]
    public static void ClearSelection(ReducerContext ctx, ulong canvasId)
    {
        foreach (var sel in ctx.Db.selection.CanvasId.Filter(canvasId).Where(s => s.UserId == ctx.Sender).ToList())
        {
            ctx.Db.selection.Id.Delete(sel.Id);
        }
    }

    // ========================================================================
    // UNDO/REDO
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void Undo(ReducerContext ctx, ulong canvasId)
    {
        // Find the latest non-undone action by this user
        var action = ctx.Db.undo_action.CanvasId.Filter(canvasId)
            .Where(a => a.UserId == ctx.Sender && !a.IsUndone)
            .OrderByDescending(a => a.SequenceNumber)
            .FirstOrDefault();

        if (action.Id == 0 && action.CanvasId == 0)
        {
            return; // Nothing to undo
        }

        // Mark as undone
        ctx.Db.undo_action.Id.Update(new UndoAction
        {
            Id = action.Id,
            UserId = action.UserId,
            CanvasId = action.CanvasId,
            ActionType = action.ActionType,
            ElementType = action.ElementType,
            ElementId = action.ElementId,
            PreviousStateJson = action.PreviousStateJson,
            SequenceNumber = action.SequenceNumber,
            IsUndone = true,
            CreatedAt = action.CreatedAt
        });

        // Actually perform the undo based on action type
        if (action.ActionType == "create")
        {
            // Undo create = delete the element
            DeleteElementByType(ctx, action.ElementType, action.ElementId);
        }
        // Note: delete and update undos would require restoring from PreviousStateJson
        // which is more complex - simplified for this implementation
    }

    [SpacetimeDB.Reducer]
    public static void Redo(ReducerContext ctx, ulong canvasId)
    {
        // Find the oldest undone action by this user
        var action = ctx.Db.undo_action.CanvasId.Filter(canvasId)
            .Where(a => a.UserId == ctx.Sender && a.IsUndone)
            .OrderBy(a => a.SequenceNumber)
            .FirstOrDefault();

        if (action.Id == 0 && action.CanvasId == 0)
        {
            return; // Nothing to redo
        }

        // Mark as not undone
        ctx.Db.undo_action.Id.Update(new UndoAction
        {
            Id = action.Id,
            UserId = action.UserId,
            CanvasId = action.CanvasId,
            ActionType = action.ActionType,
            ElementType = action.ElementType,
            ElementId = action.ElementId,
            PreviousStateJson = action.PreviousStateJson,
            SequenceNumber = action.SequenceNumber,
            IsUndone = false,
            CreatedAt = action.CreatedAt
        });
    }

    // ========================================================================
    // COMMENTS
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void AddComment(ReducerContext ctx, ulong canvasId, double x, double y, string content)
    {
        if (string.IsNullOrWhiteSpace(content))
        {
            throw new ArgumentException("Comment cannot be empty");
        }

        ctx.Db.comment.Insert(new Comment
        {
            Id = 0,
            CanvasId = canvasId,
            AuthorId = ctx.Sender,
            X = x,
            Y = y,
            Content = content.Trim(),
            IsResolved = false,
            CreatedAt = ctx.Timestamp,
            UpdatedAt = ctx.Timestamp
        });
    }

    [SpacetimeDB.Reducer]
    public static void ReplyToComment(ReducerContext ctx, ulong commentId, string content)
    {
        if (string.IsNullOrWhiteSpace(content))
        {
            throw new ArgumentException("Reply cannot be empty");
        }

        var comment = ctx.Db.comment.Id.Find(commentId);
        if (comment == null)
        {
            throw new Exception("Comment not found");
        }

        ctx.Db.comment_reply.Insert(new CommentReply
        {
            Id = 0,
            CommentId = commentId,
            AuthorId = ctx.Sender,
            Content = content.Trim(),
            CreatedAt = ctx.Timestamp
        });
    }

    [SpacetimeDB.Reducer]
    public static void ResolveComment(ReducerContext ctx, ulong commentId, bool resolved)
    {
        var comment = ctx.Db.comment.Id.Find(commentId);
        if (comment == null)
        {
            throw new Exception("Comment not found");
        }

        var c = comment.Value;
        ctx.Db.comment.Id.Update(new Comment
        {
            Id = c.Id,
            CanvasId = c.CanvasId,
            AuthorId = c.AuthorId,
            X = c.X,
            Y = c.Y,
            Content = c.Content,
            IsResolved = resolved,
            CreatedAt = c.CreatedAt,
            UpdatedAt = ctx.Timestamp
        });
    }

    [SpacetimeDB.Reducer]
    public static void DeleteComment(ReducerContext ctx, ulong commentId)
    {
        var comment = ctx.Db.comment.Id.Find(commentId);
        if (comment == null)
        {
            throw new Exception("Comment not found");
        }

        var c = comment.Value;
        if (c.AuthorId != ctx.Sender)
        {
            var canvas = ctx.Db.canvas.Id.Find(c.CanvasId);
            if (canvas == null || canvas.Value.CreatorId != ctx.Sender)
            {
                throw new Exception("Only the author or canvas owner can delete this comment");
            }
        }

        // Delete replies first
        foreach (var reply in ctx.Db.comment_reply.CommentId.Filter(commentId).ToList())
        {
            ctx.Db.comment_reply.Id.Delete(reply.Id);
        }

        ctx.Db.comment.Id.Delete(commentId);
    }

    // ========================================================================
    // VERSION HISTORY
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void SaveVersion(ReducerContext ctx, ulong canvasId, string name)
    {
        CheckCanEdit(ctx, canvasId);

        var snapshotJson = CreateCanvasSnapshot(ctx, canvasId);

        ctx.Db.canvas_version.Insert(new CanvasVersion
        {
            Id = 0,
            CanvasId = canvasId,
            CreatorId = ctx.Sender,
            Name = string.IsNullOrEmpty(name) ? $"Version {ctx.Timestamp.MicrosecondsSinceUnixEpoch}" : name,
            SnapshotJson = snapshotJson,
            IsAutoSave = false,
            CreatedAt = ctx.Timestamp
        });
    }

    [SpacetimeDB.Reducer]
    public static void RestoreVersion(ReducerContext ctx, ulong versionId)
    {
        var version = ctx.Db.canvas_version.Id.Find(versionId);
        if (version == null)
        {
            throw new Exception("Version not found");
        }

        var v = version.Value;
        CheckCanEdit(ctx, v.CanvasId);

        // Save current state as a version first
        var currentSnapshot = CreateCanvasSnapshot(ctx, v.CanvasId);
        ctx.Db.canvas_version.Insert(new CanvasVersion
        {
            Id = 0,
            CanvasId = v.CanvasId,
            CreatorId = ctx.Sender,
            Name = "Before restore",
            SnapshotJson = currentSnapshot,
            IsAutoSave = false,
            CreatedAt = ctx.Timestamp
        });

        // Note: Actual restoration from snapshot would require parsing JSON and recreating elements
        // This is simplified - in a real implementation you'd parse SnapshotJson and restore elements
        Log.Info($"Version {versionId} restoration requested. Snapshot available for client-side restore.");
    }

    [SpacetimeDB.Reducer]
    public static void ProcessAutoSave(ReducerContext ctx, AutoSaveJob job)
    {
        var canvas = ctx.Db.canvas.Id.Find(job.CanvasId);
        if (canvas == null)
        {
            return; // Canvas was deleted
        }

        var snapshotJson = CreateCanvasSnapshot(ctx, job.CanvasId);

        ctx.Db.canvas_version.Insert(new CanvasVersion
        {
            Id = 0,
            CanvasId = job.CanvasId,
            CreatorId = canvas.Value.CreatorId,
            Name = "Auto-save",
            SnapshotJson = snapshotJson,
            IsAutoSave = true,
            CreatedAt = ctx.Timestamp
        });

        // Schedule next auto-save
        ScheduleAutoSave(ctx, job.CanvasId, 300000);
    }

    // ========================================================================
    // TEMPLATES
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void SaveAsTemplate(ReducerContext ctx, ulong canvasId, string name, string description, string category, bool isPublic)
    {
        var canvas = ctx.Db.canvas.Id.Find(canvasId);
        if (canvas == null)
        {
            throw new Exception("Canvas not found");
        }

        if (canvas.Value.CreatorId != ctx.Sender)
        {
            throw new Exception("Only the creator can save this canvas as a template");
        }

        var snapshotJson = CreateCanvasSnapshot(ctx, canvasId);

        ctx.Db.template.Insert(new Template
        {
            Id = 0,
            CreatorId = ctx.Sender,
            Name = name.Trim(),
            Description = description?.Trim() ?? "",
            Category = category?.Trim() ?? "General",
            SnapshotJson = snapshotJson,
            IsPublic = isPublic,
            CreatedAt = ctx.Timestamp
        });
    }

    [SpacetimeDB.Reducer]
    public static void DeleteTemplate(ReducerContext ctx, ulong templateId)
    {
        var template = ctx.Db.template.Id.Find(templateId);
        if (template == null)
        {
            throw new Exception("Template not found");
        }

        if (template.Value.CreatorId != ctx.Sender)
        {
            throw new Exception("Only the creator can delete this template");
        }

        ctx.Db.template.Id.Delete(templateId);
    }

    // ========================================================================
    // VIEWPORT (for follow mode)
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void UpdateViewport(ReducerContext ctx, ulong canvasId, double panX, double panY, double zoom)
    {
        var existing = ctx.Db.viewport.CanvasId.Filter(canvasId)
            .FirstOrDefault(v => v.UserId == ctx.Sender);

        if (existing.Id != 0 || existing.CanvasId != 0)
        {
            ctx.Db.viewport.Id.Update(new Viewport
            {
                Id = existing.Id,
                UserId = ctx.Sender,
                CanvasId = canvasId,
                PanX = panX,
                PanY = panY,
                Zoom = zoom,
                LastUpdate = ctx.Timestamp
            });
        }
        else
        {
            ctx.Db.viewport.Insert(new Viewport
            {
                Id = 0,
                UserId = ctx.Sender,
                CanvasId = canvasId,
                PanX = panX,
                PanY = panY,
                Zoom = zoom,
                LastUpdate = ctx.Timestamp
            });
        }
    }

    // ========================================================================
    // HELPER METHODS
    // ========================================================================

    private static void CheckCanEdit(ReducerContext ctx, ulong canvasId)
    {
        var member = ctx.Db.canvas_member.CanvasId.Filter(canvasId)
            .FirstOrDefault(m => m.UserId == ctx.Sender);

        if (member.Id == 0 && member.CanvasId == 0)
        {
            throw new Exception("You are not a member of this canvas");
        }

        if (member.Role == "viewer")
        {
            throw new Exception("Viewers cannot edit the canvas");
        }
    }

    private static void CheckLayerNotLocked(ReducerContext ctx, ulong layerId)
    {
        var layer = ctx.Db.layer.Id.Find(layerId);
        if (layer != null && layer.Value.Locked)
        {
            throw new Exception("This layer is locked");
        }
    }

    private static void UpdateCanvasTimestamp(ReducerContext ctx, ulong canvasId)
    {
        var canvas = ctx.Db.canvas.Id.Find(canvasId);
        if (canvas != null)
        {
            var c = canvas.Value;
            ctx.Db.canvas.Id.Update(new Canvas
            {
                Id = c.Id,
                Name = c.Name,
                CreatorId = c.CreatorId,
                IsPrivate = c.IsPrivate,
                Width = c.Width,
                Height = c.Height,
                BackgroundColor = c.BackgroundColor,
                CreatedAt = c.CreatedAt,
                UpdatedAt = ctx.Timestamp
            });
        }
    }

    private static void AddUndoAction(ReducerContext ctx, ulong canvasId, string actionType, string elementType, ulong elementId, string previousStateJson)
    {
        var maxSeq = ctx.Db.undo_action.CanvasId.Filter(canvasId)
            .Where(a => a.UserId == ctx.Sender)
            .Select(a => a.SequenceNumber)
            .DefaultIfEmpty(0)
            .Max();

        ctx.Db.undo_action.Insert(new UndoAction
        {
            Id = 0,
            UserId = ctx.Sender,
            CanvasId = canvasId,
            ActionType = actionType,
            ElementType = elementType,
            ElementId = elementId,
            PreviousStateJson = previousStateJson,
            SequenceNumber = maxSeq + 1,
            IsUndone = false,
            CreatedAt = ctx.Timestamp
        });

        // Clear any undone actions (since we've made a new action)
        foreach (var undone in ctx.Db.undo_action.CanvasId.Filter(canvasId)
            .Where(a => a.UserId == ctx.Sender && a.IsUndone).ToList())
        {
            ctx.Db.undo_action.Id.Delete(undone.Id);
        }
    }

    private static void DeleteElementByType(ReducerContext ctx, string elementType, ulong elementId)
    {
        switch (elementType)
        {
            case "stroke":
                ctx.Db.stroke.Id.Delete(elementId);
                break;
            case "shape":
                ctx.Db.shape.Id.Delete(elementId);
                break;
            case "text":
                ctx.Db.text_element.Id.Delete(elementId);
                break;
            case "image":
                ctx.Db.image_element.Id.Delete(elementId);
                break;
            case "fill":
                ctx.Db.fill.Id.Delete(elementId);
                break;
        }
    }

    private static void ScheduleAutoSave(ReducerContext ctx, ulong canvasId, ulong delayMs)
    {
        var futureTime = ctx.Timestamp.MicrosecondsSinceUnixEpoch + (long)(delayMs * 1000);
        ctx.Db.autosave_job.Insert(new AutoSaveJob
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Time(new Timestamp(futureTime)),
            CanvasId = canvasId
        });
    }

    private static string CreateCanvasSnapshot(ReducerContext ctx, ulong canvasId)
    {
        // Simple snapshot - just metadata about element counts
        var strokeCount = ctx.Db.stroke.CanvasId.Filter(canvasId).Count();
        var shapeCount = ctx.Db.shape.CanvasId.Filter(canvasId).Count();
        var textCount = ctx.Db.text_element.CanvasId.Filter(canvasId).Count();
        var imageCount = ctx.Db.image_element.CanvasId.Filter(canvasId).Count();
        var layerCount = ctx.Db.layer.CanvasId.Filter(canvasId).Count();

        return $"{{\"strokes\":{strokeCount},\"shapes\":{shapeCount},\"texts\":{textCount},\"images\":{imageCount},\"layers\":{layerCount},\"timestamp\":{ctx.Timestamp.MicrosecondsSinceUnixEpoch}}}";
    }

    private static string EscapeJson(string input)
    {
        if (string.IsNullOrEmpty(input)) return "";
        return input.Replace("\\", "\\\\").Replace("\"", "\\\"").Replace("\n", "\\n").Replace("\r", "\\r").Replace("\t", "\\t");
    }
}
