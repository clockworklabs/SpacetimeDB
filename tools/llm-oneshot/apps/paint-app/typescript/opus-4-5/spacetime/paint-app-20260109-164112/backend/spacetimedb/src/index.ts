import { spacetimedb } from './schema';
import { t, SenderError } from 'spacetimedb/server';

// ============ User Management ============

spacetimedb.reducer('set_display_name', { displayName: t.string() }, (ctx, { displayName }) => {
  if (!displayName.trim()) throw new SenderError('Display name cannot be empty');
  
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, displayName: displayName.trim() });
  } else {
    ctx.db.user.insert({
      identity: ctx.sender,
      displayName: displayName.trim(),
      isOnline: true,
    });
  }
});

spacetimedb.clientConnected((ctx) => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({ ...user, isOnline: true });
  }
});

spacetimedb.clientDisconnected((ctx) => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({ ...user, isOnline: false });
  }
  
  // Remove cursor position when disconnecting
  const cursor = ctx.db.cursorPosition.userId.find(ctx.sender);
  if (cursor) {
    ctx.db.cursorPosition.userId.delete(ctx.sender);
  }
  
  // Mark canvas memberships as inactive
  for (const member of ctx.db.canvasMember.iter()) {
    if (member.userId.toHexString() === ctx.sender.toHexString() && member.isActive) {
      ctx.db.canvasMember.id.update({ ...member, isActive: false });
    }
  }
});

// ============ Canvas Management ============

spacetimedb.reducer('create_canvas', { name: t.string() }, (ctx, { name }) => {
  if (!name.trim()) throw new SenderError('Canvas name cannot be empty');
  
  ctx.db.canvas.insert({
    id: 0n,
    name: name.trim(),
    ownerId: ctx.sender,
    createdAt: ctx.timestamp,
  });
});

spacetimedb.reducer('join_canvas', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  const canvas = ctx.db.canvas.id.find(canvasId);
  if (!canvas) throw new SenderError('Canvas not found');
  
  // Check if already a member
  for (const member of ctx.db.canvasMember.iter()) {
    if (member.canvasId === canvasId && member.userId.toHexString() === ctx.sender.toHexString()) {
      // Already a member, just reactivate
      ctx.db.canvasMember.id.update({
        ...member,
        isActive: true,
        lastActivity: ctx.timestamp,
      });
      return;
    }
  }
  
  // Create new membership
  ctx.db.canvasMember.insert({
    id: 0n,
    canvasId,
    userId: ctx.sender,
    currentTool: 'brush',
    brushColor: '#4cf490',
    brushSize: 5n,
    isActive: true,
    lastActivity: ctx.timestamp,
  });
});

spacetimedb.reducer('leave_canvas', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  for (const member of ctx.db.canvasMember.iter()) {
    if (member.canvasId === canvasId && member.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.canvasMember.id.update({ ...member, isActive: false });
      break;
    }
  }
  
  // Clear cursor position
  const cursor = ctx.db.cursorPosition.userId.find(ctx.sender);
  if (cursor && cursor.canvasId === canvasId) {
    ctx.db.cursorPosition.userId.delete(ctx.sender);
  }
  
  // Clear selections
  for (const sel of ctx.db.userSelection.iter()) {
    if (sel.canvasId === canvasId && sel.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.userSelection.id.delete(sel.id);
    }
  }
});

spacetimedb.reducer('update_tool_settings', { 
  canvasId: t.u64(), 
  tool: t.string(), 
  color: t.string(), 
  size: t.u64() 
}, (ctx, { canvasId, tool, color, size }) => {
  for (const member of ctx.db.canvasMember.iter()) {
    if (member.canvasId === canvasId && member.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.canvasMember.id.update({
        ...member,
        currentTool: tool,
        brushColor: color,
        brushSize: size,
        lastActivity: ctx.timestamp,
      });
      break;
    }
  }
});

// ============ Drawing Elements ============

function getNextSequenceNum(ctx: any, canvasId: bigint): bigint {
  let maxSeq = 0n;
  for (const entry of ctx.db.undoEntry.iter()) {
    if (entry.canvasId === canvasId && entry.userId.toHexString() === ctx.sender.toHexString()) {
      if (entry.sequenceNum > maxSeq) maxSeq = entry.sequenceNum;
    }
  }
  return maxSeq + 1n;
}

function getNextZIndex(ctx: any, canvasId: bigint): bigint {
  let maxZ = 0n;
  for (const el of ctx.db.drawElement.iter()) {
    if (el.canvasId === canvasId && !el.isDeleted && el.zIndex > maxZ) {
      maxZ = el.zIndex;
    }
  }
  return maxZ + 1n;
}

function clearRedoHistory(ctx: any, canvasId: bigint): void {
  // When a new action is performed, clear any undone entries (redo history)
  for (const entry of ctx.db.undoEntry.iter()) {
    if (entry.canvasId === canvasId && 
        entry.userId.toHexString() === ctx.sender.toHexString() && 
        entry.isUndone) {
      ctx.db.undoEntry.id.delete(entry.id);
    }
  }
}

spacetimedb.reducer('create_element', { 
  canvasId: t.u64(), 
  elementType: t.string(), 
  data: t.string() 
}, (ctx, { canvasId, elementType, data }) => {
  const canvas = ctx.db.canvas.id.find(canvasId);
  if (!canvas) throw new SenderError('Canvas not found');
  
  clearRedoHistory(ctx, canvasId);
  
  const zIndex = getNextZIndex(ctx, canvasId);
  const row = ctx.db.drawElement.insert({
    id: 0n,
    canvasId,
    ownerId: ctx.sender,
    elementType,
    data,
    isDeleted: false,
    createdAt: ctx.timestamp,
    zIndex,
  });
  
  // Record for undo
  ctx.db.undoEntry.insert({
    id: 0n,
    userId: ctx.sender,
    canvasId,
    actionType: 'create',
    elementId: row.id,
    previousData: '',
    sequenceNum: getNextSequenceNum(ctx, canvasId),
    isUndone: false,
    createdAt: ctx.timestamp,
  });
});

spacetimedb.reducer('update_element', { 
  elementId: t.u64(), 
  data: t.string() 
}, (ctx, { elementId, data }) => {
  const element = ctx.db.drawElement.id.find(elementId);
  if (!element) throw new SenderError('Element not found');
  if (element.ownerId.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Cannot modify elements created by other users');
  }
  
  clearRedoHistory(ctx, element.canvasId);
  
  // Record previous state for undo
  ctx.db.undoEntry.insert({
    id: 0n,
    userId: ctx.sender,
    canvasId: element.canvasId,
    actionType: 'update',
    elementId,
    previousData: element.data,
    sequenceNum: getNextSequenceNum(ctx, element.canvasId),
    isUndone: false,
    createdAt: ctx.timestamp,
  });
  
  ctx.db.drawElement.id.update({ ...element, data });
});

spacetimedb.reducer('delete_element', { elementId: t.u64() }, (ctx, { elementId }) => {
  const element = ctx.db.drawElement.id.find(elementId);
  if (!element) throw new SenderError('Element not found');
  if (element.ownerId.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Cannot delete elements created by other users');
  }
  
  clearRedoHistory(ctx, element.canvasId);
  
  // Record full element for undo
  ctx.db.undoEntry.insert({
    id: 0n,
    userId: ctx.sender,
    canvasId: element.canvasId,
    actionType: 'delete',
    elementId,
    previousData: JSON.stringify({
      elementType: element.elementType,
      data: element.data,
      zIndex: element.zIndex.toString(),
    }),
    sequenceNum: getNextSequenceNum(ctx, element.canvasId),
    isUndone: false,
    createdAt: ctx.timestamp,
  });
  
  ctx.db.drawElement.id.update({ ...element, isDeleted: true });
});

spacetimedb.reducer('transform_element', { 
  elementId: t.u64(), 
  transformData: t.string() 
}, (ctx, { elementId, transformData }) => {
  const element = ctx.db.drawElement.id.find(elementId);
  if (!element) throw new SenderError('Element not found');
  if (element.ownerId.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Cannot transform elements created by other users');
  }
  
  clearRedoHistory(ctx, element.canvasId);
  
  // Record previous state for undo
  ctx.db.undoEntry.insert({
    id: 0n,
    userId: ctx.sender,
    canvasId: element.canvasId,
    actionType: 'update',
    elementId,
    previousData: element.data,
    sequenceNum: getNextSequenceNum(ctx, element.canvasId),
    isUndone: false,
    createdAt: ctx.timestamp,
  });
  
  // Merge transform into existing data
  const currentData = JSON.parse(element.data);
  const transform = JSON.parse(transformData);
  const newData = { ...currentData, ...transform };
  
  ctx.db.drawElement.id.update({ ...element, data: JSON.stringify(newData) });
});

// ============ Cursor Presence ============

spacetimedb.reducer('update_cursor', { 
  canvasId: t.u64(), 
  x: t.u64(), 
  y: t.u64(), 
  tool: t.string(), 
  color: t.string() 
}, (ctx, { canvasId, x, y, tool, color }) => {
  const existing = ctx.db.cursorPosition.userId.find(ctx.sender);
  if (existing) {
    ctx.db.cursorPosition.userId.update({
      ...existing,
      canvasId,
      x,
      y,
      tool,
      color,
      lastUpdate: ctx.timestamp,
    });
  } else {
    ctx.db.cursorPosition.insert({
      userId: ctx.sender,
      canvasId,
      x,
      y,
      tool,
      color,
      lastUpdate: ctx.timestamp,
    });
  }
});

// ============ Selection ============

spacetimedb.reducer('select_element', { canvasId: t.u64(), elementId: t.u64() }, (ctx, { canvasId, elementId }) => {
  const element = ctx.db.drawElement.id.find(elementId);
  if (!element || element.isDeleted) throw new SenderError('Element not found');
  
  // Check if already selected
  for (const sel of ctx.db.userSelection.iter()) {
    if (sel.userId.toHexString() === ctx.sender.toHexString() && sel.elementId === elementId) {
      return; // Already selected
    }
  }
  
  ctx.db.userSelection.insert({
    id: 0n,
    canvasId,
    userId: ctx.sender,
    elementId,
  });
});

spacetimedb.reducer('deselect_element', { elementId: t.u64() }, (ctx, { elementId }) => {
  for (const sel of ctx.db.userSelection.iter()) {
    if (sel.userId.toHexString() === ctx.sender.toHexString() && sel.elementId === elementId) {
      ctx.db.userSelection.id.delete(sel.id);
      break;
    }
  }
});

spacetimedb.reducer('clear_selection', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  const toDelete: bigint[] = [];
  for (const sel of ctx.db.userSelection.iter()) {
    if (sel.canvasId === canvasId && sel.userId.toHexString() === ctx.sender.toHexString()) {
      toDelete.push(sel.id);
    }
  }
  for (const id of toDelete) {
    ctx.db.userSelection.id.delete(id);
  }
});

spacetimedb.reducer('delete_selected', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  const selections: bigint[] = [];
  for (const sel of ctx.db.userSelection.iter()) {
    if (sel.canvasId === canvasId && sel.userId.toHexString() === ctx.sender.toHexString()) {
      selections.push(sel.elementId);
    }
  }
  
  for (const elementId of selections) {
    const element = ctx.db.drawElement.id.find(elementId);
    if (element && !element.isDeleted && element.ownerId.toHexString() === ctx.sender.toHexString()) {
      clearRedoHistory(ctx, canvasId);
      
      ctx.db.undoEntry.insert({
        id: 0n,
        userId: ctx.sender,
        canvasId,
        actionType: 'delete',
        elementId,
        previousData: JSON.stringify({
          elementType: element.elementType,
          data: element.data,
          zIndex: element.zIndex.toString(),
        }),
        sequenceNum: getNextSequenceNum(ctx, canvasId),
        isUndone: false,
        createdAt: ctx.timestamp,
      });
      
      ctx.db.drawElement.id.update({ ...element, isDeleted: true });
    }
  }
  
  // Clear selections
  for (const sel of ctx.db.userSelection.iter()) {
    if (sel.canvasId === canvasId && sel.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.userSelection.id.delete(sel.id);
    }
  }
});

// ============ Copy/Paste ============

spacetimedb.reducer('copy_selection', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  const elements: any[] = [];
  
  for (const sel of ctx.db.userSelection.iter()) {
    if (sel.canvasId === canvasId && sel.userId.toHexString() === ctx.sender.toHexString()) {
      const element = ctx.db.drawElement.id.find(sel.elementId);
      if (element && !element.isDeleted && element.ownerId.toHexString() === ctx.sender.toHexString()) {
        elements.push({
          elementType: element.elementType,
          data: element.data,
        });
      }
    }
  }
  
  if (elements.length === 0) return;
  
  const existing = ctx.db.clipboard.userId.find(ctx.sender);
  if (existing) {
    ctx.db.clipboard.userId.update({
      ...existing,
      data: JSON.stringify(elements),
      copiedAt: ctx.timestamp,
    });
  } else {
    ctx.db.clipboard.insert({
      userId: ctx.sender,
      data: JSON.stringify(elements),
      copiedAt: ctx.timestamp,
    });
  }
});

spacetimedb.reducer('paste', { canvasId: t.u64(), offsetX: t.u64(), offsetY: t.u64() }, (ctx, { canvasId, offsetX, offsetY }) => {
  const clipboard = ctx.db.clipboard.userId.find(ctx.sender);
  if (!clipboard) throw new SenderError('Nothing to paste');
  
  const elements = JSON.parse(clipboard.data);
  if (!Array.isArray(elements) || elements.length === 0) {
    throw new SenderError('Nothing to paste');
  }
  
  clearRedoHistory(ctx, canvasId);
  
  for (const el of elements) {
    const data = JSON.parse(el.data);
    // Offset position for paste
    if (data.x !== undefined) data.x = (parseInt(data.x) || 0) + Number(offsetX);
    if (data.y !== undefined) data.y = (parseInt(data.y) || 0) + Number(offsetY);
    if (data.points) {
      data.points = data.points.map((p: any) => ({
        x: (p.x || 0) + Number(offsetX),
        y: (p.y || 0) + Number(offsetY),
      }));
    }
    
    const zIndex = getNextZIndex(ctx, canvasId);
    const row = ctx.db.drawElement.insert({
      id: 0n,
      canvasId,
      ownerId: ctx.sender,
      elementType: el.elementType,
      data: JSON.stringify(data),
      isDeleted: false,
      createdAt: ctx.timestamp,
      zIndex,
    });
    
    ctx.db.undoEntry.insert({
      id: 0n,
      userId: ctx.sender,
      canvasId,
      actionType: 'create',
      elementId: row.id,
      previousData: '',
      sequenceNum: getNextSequenceNum(ctx, canvasId),
      isUndone: false,
      createdAt: ctx.timestamp,
    });
  }
});

// ============ Undo/Redo ============

spacetimedb.reducer('undo', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  // Find the latest non-undone entry for this user on this canvas
  let latestEntry: any = null;
  let latestSeq = 0n;
  
  for (const entry of ctx.db.undoEntry.iter()) {
    if (entry.canvasId === canvasId && 
        entry.userId.toHexString() === ctx.sender.toHexString() && 
        !entry.isUndone &&
        entry.sequenceNum > latestSeq) {
      latestSeq = entry.sequenceNum;
      latestEntry = entry;
    }
  }
  
  if (!latestEntry) return; // Nothing to undo
  
  const element = ctx.db.drawElement.id.find(latestEntry.elementId);
  
  if (latestEntry.actionType === 'create') {
    // Undo create = delete
    if (element && !element.isDeleted) {
      ctx.db.drawElement.id.update({ ...element, isDeleted: true });
    }
  } else if (latestEntry.actionType === 'update') {
    // Undo update = restore previous data
    if (element) {
      ctx.db.drawElement.id.update({ ...element, data: latestEntry.previousData });
    }
  } else if (latestEntry.actionType === 'delete') {
    // Undo delete = restore element
    if (element) {
      ctx.db.drawElement.id.update({ ...element, isDeleted: false });
    }
  }
  
  // Mark entry as undone
  ctx.db.undoEntry.id.update({ ...latestEntry, isUndone: true });
});

spacetimedb.reducer('redo', { canvasId: t.u64() }, (ctx, { canvasId }) => {
  // Find the earliest undone entry for this user on this canvas
  let earliestEntry: any = null;
  let earliestSeq = BigInt(Number.MAX_SAFE_INTEGER);
  
  for (const entry of ctx.db.undoEntry.iter()) {
    if (entry.canvasId === canvasId && 
        entry.userId.toHexString() === ctx.sender.toHexString() && 
        entry.isUndone &&
        entry.sequenceNum < earliestSeq) {
      earliestSeq = entry.sequenceNum;
      earliestEntry = entry;
    }
  }
  
  if (!earliestEntry) return; // Nothing to redo
  
  const element = ctx.db.drawElement.id.find(earliestEntry.elementId);
  
  if (earliestEntry.actionType === 'create') {
    // Redo create = undelete
    if (element) {
      ctx.db.drawElement.id.update({ ...element, isDeleted: false });
    }
  } else if (earliestEntry.actionType === 'update') {
    // Redo update = we need current data to restore after undo
    // This is tricky - we stored previous data, but for redo we need "after" data
    // For simplicity, swap the stored data
    if (element) {
      const currentData = element.data;
      ctx.db.drawElement.id.update({ ...element, data: earliestEntry.previousData });
      ctx.db.undoEntry.id.update({ ...earliestEntry, previousData: currentData, isUndone: false });
      return;
    }
  } else if (earliestEntry.actionType === 'delete') {
    // Redo delete = delete again
    if (element) {
      ctx.db.drawElement.id.update({ ...element, isDeleted: true });
    }
  }
  
  // Mark entry as not undone
  ctx.db.undoEntry.id.update({ ...earliestEntry, isUndone: false });
});
