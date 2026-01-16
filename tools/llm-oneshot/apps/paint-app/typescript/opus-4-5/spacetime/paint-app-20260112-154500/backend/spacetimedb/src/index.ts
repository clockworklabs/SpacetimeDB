import { spacetimedb, AutoSaveJob, LayerUnlockJob, CanvasCleanupJob, DeletionWarningJob } from './schema';
import { t, SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';

// ============================================================================
// LIFECYCLE HOOKS
// ============================================================================

spacetimedb.clientConnected((ctx) => {
  const existingUser = ctx.db.user.identity.find(ctx.sender);
  if (!existingUser) {
    ctx.db.user.insert({
      identity: ctx.sender,
      displayName: `User-${ctx.sender.toHexString().slice(0, 6)}`,
      avatarColor: getRandomColor(),
      createdAt: ctx.timestamp,
    });
  }
});

spacetimedb.clientDisconnected((ctx) => {
  // Clean up presence records
  for (const presence of ctx.db.canvasPresence.iter()) {
    if (presence.userIdentity.toHexString() === ctx.sender.toHexString()) {
      logActivity(ctx, presence.canvasId, ctx.sender, 'left');
      ctx.db.canvasPresence.id.delete(presence.id);
    }
  }

  // Clean up cursors
  for (const cursor of ctx.db.cursor.iter()) {
    if (cursor.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.cursor.id.delete(cursor.id);
    }
  }

  // Clean up selections
  for (const selection of ctx.db.selection.iter()) {
    if (selection.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.selection.id.delete(selection.id);
    }
  }

  // Clean up typing indicators
  for (const typing of ctx.db.typingIndicator.iter()) {
    if (typing.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.delete(typing.id);
    }
  }

  // Unlock layers locked by this user
  for (const layer of ctx.db.layer.iter()) {
    if (layer.lockedBy && layer.lockedBy.toHexString() === ctx.sender.toHexString()) {
      ctx.db.layer.id.update({ ...layer, lockedBy: undefined, lockedAt: undefined });
    }
  }

  // Clear text editing
  for (const textEl of ctx.db.textElement.iter()) {
    if (textEl.editingBy && textEl.editingBy.toHexString() === ctx.sender.toHexString()) {
      ctx.db.textElement.id.update({ ...textEl, editingBy: undefined });
    }
  }
});

// ============================================================================
// USER REDUCERS
// ============================================================================

spacetimedb.reducer('set_display_name', { displayName: t.string() }, (ctx, { displayName }) => {
  if (!displayName.trim()) throw new SenderError('Display name cannot be empty');
  if (displayName.length > 50) throw new SenderError('Display name too long');

  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) throw new SenderError('User not found');

  ctx.db.user.identity.update({ ...user, displayName: displayName.trim() });
});

spacetimedb.reducer('set_avatar_color', { color: t.string() }, (ctx, { color }) => {
  if (!color.match(/^#[0-9a-fA-F]{6}$/)) throw new SenderError('Invalid color format');

  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) throw new SenderError('User not found');

  ctx.db.user.identity.update({ ...user, avatarColor: color });
});

// ============================================================================
// CANVAS REDUCERS
// ============================================================================

spacetimedb.reducer('create_canvas', { name: t.string() }, (ctx, { name }) => {
  if (!name.trim()) throw new SenderError('Canvas name cannot be empty');

  const canvas = ctx.db.canvas.insert({
    id: 0n,
    ownerIdentity: ctx.sender,
    name: name.trim(),
    isPrivate: true,
    shareLinkToken: undefined,
    shareLinkPermission: undefined,
    keepForever: false,
    lastActivityAt: ctx.timestamp,
    createdAt: ctx.timestamp,
  });

  // Add owner as member
  ctx.db.canvasMember.insert({
    id: 0n,
    canvasId: canvas.id,
    userIdentity: ctx.sender,
    role: 'owner',
    invitedAt: ctx.timestamp,
  });

  // Create default layer
  ctx.db.layer.insert({
    id: 0n,
    canvasId: canvas.id,
    name: 'Layer 1',
    orderIndex: 0,
    visible: true,
    opacity: 1.0,
    lockedBy: undefined,
    lockedAt: undefined,
  });

  // Schedule cleanup check
  scheduleCleanupCheck(ctx, canvas.id);
});

spacetimedb.reducer('join_canvas', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  const canvas = ctx.db.canvas.id.find(canvasId);
  if (!canvas) throw new SenderError('Canvas not found');

  // Check if already a member
  let isMember = false;
  for (const member of ctx.db.canvasMember.canvas_member_canvas_id.filter(canvasId)) {
    if (member.userIdentity.toHexString() === ctx.sender.toHexString()) {
      isMember = true;
      break;
    }
  }

  if (!isMember && canvas.isPrivate) {
    throw new SenderError('Canvas is private');
  }

  // Create or update presence
  let presenceFound = false;
  for (const presence of ctx.db.canvasPresence.canvas_presence_canvas_id.filter(canvasId)) {
    if (presence.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.canvasPresence.id.update({
        ...presence,
        status: 'active',
        lastActivityAt: ctx.timestamp,
      });
      presenceFound = true;
      break;
    }
  }

  if (!presenceFound) {
    ctx.db.canvasPresence.insert({
      id: 0n,
      canvasId,
      userIdentity: ctx.sender,
      status: 'active',
      currentTool: 'brush',
      lastActivityAt: ctx.timestamp,
      viewportX: 0,
      viewportY: 0,
      viewportZoom: 1.0,
      followingUser: undefined,
    });

    logActivity(ctx, canvasId, ctx.sender, 'joined');
  }

  // Update canvas activity
  ctx.db.canvas.id.update({ ...canvas, lastActivityAt: ctx.timestamp });
});

spacetimedb.reducer('leave_canvas', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  // Remove presence
  for (const presence of ctx.db.canvasPresence.canvas_presence_canvas_id.filter(canvasId)) {
    if (presence.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.canvasPresence.id.delete(presence.id);
      break;
    }
  }

  // Remove cursor
  for (const cursor of ctx.db.cursor.cursor_canvas_id.filter(canvasId)) {
    if (cursor.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.cursor.id.delete(cursor.id);
      break;
    }
  }

  // Remove selections
  for (const selection of ctx.db.selection.selection_canvas_id.filter(canvasId)) {
    if (selection.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.selection.id.delete(selection.id);
    }
  }

  logActivity(ctx, canvasId, ctx.sender, 'left');
});

spacetimedb.reducer('join_canvas_via_link', { shareLinkToken: t.string() }, (ctx, { shareLinkToken }) => {
  let canvas = null;
  for (const c of ctx.db.canvas.iter()) {
    if (c.shareLinkToken === shareLinkToken) {
      canvas = c;
      break;
    }
  }

  if (!canvas) throw new SenderError('Invalid or expired share link');

  // Check if already a member
  let isMember = false;
  for (const member of ctx.db.canvasMember.canvas_member_canvas_id.filter(canvas.id)) {
    if (member.userIdentity.toHexString() === ctx.sender.toHexString()) {
      isMember = true;
      break;
    }
  }

  if (!isMember) {
    const role = canvas.shareLinkPermission === 'edit' ? 'editor' : 'viewer';
    ctx.db.canvasMember.insert({
      id: 0n,
      canvasId: canvas.id,
      userIdentity: ctx.sender,
      role,
      invitedAt: ctx.timestamp,
    });
  }
});

spacetimedb.reducer('generate_share_link', { canvasId: t.u64(), permission: t.string() }, (ctx, { canvasId, permission }) => {
  const canvas = ctx.db.canvas.id.find(canvasId);
  if (!canvas) throw new SenderError('Canvas not found');
  if (canvas.ownerIdentity.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Only the owner can generate share links');
  }

  const token = generateToken(ctx);
  ctx.db.canvas.id.update({
    ...canvas,
    shareLinkToken: token,
    shareLinkPermission: permission,
    isPrivate: false,
  });
});

spacetimedb.reducer('revoke_share_link', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  const canvas = ctx.db.canvas.id.find(canvasId);
  if (!canvas) throw new SenderError('Canvas not found');
  if (canvas.ownerIdentity.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Only the owner can revoke share links');
  }

  ctx.db.canvas.id.update({
    ...canvas,
    shareLinkToken: undefined,
    shareLinkPermission: undefined,
    isPrivate: true,
  });
});

spacetimedb.reducer('set_member_role', { canvasId: t.u64(), memberIdentity: t.identity(), role: t.string() }, (ctx, { canvasId, memberIdentity, role }) => {
  const canvas = ctx.db.canvas.id.find(canvasId);
  if (!canvas) throw new SenderError('Canvas not found');
  if (canvas.ownerIdentity.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Only the owner can change roles');
  }
  if (memberIdentity.toHexString() === ctx.sender.toHexString()) {
    throw new SenderError('Cannot change your own role');
  }

  for (const member of ctx.db.canvasMember.canvas_member_canvas_id.filter(canvasId)) {
    if (member.userIdentity.toHexString() === memberIdentity.toHexString()) {
      ctx.db.canvasMember.id.update({ ...member, role });
      return;
    }
  }
  throw new SenderError('Member not found');
});

spacetimedb.reducer('remove_member', { canvasId: t.u64(), memberIdentity: t.identity() }, (ctx, { canvasId, memberIdentity }) => {
  const canvas = ctx.db.canvas.id.find(canvasId);
  if (!canvas) throw new SenderError('Canvas not found');
  if (canvas.ownerIdentity.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Only the owner can remove members');
  }
  if (memberIdentity.toHexString() === ctx.sender.toHexString()) {
    throw new SenderError('Cannot remove yourself');
  }

  for (const member of ctx.db.canvasMember.canvas_member_canvas_id.filter(canvasId)) {
    if (member.userIdentity.toHexString() === memberIdentity.toHexString()) {
      ctx.db.canvasMember.id.delete(member.id);
      break;
    }
  }

  // Remove their presence and cursor
  for (const presence of ctx.db.canvasPresence.canvas_presence_canvas_id.filter(canvasId)) {
    if (presence.userIdentity.toHexString() === memberIdentity.toHexString()) {
      ctx.db.canvasPresence.id.delete(presence.id);
      break;
    }
  }
  for (const cursor of ctx.db.cursor.cursor_canvas_id.filter(canvasId)) {
    if (cursor.userIdentity.toHexString() === memberIdentity.toHexString()) {
      ctx.db.cursor.id.delete(cursor.id);
      break;
    }
  }
});

spacetimedb.reducer('set_keep_forever', { canvasId: t.u64(), keepForever: t.bool() }, (ctx, { canvasId, keepForever }) => {
  const canvas = ctx.db.canvas.id.find(canvasId);
  if (!canvas) throw new SenderError('Canvas not found');
  if (canvas.ownerIdentity.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Only the owner can change this setting');
  }

  ctx.db.canvas.id.update({ ...canvas, keepForever });
});

spacetimedb.reducer('delete_canvas', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  const canvas = ctx.db.canvas.id.find(canvasId);
  if (!canvas) throw new SenderError('Canvas not found');
  if (canvas.ownerIdentity.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Only the owner can delete the canvas');
  }

  deleteCanvasData(ctx, canvasId);
});

// ============================================================================
// LAYER REDUCERS
// ============================================================================

spacetimedb.reducer('create_layer', { canvasId: t.u64(), name: t.string() }, (ctx, { canvasId, name }) => {
  requireEditor(ctx, canvasId);

  let maxOrder = 0;
  for (const layer of ctx.db.layer.layer_canvas_id.filter(canvasId)) {
    if (layer.orderIndex > maxOrder) maxOrder = layer.orderIndex;
  }

  ctx.db.layer.insert({
    id: 0n,
    canvasId,
    name: name.trim() || 'New Layer',
    orderIndex: maxOrder + 1,
    visible: true,
    opacity: 1.0,
    lockedBy: undefined,
    lockedAt: undefined,
  });

  touchCanvas(ctx, canvasId);
});

spacetimedb.reducer('rename_layer', { layerId: t.u64(), name: t.string() }, (ctx, { layerId, name }) => {
  const layer = ctx.db.layer.id.find(layerId);
  if (!layer) throw new SenderError('Layer not found');
  requireEditor(ctx, layer.canvasId);

  ctx.db.layer.id.update({ ...layer, name: name.trim() });
  touchCanvas(ctx, layer.canvasId);
});

spacetimedb.reducer('reorder_layers', { canvasId: t.u64(), layerIds: t.string() }, (ctx, { canvasId, layerIds }) => {
  requireEditor(ctx, canvasId);

  const ids: bigint[] = JSON.parse(layerIds).map((id: string) => BigInt(id));
  ids.forEach((id, index) => {
    const layer = ctx.db.layer.id.find(id);
    if (layer && layer.canvasId === canvasId) {
      ctx.db.layer.id.update({ ...layer, orderIndex: index });
    }
  });
  touchCanvas(ctx, canvasId);
});

spacetimedb.reducer('toggle_layer_visibility', { layerId: t.u64() }, (ctx, { layerId }) => {
  const layer = ctx.db.layer.id.find(layerId);
  if (!layer) throw new SenderError('Layer not found');
  requireEditor(ctx, layer.canvasId);

  ctx.db.layer.id.update({ ...layer, visible: !layer.visible });
  touchCanvas(ctx, layer.canvasId);
});

spacetimedb.reducer('set_layer_opacity', { layerId: t.u64(), opacity: t.f64() }, (ctx, { layerId, opacity }) => {
  const layer = ctx.db.layer.id.find(layerId);
  if (!layer) throw new SenderError('Layer not found');
  requireEditor(ctx, layer.canvasId);

  ctx.db.layer.id.update({ ...layer, opacity: Math.max(0, Math.min(1, opacity)) });
  touchCanvas(ctx, layer.canvasId);
});

spacetimedb.reducer('lock_layer', { layerId: t.u64() }, (ctx, { layerId }) => {
  const layer = ctx.db.layer.id.find(layerId);
  if (!layer) throw new SenderError('Layer not found');
  requireEditor(ctx, layer.canvasId);

  if (layer.lockedBy && layer.lockedBy.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Layer is locked by another user');
  }

  ctx.db.layer.id.update({
    ...layer,
    lockedBy: ctx.sender,
    lockedAt: ctx.timestamp,
  });

  // Schedule auto-unlock after 5 minutes
  const unlockTime = ctx.timestamp.microsSinceUnixEpoch + 300_000_000n;
  ctx.db.layerUnlockJob.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(unlockTime),
    layerId,
  });
});

spacetimedb.reducer('unlock_layer', { layerId: t.u64() }, (ctx, { layerId }) => {
  const layer = ctx.db.layer.id.find(layerId);
  if (!layer) throw new SenderError('Layer not found');

  if (layer.lockedBy && layer.lockedBy.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Only the user who locked can unlock');
  }

  ctx.db.layer.id.update({
    ...layer,
    lockedBy: undefined,
    lockedAt: undefined,
  });
});

spacetimedb.reducer('delete_layer', { layerId: t.u64() }, (ctx, { layerId }) => {
  const layer = ctx.db.layer.id.find(layerId);
  if (!layer) throw new SenderError('Layer not found');
  requireEditor(ctx, layer.canvasId);

  // Delete all elements on this layer
  for (const stroke of ctx.db.stroke.stroke_layer_id.filter(layerId)) {
    ctx.db.stroke.id.delete(stroke.id);
  }
  for (const shape of ctx.db.shape.shape_layer_id.filter(layerId)) {
    ctx.db.shape.id.delete(shape.id);
  }
  for (const text of ctx.db.textElement.text_element_layer_id.filter(layerId)) {
    ctx.db.textElement.id.delete(text.id);
  }

  ctx.db.layer.id.delete(layerId);
  touchCanvas(ctx, layer.canvasId);
});

// ============================================================================
// STROKE REDUCERS
// ============================================================================

spacetimedb.reducer('add_stroke', {
  canvasId: t.u64(),
  layerId: t.u64(),
  tool: t.string(),
  color: t.string(),
  brushSize: t.u32(),
  points: t.string(),
}, (ctx, { canvasId, layerId, tool, color, brushSize, points }) => {
  requireEditor(ctx, canvasId);
  requireLayerEditable(ctx, layerId);

  const stroke = ctx.db.stroke.insert({
    id: 0n,
    canvasId,
    layerId,
    creatorIdentity: ctx.sender,
    tool,
    color,
    brushSize,
    points,
    createdAt: ctx.timestamp,
  });

  // Create undo entry
  ctx.db.undoEntry.insert({
    id: 0n,
    canvasId,
    userIdentity: ctx.sender,
    actionType: 'add_stroke',
    elementId: stroke.id,
    elementData: JSON.stringify({ layerId: layerId.toString(), tool, color, brushSize, points }),
    isUndone: false,
    createdAt: ctx.timestamp,
  });

  logActivity(ctx, canvasId, ctx.sender, tool === 'eraser' ? 'erased' : 'drew', undefined, undefined);
  touchCanvas(ctx, canvasId);
});

spacetimedb.reducer('delete_stroke', { strokeId: t.u64() }, (ctx, { strokeId }) => {
  const stroke = ctx.db.stroke.id.find(strokeId);
  if (!stroke) throw new SenderError('Stroke not found');
  requireEditor(ctx, stroke.canvasId);
  requireLayerEditable(ctx, stroke.layerId);

  ctx.db.stroke.id.delete(strokeId);
  touchCanvas(ctx, stroke.canvasId);
});

// ============================================================================
// SHAPE REDUCERS
// ============================================================================

spacetimedb.reducer('add_shape', {
  canvasId: t.u64(),
  layerId: t.u64(),
  shapeType: t.string(),
  x: t.f64(),
  y: t.f64(),
  width: t.f64(),
  height: t.f64(),
  strokeColor: t.string(),
  fillColor: t.string(),
  strokeWidth: t.u32(),
}, (ctx, args) => {
  requireEditor(ctx, args.canvasId);
  requireLayerEditable(ctx, args.layerId);

  const shape = ctx.db.shape.insert({
    id: 0n,
    canvasId: args.canvasId,
    layerId: args.layerId,
    creatorIdentity: ctx.sender,
    shapeType: args.shapeType,
    x: args.x,
    y: args.y,
    width: args.width,
    height: args.height,
    rotation: 0,
    strokeColor: args.strokeColor,
    fillColor: args.fillColor,
    strokeWidth: args.strokeWidth,
    createdAt: ctx.timestamp,
  });

  // Create undo entry
  ctx.db.undoEntry.insert({
    id: 0n,
    canvasId: args.canvasId,
    userIdentity: ctx.sender,
    actionType: 'add_shape',
    elementId: shape.id,
    elementData: JSON.stringify({ ...args, canvasId: args.canvasId.toString(), layerId: args.layerId.toString() }),
    isUndone: false,
    createdAt: ctx.timestamp,
  });

  logActivity(ctx, args.canvasId, ctx.sender, `added_${args.shapeType}`, undefined, args.x, args.y);
  touchCanvas(ctx, args.canvasId);
});

spacetimedb.reducer('update_shape', {
  shapeId: t.u64(),
  x: t.f64(),
  y: t.f64(),
  width: t.f64(),
  height: t.f64(),
  rotation: t.f64(),
}, (ctx, { shapeId, x, y, width, height, rotation }) => {
  const shape = ctx.db.shape.id.find(shapeId);
  if (!shape) throw new SenderError('Shape not found');
  requireEditor(ctx, shape.canvasId);
  requireLayerEditable(ctx, shape.layerId);

  ctx.db.shape.id.update({ ...shape, x, y, width, height, rotation });
  touchCanvas(ctx, shape.canvasId);
});

spacetimedb.reducer('delete_shape', { shapeId: t.u64() }, (ctx, { shapeId }) => {
  const shape = ctx.db.shape.id.find(shapeId);
  if (!shape) throw new SenderError('Shape not found');
  requireEditor(ctx, shape.canvasId);
  requireLayerEditable(ctx, shape.layerId);

  ctx.db.shape.id.delete(shapeId);
  removeSelectionForElement(ctx, shape.canvasId, 'shape', shapeId);
  touchCanvas(ctx, shape.canvasId);
});

// ============================================================================
// TEXT REDUCERS
// ============================================================================

spacetimedb.reducer('add_text_element', {
  canvasId: t.u64(),
  layerId: t.u64(),
  elementType: t.string(),
  x: t.f64(),
  y: t.f64(),
  width: t.f64(),
  height: t.f64(),
  content: t.string(),
  fontFamily: t.string(),
  fontSize: t.string(),
  textColor: t.string(),
  backgroundColor: t.string().optional(),
}, (ctx, args) => {
  requireEditor(ctx, args.canvasId);
  requireLayerEditable(ctx, args.layerId);

  const text = ctx.db.textElement.insert({
    id: 0n,
    canvasId: args.canvasId,
    layerId: args.layerId,
    creatorIdentity: ctx.sender,
    elementType: args.elementType,
    x: args.x,
    y: args.y,
    width: args.width,
    height: args.height,
    rotation: 0,
    content: args.content,
    fontFamily: args.fontFamily,
    fontSize: args.fontSize,
    textColor: args.textColor,
    backgroundColor: args.backgroundColor,
    editingBy: undefined,
    createdAt: ctx.timestamp,
  });

  // Create undo entry
  ctx.db.undoEntry.insert({
    id: 0n,
    canvasId: args.canvasId,
    userIdentity: ctx.sender,
    actionType: 'add_text',
    elementId: text.id,
    elementData: JSON.stringify({ ...args, canvasId: args.canvasId.toString(), layerId: args.layerId.toString() }),
    isUndone: false,
    createdAt: ctx.timestamp,
  });

  logActivity(ctx, args.canvasId, ctx.sender, `added_${args.elementType}`, undefined, args.x, args.y);
  touchCanvas(ctx, args.canvasId);
});

spacetimedb.reducer('update_text_element', {
  textId: t.u64(),
  x: t.f64(),
  y: t.f64(),
  width: t.f64(),
  height: t.f64(),
  rotation: t.f64(),
  content: t.string(),
}, (ctx, args) => {
  const text = ctx.db.textElement.id.find(args.textId);
  if (!text) throw new SenderError('Text element not found');
  requireEditor(ctx, text.canvasId);
  requireLayerEditable(ctx, text.layerId);

  ctx.db.textElement.id.update({
    ...text,
    x: args.x,
    y: args.y,
    width: args.width,
    height: args.height,
    rotation: args.rotation,
    content: args.content,
  });
  touchCanvas(ctx, text.canvasId);
});

spacetimedb.reducer('start_editing_text', { textId: t.u64() }, (ctx, { textId }) => {
  const text = ctx.db.textElement.id.find(textId);
  if (!text) throw new SenderError('Text element not found');
  requireEditor(ctx, text.canvasId);

  if (text.editingBy && text.editingBy.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Someone else is editing this');
  }

  ctx.db.textElement.id.update({ ...text, editingBy: ctx.sender });
});

spacetimedb.reducer('stop_editing_text', { textId: t.u64() }, (ctx, { textId }) => {
  const text = ctx.db.textElement.id.find(textId);
  if (!text) throw new SenderError('Text element not found');

  if (text.editingBy?.toHexString() === ctx.sender.toHexString()) {
    ctx.db.textElement.id.update({ ...text, editingBy: undefined });
  }
});

spacetimedb.reducer('delete_text_element', { textId: t.u64() }, (ctx, { textId }) => {
  const text = ctx.db.textElement.id.find(textId);
  if (!text) throw new SenderError('Text element not found');
  requireEditor(ctx, text.canvasId);
  requireLayerEditable(ctx, text.layerId);

  ctx.db.textElement.id.delete(textId);
  removeSelectionForElement(ctx, text.canvasId, 'text', textId);
  touchCanvas(ctx, text.canvasId);
});

// ============================================================================
// CURSOR & SELECTION REDUCERS
// ============================================================================

spacetimedb.reducer('update_cursor', {
  canvasId: t.u64(),
  x: t.f64(),
  y: t.f64(),
  tool: t.string(),
  color: t.string(),
}, (ctx, { canvasId, x, y, tool, color }) => {
  let found = false;
  for (const cursor of ctx.db.cursor.cursor_canvas_id.filter(canvasId)) {
    if (cursor.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.cursor.id.update({ ...cursor, x, y, tool, color, lastUpdatedAt: ctx.timestamp });
      found = true;
      break;
    }
  }

  if (!found) {
    ctx.db.cursor.insert({
      id: 0n,
      canvasId,
      userIdentity: ctx.sender,
      x,
      y,
      tool,
      color,
      lastUpdatedAt: ctx.timestamp,
    });
  }

  // Update presence activity
  for (const presence of ctx.db.canvasPresence.canvas_presence_canvas_id.filter(canvasId)) {
    if (presence.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.canvasPresence.id.update({
        ...presence,
        status: 'active',
        currentTool: tool,
        lastActivityAt: ctx.timestamp,
      });
      break;
    }
  }
});

spacetimedb.reducer('select_element', {
  canvasId: t.u64(),
  elementType: t.string(),
  elementId: t.u64(),
  addToSelection: t.bool(),
}, (ctx, { canvasId, elementType, elementId, addToSelection }) => {
  if (!addToSelection) {
    // Clear existing selections
    for (const sel of ctx.db.selection.selection_user_identity.filter(ctx.sender)) {
      if (sel.canvasId === canvasId) {
        ctx.db.selection.id.delete(sel.id);
      }
    }
  }

  // Check if already selected
  for (const sel of ctx.db.selection.selection_canvas_id.filter(canvasId)) {
    if (sel.userIdentity.toHexString() === ctx.sender.toHexString() &&
        sel.elementType === elementType && sel.elementId === elementId) {
      return; // Already selected
    }
  }

  ctx.db.selection.insert({
    id: 0n,
    canvasId,
    userIdentity: ctx.sender,
    elementType,
    elementId,
  });
});

spacetimedb.reducer('clear_selection', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  for (const sel of ctx.db.selection.selection_canvas_id.filter(canvasId)) {
    if (sel.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.selection.id.delete(sel.id);
    }
  }
});

spacetimedb.reducer('delete_selected', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  requireEditor(ctx, canvasId);

  const selectionsToDelete: { elementType: string; elementId: bigint }[] = [];
  for (const sel of ctx.db.selection.selection_canvas_id.filter(canvasId)) {
    if (sel.userIdentity.toHexString() === ctx.sender.toHexString()) {
      selectionsToDelete.push({ elementType: sel.elementType, elementId: sel.elementId });
    }
  }

  for (const sel of selectionsToDelete) {
    if (sel.elementType === 'stroke') {
      const stroke = ctx.db.stroke.id.find(sel.elementId);
      if (stroke) {
        requireLayerEditable(ctx, stroke.layerId);
        ctx.db.stroke.id.delete(sel.elementId);
      }
    } else if (sel.elementType === 'shape') {
      const shape = ctx.db.shape.id.find(sel.elementId);
      if (shape) {
        requireLayerEditable(ctx, shape.layerId);
        ctx.db.shape.id.delete(sel.elementId);
      }
    } else if (sel.elementType === 'text') {
      const text = ctx.db.textElement.id.find(sel.elementId);
      if (text) {
        requireLayerEditable(ctx, text.layerId);
        ctx.db.textElement.id.delete(sel.elementId);
      }
    }
    removeSelectionForElement(ctx, canvasId, sel.elementType, sel.elementId);
  }

  touchCanvas(ctx, canvasId);
});

// ============================================================================
// PRESENCE REDUCERS
// ============================================================================

spacetimedb.reducer('update_status', { canvasId: t.u64(), status: t.string() }, (ctx, { canvasId, status }) => {
  for (const presence of ctx.db.canvasPresence.canvas_presence_canvas_id.filter(canvasId)) {
    if (presence.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.canvasPresence.id.update({ ...presence, status, lastActivityAt: ctx.timestamp });
      break;
    }
  }
});

spacetimedb.reducer('update_viewport', {
  canvasId: t.u64(),
  x: t.f64(),
  y: t.f64(),
  zoom: t.f64(),
}, (ctx, { canvasId, x, y, zoom }) => {
  for (const presence of ctx.db.canvasPresence.canvas_presence_canvas_id.filter(canvasId)) {
    if (presence.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.canvasPresence.id.update({
        ...presence,
        viewportX: x,
        viewportY: y,
        viewportZoom: zoom,
        followingUser: undefined, // Stop following when manually panning
      });
      break;
    }
  }
});

spacetimedb.reducer('follow_user', { canvasId: t.u64(), targetIdentity: t.identity() }, (ctx, { canvasId, targetIdentity }) => {
  for (const presence of ctx.db.canvasPresence.canvas_presence_canvas_id.filter(canvasId)) {
    if (presence.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.canvasPresence.id.update({ ...presence, followingUser: targetIdentity });
      break;
    }
  }
});

spacetimedb.reducer('unfollow_user', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  for (const presence of ctx.db.canvasPresence.canvas_presence_canvas_id.filter(canvasId)) {
    if (presence.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.canvasPresence.id.update({ ...presence, followingUser: undefined });
      break;
    }
  }
});

// ============================================================================
// UNDO/REDO REDUCERS
// ============================================================================

spacetimedb.reducer('undo', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  requireEditor(ctx, canvasId);

  // Find the most recent non-undone action by this user
  let latestEntry: any = null;
  let latestTime = 0n;

  for (const entry of ctx.db.undoEntry.undo_entry_canvas_user.filter(canvasId)) {
    if (entry.userIdentity.toHexString() === ctx.sender.toHexString() && !entry.isUndone) {
      if (entry.createdAt.microsSinceUnixEpoch > latestTime) {
        latestTime = entry.createdAt.microsSinceUnixEpoch;
        latestEntry = entry;
      }
    }
  }

  if (!latestEntry) {
    return; // Nothing to undo
  }

  // Mark as undone
  ctx.db.undoEntry.id.update({ ...latestEntry, isUndone: true });

  // Reverse the action
  if (latestEntry.actionType === 'add_stroke') {
    ctx.db.stroke.id.delete(latestEntry.elementId);
  } else if (latestEntry.actionType === 'add_shape') {
    ctx.db.shape.id.delete(latestEntry.elementId);
  } else if (latestEntry.actionType === 'add_text') {
    ctx.db.textElement.id.delete(latestEntry.elementId);
  }

  touchCanvas(ctx, canvasId);
});

spacetimedb.reducer('redo', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  requireEditor(ctx, canvasId);

  // Find the most recent undone action by this user
  let latestEntry: any = null;
  let latestTime = 0n;

  for (const entry of ctx.db.undoEntry.undo_entry_canvas_user.filter(canvasId)) {
    if (entry.userIdentity.toHexString() === ctx.sender.toHexString() && entry.isUndone) {
      if (entry.createdAt.microsSinceUnixEpoch > latestTime) {
        latestTime = entry.createdAt.microsSinceUnixEpoch;
        latestEntry = entry;
      }
    }
  }

  if (!latestEntry) {
    return; // Nothing to redo
  }

  // Mark as not undone
  ctx.db.undoEntry.id.update({ ...latestEntry, isUndone: false });

  // Re-apply the action
  const data = JSON.parse(latestEntry.elementData);

  if (latestEntry.actionType === 'add_stroke') {
    ctx.db.stroke.insert({
      id: latestEntry.elementId,
      canvasId,
      layerId: BigInt(data.layerId),
      creatorIdentity: ctx.sender,
      tool: data.tool,
      color: data.color,
      brushSize: data.brushSize,
      points: data.points,
      createdAt: ctx.timestamp,
    });
  } else if (latestEntry.actionType === 'add_shape') {
    ctx.db.shape.insert({
      id: latestEntry.elementId,
      canvasId,
      layerId: BigInt(data.layerId),
      creatorIdentity: ctx.sender,
      shapeType: data.shapeType,
      x: data.x,
      y: data.y,
      width: data.width,
      height: data.height,
      rotation: 0,
      strokeColor: data.strokeColor,
      fillColor: data.fillColor,
      strokeWidth: data.strokeWidth,
      createdAt: ctx.timestamp,
    });
  } else if (latestEntry.actionType === 'add_text') {
    ctx.db.textElement.insert({
      id: latestEntry.elementId,
      canvasId,
      layerId: BigInt(data.layerId),
      creatorIdentity: ctx.sender,
      elementType: data.elementType,
      x: data.x,
      y: data.y,
      width: data.width,
      height: data.height,
      rotation: 0,
      content: data.content,
      fontFamily: data.fontFamily,
      fontSize: data.fontSize,
      textColor: data.textColor,
      backgroundColor: data.backgroundColor,
      editingBy: undefined,
      createdAt: ctx.timestamp,
    });
  }

  touchCanvas(ctx, canvasId);
});

// ============================================================================
// COMMENT REDUCERS
// ============================================================================

spacetimedb.reducer('add_comment', {
  canvasId: t.u64(),
  x: t.f64(),
  y: t.f64(),
  content: t.string(),
}, (ctx, { canvasId, x, y, content }) => {
  requireMember(ctx, canvasId);

  ctx.db.comment.insert({
    id: 0n,
    canvasId,
    authorIdentity: ctx.sender,
    x,
    y,
    content: content.trim(),
    resolved: false,
    createdAt: ctx.timestamp,
  });

  logActivity(ctx, canvasId, ctx.sender, 'added_comment', undefined, x, y);
  touchCanvas(ctx, canvasId);
});

spacetimedb.reducer('add_comment_reply', { commentId: t.u64(), content: t.string() }, (ctx, { commentId, content }) => {
  const comment = ctx.db.comment.id.find(commentId);
  if (!comment) throw new SenderError('Comment not found');
  requireMember(ctx, comment.canvasId);

  ctx.db.commentReply.insert({
    id: 0n,
    commentId,
    authorIdentity: ctx.sender,
    content: content.trim(),
    createdAt: ctx.timestamp,
  });

  touchCanvas(ctx, comment.canvasId);
});

spacetimedb.reducer('resolve_comment', { commentId: t.u64() }, (ctx, { commentId }) => {
  const comment = ctx.db.comment.id.find(commentId);
  if (!comment) throw new SenderError('Comment not found');
  requireMember(ctx, comment.canvasId);

  ctx.db.comment.id.update({ ...comment, resolved: true });
  touchCanvas(ctx, comment.canvasId);
});

spacetimedb.reducer('unresolve_comment', { commentId: t.u64() }, (ctx, { commentId }) => {
  const comment = ctx.db.comment.id.find(commentId);
  if (!comment) throw new SenderError('Comment not found');
  requireMember(ctx, comment.canvasId);

  ctx.db.comment.id.update({ ...comment, resolved: false });
});

spacetimedb.reducer('delete_comment', { commentId: t.u64() }, (ctx, { commentId }) => {
  const comment = ctx.db.comment.id.find(commentId);
  if (!comment) throw new SenderError('Comment not found');

  // Only author or owner can delete
  const canvas = ctx.db.canvas.id.find(comment.canvasId);
  const isAuthor = comment.authorIdentity.toHexString() === ctx.sender.toHexString();
  const isOwner = canvas?.ownerIdentity.toHexString() === ctx.sender.toHexString();
  if (!isAuthor && !isOwner) throw new SenderError('Not authorized to delete');

  // Delete replies
  for (const reply of ctx.db.commentReply.comment_reply_comment_id.filter(commentId)) {
    ctx.db.commentReply.id.delete(reply.id);
  }

  ctx.db.comment.id.delete(commentId);
  touchCanvas(ctx, comment.canvasId);
});

// ============================================================================
// VERSION REDUCERS
// ============================================================================

spacetimedb.reducer('save_version', {
  canvasId: t.u64(),
  name: t.string().optional(),
  description: t.string().optional(),
}, (ctx, { canvasId, name, description }) => {
  requireMember(ctx, canvasId);

  const snapshotData = createSnapshot(ctx, canvasId);
  ctx.db.version.insert({
    id: 0n,
    canvasId,
    name,
    description,
    snapshotData,
    isAutoSave: false,
    createdBy: ctx.sender,
    createdAt: ctx.timestamp,
  });

  logActivity(ctx, canvasId, ctx.sender, 'saved_version', name);
});

spacetimedb.reducer('restore_version', { versionId: t.u64() }, (ctx, { versionId }) => {
  const version = ctx.db.version.id.find(versionId);
  if (!version) throw new SenderError('Version not found');
  requireEditor(ctx, version.canvasId);

  // Save current state as a new version first
  const currentSnapshot = createSnapshot(ctx, version.canvasId);
  ctx.db.version.insert({
    id: 0n,
    canvasId: version.canvasId,
    name: `Before restore from ${version.name || 'unnamed'}`,
    description: undefined,
    snapshotData: currentSnapshot,
    isAutoSave: false,
    createdBy: ctx.sender,
    createdAt: ctx.timestamp,
  });

  // Delete current elements
  deleteCanvasElements(ctx, version.canvasId);

  // Restore from snapshot
  restoreSnapshot(ctx, version.canvasId, version.snapshotData);

  // Save new "Restored" version
  ctx.db.version.insert({
    id: 0n,
    canvasId: version.canvasId,
    name: `Restored from ${version.name || 'unnamed'}`,
    description: undefined,
    snapshotData: version.snapshotData,
    isAutoSave: false,
    createdBy: ctx.sender,
    createdAt: ctx.timestamp,
  });

  logActivity(ctx, version.canvasId, ctx.sender, 'restored_version', version.name);
  touchCanvas(ctx, version.canvasId);
});

// ============================================================================
// CHAT REDUCERS
// ============================================================================

spacetimedb.reducer('send_chat_message', { canvasId: t.u64(), content: t.string() }, (ctx, { canvasId, content }) => {
  requireMember(ctx, canvasId);

  ctx.db.chatMessage.insert({
    id: 0n,
    canvasId,
    authorIdentity: ctx.sender,
    content: content.trim(),
    createdAt: ctx.timestamp,
  });

  // Clear typing indicator
  for (const typing of ctx.db.typingIndicator.typing_indicator_canvas_id.filter(canvasId)) {
    if (typing.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.delete(typing.id);
      break;
    }
  }
});

spacetimedb.reducer('set_typing', { canvasId: t.u64(), isTyping: t.bool() }, (ctx, { canvasId, isTyping }) => {
  requireMember(ctx, canvasId);

  let found = false;
  for (const typing of ctx.db.typingIndicator.typing_indicator_canvas_id.filter(canvasId)) {
    if (typing.userIdentity.toHexString() === ctx.sender.toHexString()) {
      if (isTyping) {
        ctx.db.typingIndicator.id.update({ ...typing, startedAt: ctx.timestamp });
      } else {
        ctx.db.typingIndicator.id.delete(typing.id);
      }
      found = true;
      break;
    }
  }

  if (!found && isTyping) {
    ctx.db.typingIndicator.insert({
      id: 0n,
      canvasId,
      userIdentity: ctx.sender,
      startedAt: ctx.timestamp,
    });
  }
});

// ============================================================================
// NOTIFICATION REDUCERS
// ============================================================================

spacetimedb.reducer('mark_notification_read', { notificationId: t.u64() }, (ctx, { notificationId }) => {
  const notification = ctx.db.notification.id.find(notificationId);
  if (!notification) throw new SenderError('Notification not found');
  if (notification.userIdentity.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Not your notification');
  }

  ctx.db.notification.id.update({ ...notification, read: true });
});

spacetimedb.reducer('dismiss_notification', { notificationId: t.u64() }, (ctx, { notificationId }) => {
  const notification = ctx.db.notification.id.find(notificationId);
  if (!notification) throw new SenderError('Notification not found');
  if (notification.userIdentity.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Not your notification');
  }

  ctx.db.notification.id.delete(notificationId);
});

// ============================================================================
// CANVAS CLEAR
// ============================================================================

spacetimedb.reducer('clear_canvas', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  requireEditor(ctx, canvasId);

  // Save version before clearing
  const snapshotData = createSnapshot(ctx, canvasId);
  ctx.db.version.insert({
    id: 0n,
    canvasId,
    name: 'Before clear',
    description: undefined,
    snapshotData,
    isAutoSave: true,
    createdBy: ctx.sender,
    createdAt: ctx.timestamp,
  });

  deleteCanvasElements(ctx, canvasId);
  logActivity(ctx, canvasId, ctx.sender, 'cleared_canvas');
  touchCanvas(ctx, canvasId);
});

// ============================================================================
// SCHEDULED REDUCERS
// ============================================================================

spacetimedb.reducer('run_auto_save', { arg: AutoSaveJob.rowType }, (ctx, { arg }) => {
  const canvas = ctx.db.canvas.id.find(arg.canvasId);
  if (!canvas) return;

  // Check if canvas has any elements
  let hasElements = false;
  for (const _ of ctx.db.stroke.stroke_canvas_id.filter(arg.canvasId)) {
    hasElements = true;
    break;
  }
  if (!hasElements) {
    for (const _ of ctx.db.shape.shape_canvas_id.filter(arg.canvasId)) {
      hasElements = true;
      break;
    }
  }

  if (hasElements) {
    const snapshotData = createSnapshot(ctx, arg.canvasId);
    ctx.db.version.insert({
      id: 0n,
      canvasId: arg.canvasId,
      name: undefined,
      description: undefined,
      snapshotData,
      isAutoSave: true,
      createdBy: undefined,
      createdAt: ctx.timestamp,
    });
  }

  // Schedule next auto-save (5 minutes)
  scheduleAutoSave(ctx, arg.canvasId);
});

spacetimedb.reducer('run_layer_unlock', { arg: LayerUnlockJob.rowType }, (ctx, { arg }) => {
  const layer = ctx.db.layer.id.find(arg.layerId);
  if (layer && layer.lockedBy) {
    ctx.db.layer.id.update({ ...layer, lockedBy: undefined, lockedAt: undefined });
  }
});

spacetimedb.reducer('run_canvas_cleanup', { arg: CanvasCleanupJob.rowType }, (ctx, { arg }) => {
  const canvas = ctx.db.canvas.id.find(arg.canvasId);
  if (!canvas) return;

  if (canvas.keepForever) return;

  const thirtyDaysAgo = ctx.timestamp.microsSinceUnixEpoch - 30n * 24n * 60n * 60n * 1_000_000n;
  if (canvas.lastActivityAt.microsSinceUnixEpoch < thirtyDaysAgo) {
    deleteCanvasData(ctx, arg.canvasId);
  }
});

spacetimedb.reducer('run_deletion_warning', { arg: DeletionWarningJob.rowType }, (ctx, { arg }) => {
  const canvas = ctx.db.canvas.id.find(arg.canvasId);
  if (!canvas) return;

  if (canvas.keepForever) return;

  const twentyThreeDaysAgo = ctx.timestamp.microsSinceUnixEpoch - 23n * 24n * 60n * 60n * 1_000_000n;
  if (canvas.lastActivityAt.microsSinceUnixEpoch < twentyThreeDaysAgo) {
    // Send warning to all members
    for (const member of ctx.db.canvasMember.canvas_member_canvas_id.filter(arg.canvasId)) {
      ctx.db.notification.insert({
        id: 0n,
        userIdentity: member.userIdentity,
        notificationType: 'deletion_warning',
        title: 'Canvas will be deleted',
        message: `"${canvas.name}" will be deleted in 7 days due to inactivity.`,
        canvasId: arg.canvasId,
        read: false,
        createdAt: ctx.timestamp,
      });
    }
  }
});

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

function getRandomColor(): string {
  const colors = ['#4cf490', '#a880ff', '#02befa', '#fbdc8e', '#ff80fb', '#00ccb4', '#ff9e9e'];
  const index = Math.floor(Math.random() * colors.length);
  return colors[index];
}

function generateToken(ctx: any): string {
  const timestamp = ctx.timestamp.microsSinceUnixEpoch.toString(16);
  const random = Math.random().toString(16).slice(2, 10);
  return `${timestamp}-${random}`;
}

function requireMember(ctx: any, canvasId: bigint): void {
  let isMember = false;
  for (const member of ctx.db.canvasMember.canvas_member_canvas_id.filter(canvasId)) {
    if (member.userIdentity.toHexString() === ctx.sender.toHexString()) {
      isMember = true;
      break;
    }
  }
  if (!isMember) throw new SenderError('Not a member of this canvas');
}

function requireEditor(ctx: any, canvasId: bigint): void {
  let role = '';
  for (const member of ctx.db.canvasMember.canvas_member_canvas_id.filter(canvasId)) {
    if (member.userIdentity.toHexString() === ctx.sender.toHexString()) {
      role = member.role;
      break;
    }
  }
  if (role !== 'owner' && role !== 'editor') {
    throw new SenderError('View-only access');
  }
}

function requireLayerEditable(ctx: any, layerId: bigint): void {
  const layer = ctx.db.layer.id.find(layerId);
  if (!layer) throw new SenderError('Layer not found');
  if (layer.lockedBy && layer.lockedBy.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Layer is locked by another user');
  }
}

function touchCanvas(ctx: any, canvasId: bigint): void {
  const canvas = ctx.db.canvas.id.find(canvasId);
  if (canvas) {
    ctx.db.canvas.id.update({ ...canvas, lastActivityAt: ctx.timestamp });
  }
}

function logActivity(ctx: any, canvasId: bigint, userIdentity: any, action: string, details?: string, x?: number, y?: number): void {
  ctx.db.activityEntry.insert({
    id: 0n,
    canvasId,
    userIdentity,
    action,
    details,
    locationX: x,
    locationY: y,
    createdAt: ctx.timestamp,
  });

  // Trim to last 100 entries
  const entries: any[] = [];
  for (const entry of ctx.db.activityEntry.activity_entry_canvas_id.filter(canvasId)) {
    entries.push(entry);
  }
  entries.sort((a, b) => Number(b.createdAt.microsSinceUnixEpoch - a.createdAt.microsSinceUnixEpoch));
  for (let i = 100; i < entries.length; i++) {
    ctx.db.activityEntry.id.delete(entries[i].id);
  }
}

function removeSelectionForElement(ctx: any, canvasId: bigint, elementType: string, elementId: bigint): void {
  for (const sel of ctx.db.selection.selection_canvas_id.filter(canvasId)) {
    if (sel.elementType === elementType && sel.elementId === elementId) {
      ctx.db.selection.id.delete(sel.id);
    }
  }
}

function createSnapshot(ctx: any, canvasId: bigint): string {
  const strokes: any[] = [];
  const shapes: any[] = [];
  const textElements: any[] = [];
  const layers: any[] = [];

  for (const stroke of ctx.db.stroke.stroke_canvas_id.filter(canvasId)) {
    strokes.push({
      layerId: stroke.layerId.toString(),
      tool: stroke.tool,
      color: stroke.color,
      brushSize: stroke.brushSize,
      points: stroke.points,
    });
  }

  for (const shape of ctx.db.shape.shape_canvas_id.filter(canvasId)) {
    shapes.push({
      layerId: shape.layerId.toString(),
      shapeType: shape.shapeType,
      x: shape.x,
      y: shape.y,
      width: shape.width,
      height: shape.height,
      rotation: shape.rotation,
      strokeColor: shape.strokeColor,
      fillColor: shape.fillColor,
      strokeWidth: shape.strokeWidth,
    });
  }

  for (const text of ctx.db.textElement.text_element_canvas_id.filter(canvasId)) {
    textElements.push({
      layerId: text.layerId.toString(),
      elementType: text.elementType,
      x: text.x,
      y: text.y,
      width: text.width,
      height: text.height,
      rotation: text.rotation,
      content: text.content,
      fontFamily: text.fontFamily,
      fontSize: text.fontSize,
      textColor: text.textColor,
      backgroundColor: text.backgroundColor,
    });
  }

  for (const layer of ctx.db.layer.layer_canvas_id.filter(canvasId)) {
    layers.push({
      id: layer.id.toString(),
      name: layer.name,
      orderIndex: layer.orderIndex,
      visible: layer.visible,
      opacity: layer.opacity,
    });
  }

  return JSON.stringify({ layers, strokes, shapes, textElements });
}

function deleteCanvasElements(ctx: any, canvasId: bigint): void {
  for (const stroke of ctx.db.stroke.stroke_canvas_id.filter(canvasId)) {
    ctx.db.stroke.id.delete(stroke.id);
  }
  for (const shape of ctx.db.shape.shape_canvas_id.filter(canvasId)) {
    ctx.db.shape.id.delete(shape.id);
  }
  for (const text of ctx.db.textElement.text_element_canvas_id.filter(canvasId)) {
    ctx.db.textElement.id.delete(text.id);
  }
}

function restoreSnapshot(ctx: any, canvasId: bigint, snapshotData: string): void {
  const data = JSON.parse(snapshotData);

  // Create layer ID mapping
  const layerIdMap = new Map<string, bigint>();
  if (data.layers) {
    for (const layerData of data.layers) {
      const layer = ctx.db.layer.insert({
        id: 0n,
        canvasId,
        name: layerData.name,
        orderIndex: layerData.orderIndex,
        visible: layerData.visible,
        opacity: layerData.opacity,
        lockedBy: undefined,
        lockedAt: undefined,
      });
      layerIdMap.set(layerData.id, layer.id);
    }
  }

  // Delete old layers
  for (const layer of ctx.db.layer.layer_canvas_id.filter(canvasId)) {
    if (!layerIdMap.has(layer.id.toString())) {
      ctx.db.layer.id.delete(layer.id);
    }
  }

  const getLayerId = (oldId: string): bigint => {
    return layerIdMap.get(oldId) || [...layerIdMap.values()][0] || 0n;
  };

  if (data.strokes) {
    for (const stroke of data.strokes) {
      ctx.db.stroke.insert({
        id: 0n,
        canvasId,
        layerId: getLayerId(stroke.layerId),
        creatorIdentity: ctx.sender,
        tool: stroke.tool,
        color: stroke.color,
        brushSize: stroke.brushSize,
        points: stroke.points,
        createdAt: ctx.timestamp,
      });
    }
  }

  if (data.shapes) {
    for (const shape of data.shapes) {
      ctx.db.shape.insert({
        id: 0n,
        canvasId,
        layerId: getLayerId(shape.layerId),
        creatorIdentity: ctx.sender,
        shapeType: shape.shapeType,
        x: shape.x,
        y: shape.y,
        width: shape.width,
        height: shape.height,
        rotation: shape.rotation,
        strokeColor: shape.strokeColor,
        fillColor: shape.fillColor,
        strokeWidth: shape.strokeWidth,
        createdAt: ctx.timestamp,
      });
    }
  }

  if (data.textElements) {
    for (const text of data.textElements) {
      ctx.db.textElement.insert({
        id: 0n,
        canvasId,
        layerId: getLayerId(text.layerId),
        creatorIdentity: ctx.sender,
        elementType: text.elementType,
        x: text.x,
        y: text.y,
        width: text.width,
        height: text.height,
        rotation: text.rotation,
        content: text.content,
        fontFamily: text.fontFamily,
        fontSize: text.fontSize,
        textColor: text.textColor,
        backgroundColor: text.backgroundColor,
        editingBy: undefined,
        createdAt: ctx.timestamp,
      });
    }
  }
}

function deleteCanvasData(ctx: any, canvasId: bigint): void {
  // Delete all related data
  deleteCanvasElements(ctx, canvasId);

  for (const layer of ctx.db.layer.layer_canvas_id.filter(canvasId)) {
    ctx.db.layer.id.delete(layer.id);
  }
  for (const presence of ctx.db.canvasPresence.canvas_presence_canvas_id.filter(canvasId)) {
    ctx.db.canvasPresence.id.delete(presence.id);
  }
  for (const cursor of ctx.db.cursor.cursor_canvas_id.filter(canvasId)) {
    ctx.db.cursor.id.delete(cursor.id);
  }
  for (const selection of ctx.db.selection.selection_canvas_id.filter(canvasId)) {
    ctx.db.selection.id.delete(selection.id);
  }
  for (const comment of ctx.db.comment.comment_canvas_id.filter(canvasId)) {
    for (const reply of ctx.db.commentReply.comment_reply_comment_id.filter(comment.id)) {
      ctx.db.commentReply.id.delete(reply.id);
    }
    ctx.db.comment.id.delete(comment.id);
  }
  for (const version of ctx.db.version.version_canvas_id.filter(canvasId)) {
    ctx.db.version.id.delete(version.id);
  }
  for (const chat of ctx.db.chatMessage.chat_message_canvas_id.filter(canvasId)) {
    ctx.db.chatMessage.id.delete(chat.id);
  }
  for (const typing of ctx.db.typingIndicator.typing_indicator_canvas_id.filter(canvasId)) {
    ctx.db.typingIndicator.id.delete(typing.id);
  }
  for (const activity of ctx.db.activityEntry.activity_entry_canvas_id.filter(canvasId)) {
    ctx.db.activityEntry.id.delete(activity.id);
  }
  for (const member of ctx.db.canvasMember.canvas_member_canvas_id.filter(canvasId)) {
    ctx.db.canvasMember.id.delete(member.id);
  }

  ctx.db.canvas.id.delete(canvasId);
}

function scheduleAutoSave(ctx: any, canvasId: bigint): void {
  const fiveMinutes = ctx.timestamp.microsSinceUnixEpoch + 5n * 60n * 1_000_000n;
  ctx.db.autoSaveJob.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(fiveMinutes),
    canvasId,
  });
}

function scheduleCleanupCheck(ctx: any, canvasId: bigint): void {
  // Schedule warning at 23 days
  const twentyThreeDays = ctx.timestamp.microsSinceUnixEpoch + 23n * 24n * 60n * 60n * 1_000_000n;
  ctx.db.deletionWarningJob.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(twentyThreeDays),
    canvasId,
  });

  // Schedule cleanup at 30 days
  const thirtyDays = ctx.timestamp.microsSinceUnixEpoch + 30n * 24n * 60n * 60n * 1_000_000n;
  ctx.db.canvasCleanupJob.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(thirtyDays),
    canvasId,
  });

  // Schedule first auto-save
  scheduleAutoSave(ctx, canvasId);
}
