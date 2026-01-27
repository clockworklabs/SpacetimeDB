import { schema, table, t } from 'spacetimedb/server';

// User table - stores display names
export const User = table(
  {
    name: 'user',
    public: true,
  },
  {
    identity: t.identity().primaryKey(),
    displayName: t.string(),
    isOnline: t.bool(),
  }
);

// Canvas table - collaborative drawing spaces
export const Canvas = table(
  {
    name: 'canvas',
    public: true,
    indexes: [
      { name: 'canvas_ownerId', algorithm: 'btree', columns: ['ownerId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    ownerId: t.identity(),
    createdAt: t.timestamp(),
  }
);

// Canvas membership - who's on which canvas
export const CanvasMember = table(
  {
    name: 'canvas_member',
    public: true,
    indexes: [
      {
        name: 'canvas_member_canvasId',
        algorithm: 'btree',
        columns: ['canvasId'],
      },
      { name: 'canvas_member_userId', algorithm: 'btree', columns: ['userId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    canvasId: t.u64(),
    userId: t.identity(),
    currentTool: t.string(),
    brushColor: t.string(),
    brushSize: t.u64(),
    isActive: t.bool(),
    lastActivity: t.timestamp(),
  }
);

// Draw elements - strokes, shapes, etc. on a canvas
export const DrawElement = table(
  {
    name: 'draw_element',
    public: true,
    indexes: [
      {
        name: 'draw_element_canvasId',
        algorithm: 'btree',
        columns: ['canvasId'],
      },
      {
        name: 'draw_element_ownerId',
        algorithm: 'btree',
        columns: ['ownerId'],
      },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    canvasId: t.u64(),
    ownerId: t.identity(),
    elementType: t.string(), // 'stroke', 'rect', 'ellipse', 'line', 'fill'
    data: t.string(), // JSON with element-specific data
    isDeleted: t.bool(),
    createdAt: t.timestamp(),
    zIndex: t.u64(),
  }
);

// Cursor positions for live presence
export const CursorPosition = table(
  {
    name: 'cursor_position',
    public: true,
    indexes: [
      {
        name: 'cursor_position_canvasId',
        algorithm: 'btree',
        columns: ['canvasId'],
      },
    ],
  },
  {
    userId: t.identity().primaryKey(),
    canvasId: t.u64(),
    x: t.u64(),
    y: t.u64(),
    tool: t.string(),
    color: t.string(),
    lastUpdate: t.timestamp(),
  }
);

// User selections - what elements each user has selected
export const UserSelection = table(
  {
    name: 'user_selection',
    public: true,
    indexes: [
      {
        name: 'user_selection_canvasId',
        algorithm: 'btree',
        columns: ['canvasId'],
      },
      {
        name: 'user_selection_userId',
        algorithm: 'btree',
        columns: ['userId'],
      },
      {
        name: 'user_selection_elementId',
        algorithm: 'btree',
        columns: ['elementId'],
      },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    canvasId: t.u64(),
    userId: t.identity(),
    elementId: t.u64(),
  }
);

// Undo history - per-user action history for undo/redo
export const UndoEntry = table(
  {
    name: 'undo_entry',
    public: true,
    indexes: [
      { name: 'undo_entry_userId', algorithm: 'btree', columns: ['userId'] },
      {
        name: 'undo_entry_canvasId',
        algorithm: 'btree',
        columns: ['canvasId'],
      },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    userId: t.identity(),
    canvasId: t.u64(),
    actionType: t.string(), // 'create', 'update', 'delete'
    elementId: t.u64(),
    previousData: t.string(), // JSON of element state before action (for undo)
    sequenceNum: t.u64(), // Order within user's history
    isUndone: t.bool(), // Whether this action has been undone
    createdAt: t.timestamp(),
  }
);

// Clipboard - for copy/paste functionality
export const Clipboard = table(
  {
    name: 'clipboard',
    public: true,
  },
  {
    userId: t.identity().primaryKey(),
    data: t.string(), // JSON array of copied elements
    copiedAt: t.timestamp(),
  }
);

export const spacetimedb = schema(
  User,
  Canvas,
  CanvasMember,
  DrawElement,
  CursorPosition,
  UserSelection,
  UndoEntry,
  Clipboard
);
