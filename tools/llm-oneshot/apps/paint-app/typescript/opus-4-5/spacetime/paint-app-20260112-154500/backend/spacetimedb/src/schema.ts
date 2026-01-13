import { schema, table, t } from 'spacetimedb/server';

// ============================================================================
// USER & PRESENCE
// ============================================================================

export const User = table({
  name: 'user',
  public: true,
}, {
  identity: t.identity().primaryKey(),
  displayName: t.string(),
  avatarColor: t.string(),
  createdAt: t.timestamp(),
});

// Per-canvas presence info
export const CanvasPresence = table({
  name: 'canvas_presence',
  public: true,
  indexes: [
    { name: 'canvas_presence_canvas_id', algorithm: 'btree', columns: ['canvasId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  canvasId: t.u64(),
  userIdentity: t.identity(),
  status: t.string(), // 'active' | 'idle' | 'away'
  currentTool: t.string(),
  lastActivityAt: t.timestamp(),
  viewportX: t.f64(),
  viewportY: t.f64(),
  viewportZoom: t.f64(),
  followingUser: t.identity().optional(), // Who they're following
});

// ============================================================================
// CANVAS & MEMBERSHIP
// ============================================================================

export const Canvas = table({
  name: 'canvas',
  public: true,
}, {
  id: t.u64().primaryKey().autoInc(),
  ownerIdentity: t.identity(),
  name: t.string(),
  isPrivate: t.bool(),
  shareLinkToken: t.string().optional(),
  shareLinkPermission: t.string().optional(), // 'view' | 'edit'
  keepForever: t.bool(),
  lastActivityAt: t.timestamp(),
  createdAt: t.timestamp(),
});

export const CanvasMember = table({
  name: 'canvas_member',
  public: true,
  indexes: [
    { name: 'canvas_member_canvas_id', algorithm: 'btree', columns: ['canvasId'] },
    { name: 'canvas_member_user_identity', algorithm: 'btree', columns: ['userIdentity'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  canvasId: t.u64(),
  userIdentity: t.identity(),
  role: t.string(), // 'owner' | 'editor' | 'viewer'
  invitedAt: t.timestamp(),
});

// ============================================================================
// LAYERS
// ============================================================================

export const Layer = table({
  name: 'layer',
  public: true,
  indexes: [
    { name: 'layer_canvas_id', algorithm: 'btree', columns: ['canvasId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  canvasId: t.u64(),
  name: t.string(),
  orderIndex: t.u32(),
  visible: t.bool(),
  opacity: t.f64(),
  lockedBy: t.identity().optional(),
  lockedAt: t.timestamp().optional(),
});

// ============================================================================
// STROKES (Freehand Drawing)
// ============================================================================

export const Stroke = table({
  name: 'stroke',
  public: true,
  indexes: [
    { name: 'stroke_canvas_id', algorithm: 'btree', columns: ['canvasId'] },
    { name: 'stroke_layer_id', algorithm: 'btree', columns: ['layerId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  canvasId: t.u64(),
  layerId: t.u64(),
  creatorIdentity: t.identity(),
  tool: t.string(), // 'brush' | 'eraser'
  color: t.string(),
  brushSize: t.u32(),
  points: t.string(), // JSON array of {x, y} points
  createdAt: t.timestamp(),
});

// ============================================================================
// SHAPES
// ============================================================================

export const Shape = table({
  name: 'shape',
  public: true,
  indexes: [
    { name: 'shape_canvas_id', algorithm: 'btree', columns: ['canvasId'] },
    { name: 'shape_layer_id', algorithm: 'btree', columns: ['layerId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  canvasId: t.u64(),
  layerId: t.u64(),
  creatorIdentity: t.identity(),
  shapeType: t.string(), // 'rectangle' | 'ellipse' | 'line' | 'arrow'
  x: t.f64(),
  y: t.f64(),
  width: t.f64(),
  height: t.f64(),
  rotation: t.f64(),
  strokeColor: t.string(),
  fillColor: t.string(),
  strokeWidth: t.u32(),
  createdAt: t.timestamp(),
});

// ============================================================================
// TEXT & STICKY NOTES
// ============================================================================

export const TextElement = table({
  name: 'text_element',
  public: true,
  indexes: [
    { name: 'text_element_canvas_id', algorithm: 'btree', columns: ['canvasId'] },
    { name: 'text_element_layer_id', algorithm: 'btree', columns: ['layerId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  canvasId: t.u64(),
  layerId: t.u64(),
  creatorIdentity: t.identity(),
  elementType: t.string(), // 'text' | 'sticky'
  x: t.f64(),
  y: t.f64(),
  width: t.f64(),
  height: t.f64(),
  rotation: t.f64(),
  content: t.string(),
  fontFamily: t.string(), // 'sans-serif' | 'serif' | 'monospace'
  fontSize: t.string(), // 'small' | 'medium' | 'large' | 'x-large'
  textColor: t.string(),
  backgroundColor: t.string().optional(), // For sticky notes
  editingBy: t.identity().optional(),
  createdAt: t.timestamp(),
});

// ============================================================================
// CURSORS
// ============================================================================

export const Cursor = table({
  name: 'cursor',
  public: true,
  indexes: [
    { name: 'cursor_canvas_id', algorithm: 'btree', columns: ['canvasId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  canvasId: t.u64(),
  userIdentity: t.identity(),
  x: t.f64(),
  y: t.f64(),
  tool: t.string(),
  color: t.string(),
  lastUpdatedAt: t.timestamp(),
});

// ============================================================================
// SELECTIONS
// ============================================================================

export const Selection = table({
  name: 'selection',
  public: true,
  indexes: [
    { name: 'selection_canvas_id', algorithm: 'btree', columns: ['canvasId'] },
    { name: 'selection_user_identity', algorithm: 'btree', columns: ['userIdentity'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  canvasId: t.u64(),
  userIdentity: t.identity(),
  elementType: t.string(), // 'stroke' | 'shape' | 'text'
  elementId: t.u64(),
});

// ============================================================================
// COMMENTS
// ============================================================================

export const Comment = table({
  name: 'comment',
  public: true,
  indexes: [
    { name: 'comment_canvas_id', algorithm: 'btree', columns: ['canvasId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  canvasId: t.u64(),
  authorIdentity: t.identity(),
  x: t.f64(),
  y: t.f64(),
  content: t.string(),
  resolved: t.bool(),
  createdAt: t.timestamp(),
});

export const CommentReply = table({
  name: 'comment_reply',
  public: true,
  indexes: [
    { name: 'comment_reply_comment_id', algorithm: 'btree', columns: ['commentId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  commentId: t.u64(),
  authorIdentity: t.identity(),
  content: t.string(),
  createdAt: t.timestamp(),
});

// ============================================================================
// VERSIONS (Snapshots)
// ============================================================================

export const Version = table({
  name: 'version',
  public: true,
  indexes: [
    { name: 'version_canvas_id', algorithm: 'btree', columns: ['canvasId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  canvasId: t.u64(),
  name: t.string().optional(),
  description: t.string().optional(),
  snapshotData: t.string(), // JSON snapshot of all elements
  isAutoSave: t.bool(),
  createdBy: t.identity().optional(),
  createdAt: t.timestamp(),
});

// ============================================================================
// CHAT
// ============================================================================

export const ChatMessage = table({
  name: 'chat_message',
  public: true,
  indexes: [
    { name: 'chat_message_canvas_id', algorithm: 'btree', columns: ['canvasId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  canvasId: t.u64(),
  authorIdentity: t.identity(),
  content: t.string(),
  createdAt: t.timestamp(),
});

export const TypingIndicator = table({
  name: 'typing_indicator',
  public: true,
  indexes: [
    { name: 'typing_indicator_canvas_id', algorithm: 'btree', columns: ['canvasId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  canvasId: t.u64(),
  userIdentity: t.identity(),
  startedAt: t.timestamp(),
});

// ============================================================================
// ACTIVITY FEED
// ============================================================================

export const ActivityEntry = table({
  name: 'activity_entry',
  public: true,
  indexes: [
    { name: 'activity_entry_canvas_id', algorithm: 'btree', columns: ['canvasId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  canvasId: t.u64(),
  userIdentity: t.identity(),
  action: t.string(), // 'joined' | 'left' | 'added_stroke' | 'added_shape' | etc.
  details: t.string().optional(), // JSON with extra info
  locationX: t.f64().optional(),
  locationY: t.f64().optional(),
  createdAt: t.timestamp(),
});

// ============================================================================
// NOTIFICATIONS
// ============================================================================

export const Notification = table({
  name: 'notification',
  public: true,
  indexes: [
    { name: 'notification_user_identity', algorithm: 'btree', columns: ['userIdentity'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  userIdentity: t.identity(),
  notificationType: t.string(), // 'deletion_warning' | 'activity' | etc.
  title: t.string(),
  message: t.string(),
  canvasId: t.u64().optional(),
  read: t.bool(),
  createdAt: t.timestamp(),
});

// ============================================================================
// SCHEDULED JOBS
// ============================================================================

export const AutoSaveJob = table({
  name: 'auto_save_job',
  scheduled: 'run_auto_save',
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  canvasId: t.u64(),
});

export const LayerUnlockJob = table({
  name: 'layer_unlock_job',
  scheduled: 'run_layer_unlock',
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  layerId: t.u64(),
});

export const CanvasCleanupJob = table({
  name: 'canvas_cleanup_job',
  scheduled: 'run_canvas_cleanup',
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  canvasId: t.u64(),
});

export const DeletionWarningJob = table({
  name: 'deletion_warning_job',
  scheduled: 'run_deletion_warning',
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  canvasId: t.u64(),
});

// ============================================================================
// UNDO/REDO HISTORY
// ============================================================================

export const UndoEntry = table({
  name: 'undo_entry',
  public: true,
  indexes: [
    { name: 'undo_entry_canvas_user', algorithm: 'btree', columns: ['canvasId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  canvasId: t.u64(),
  userIdentity: t.identity(),
  actionType: t.string(), // 'add_stroke' | 'add_shape' | 'add_text' | 'delete_stroke' | 'delete_shape' | 'delete_text'
  elementId: t.u64(),
  elementData: t.string(), // JSON snapshot of the element for restore
  isUndone: t.bool(),
  createdAt: t.timestamp(),
});

// Export schema
export const spacetimedb = schema(
  User,
  CanvasPresence,
  Canvas,
  CanvasMember,
  Layer,
  Stroke,
  Shape,
  TextElement,
  Cursor,
  Selection,
  Comment,
  CommentReply,
  Version,
  ChatMessage,
  TypingIndicator,
  ActivityEntry,
  Notification,
  UndoEntry,
  AutoSaveJob,
  LayerUnlockJob,
  CanvasCleanupJob,
  DeletionWarningJob,
);
