import { useState, useEffect, useRef, useCallback } from 'react';
import { useTable } from 'spacetimedb/react';
import { tables } from './module_bindings';

// Tool types
type Tool = 'brush' | 'eraser' | 'rect' | 'ellipse' | 'line' | 'fill' | 'select';

// Element data types
interface StrokeData {
  points: { x: number; y: number }[];
  color: string;
  size: number;
  opacity: number;
}

interface ShapeData {
  x: number;
  y: number;
  width: number;
  height: number;
  strokeColor: string;
  fillColor: string;
  strokeWidth: number;
  rotation?: number;
}

interface LineData {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  color: string;
  size: number;
}

// User colors for presence
const USER_COLORS = [
  '#4cf490', '#a880ff', '#02befa', '#fbdc8e', '#ff80fb', '#00ccb4', '#ff9e9e', '#fc6897'
];

function getUserColor(identity: string): string {
  let hash = 0;
  for (let i = 0; i < identity.length; i++) {
    hash = identity.charCodeAt(i) + ((hash << 5) - hash);
  }
  return USER_COLORS[Math.abs(hash) % USER_COLORS.length];
}

function getToolIcon(tool: string): string {
  switch (tool) {
    case 'brush': return '✏️';
    case 'eraser': return '🧹';
    case 'rect': return '⬜';
    case 'ellipse': return '⭕';
    case 'line': return '📏';
    case 'fill': return '🪣';
    case 'select': return '👆';
    default: return '✏️';
  }
}

export default function App() {
  const [users, usersLoading] = useTable(tables.user);
  const [canvases, canvasesLoading] = useTable(tables.canvas);
  const [canvasMembers] = useTable(tables.canvasMember);
  const [drawElements] = useTable(tables.drawElement);
  const [cursorPositions] = useTable(tables.cursorPosition);
  const [_userSelections] = useTable(tables.userSelection);
  const [undoEntries] = useTable(tables.undoEntry);

  const [displayName, setDisplayName] = useState('');
  const [newCanvasName, setNewCanvasName] = useState('');
  const [showNewCanvasModal, setShowNewCanvasModal] = useState(false);
  const [activeCanvasId, setActiveCanvasId] = useState<bigint | null>(null);

  // Drawing state
  const [tool, setTool] = useState<Tool>('brush');
  const [color, setColor] = useState('#4cf490');
  const [fillColor, setFillColor] = useState('#4cf49080');
  const [brushSize, setBrushSize] = useState(5);
  const [opacity, setOpacity] = useState(100);
  const [isDrawing, setIsDrawing] = useState(false);
  const [currentStroke, setCurrentStroke] = useState<{ x: number; y: number }[]>([]);
  const [shapeStart, setShapeStart] = useState<{ x: number; y: number } | null>(null);
  const [shapePreview, setShapePreview] = useState<{ x: number; y: number; width: number; height: number } | null>(null);

  // Selection state
  const [selectedElements, setSelectedElements] = useState<Set<bigint>>(new Set());
  const [selectionBox, setSelectionBox] = useState<{ x: number; y: number; width: number; height: number } | null>(null);

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const myIdentity = window.__my_identity;
  const conn = window.__db_conn;

  // Find current user
  const currentUser = users.find(u => u.identity.toHexString() === myIdentity?.toHexString());

  // Get active canvas data
  const activeCanvas = activeCanvasId ? canvases.find(c => c.id === activeCanvasId) : null;
  const activeMembersData = activeCanvasId 
    ? canvasMembers.filter(m => m.canvasId === activeCanvasId && m.isActive)
    : [];
  const activeElements = activeCanvasId
    ? drawElements.filter(e => e.canvasId === activeCanvasId && !e.isDeleted)
        .sort((a, b) => Number(a.zIndex - b.zIndex))
    : [];
  const activeCursors = activeCanvasId
    ? cursorPositions.filter(c => c.canvasId === activeCanvasId && c.userId.toHexString() !== myIdentity?.toHexString())
    : [];

  // Undo/redo availability
  const myUndoEntries = activeCanvasId
    ? undoEntries.filter(e => e.canvasId === activeCanvasId && e.userId.toHexString() === myIdentity?.toHexString())
    : [];
  const canUndo = myUndoEntries.some(e => !e.isUndone);
  const canRedo = myUndoEntries.some(e => e.isUndone);

  // Check cursor inactivity (fade after 5 seconds)
  const now = Date.now();

  // Set display name
  const handleSetName = () => {
    if (!displayName.trim() || !conn) return;
    conn.reducers.setDisplayName({ displayName: displayName.trim() });
  };

  // Create canvas
  const handleCreateCanvas = () => {
    if (!newCanvasName.trim() || !conn) return;
    conn.reducers.createCanvas({ name: newCanvasName.trim() });
    setNewCanvasName('');
    setShowNewCanvasModal(false);
  };

  // Join canvas
  const handleJoinCanvas = (canvasId: bigint) => {
    if (!conn) return;
    if (activeCanvasId && activeCanvasId !== canvasId) {
      conn.reducers.leaveCanvas({ canvasId: activeCanvasId });
    }
    conn.reducers.joinCanvas({ canvasId });
    setActiveCanvasId(canvasId);
    setSelectedElements(new Set());
  };

  // Leave canvas
  const handleLeaveCanvas = () => {
    if (!conn || !activeCanvasId) return;
    conn.reducers.leaveCanvas({ canvasId: activeCanvasId });
    setActiveCanvasId(null);
    setSelectedElements(new Set());
  };

  // Update tool settings
  useEffect(() => {
    if (!conn || !activeCanvasId) return;
    conn.reducers.updateToolSettings({
      canvasId: activeCanvasId,
      tool,
      color,
      size: BigInt(brushSize),
    });
  }, [conn, activeCanvasId, tool, color, brushSize]);

  // Update cursor position
  const updateCursor = useCallback((x: number, y: number) => {
    if (!conn || !activeCanvasId) return;
    conn.reducers.updateCursor({
      canvasId: activeCanvasId,
      x: BigInt(Math.round(x)),
      y: BigInt(Math.round(y)),
      tool,
      color,
    });
  }, [conn, activeCanvasId, tool, color]);

  // Handle mouse move for cursor presence
  const handleMouseMove = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    updateCursor(x, y);

    if (isDrawing) {
      if (tool === 'brush' || tool === 'eraser') {
        setCurrentStroke(prev => [...prev, { x, y }]);
      } else if (shapeStart && (tool === 'rect' || tool === 'ellipse' || tool === 'line')) {
        const width = x - shapeStart.x;
        const height = y - shapeStart.y;
        
        // Shift key for perfect squares/circles
        if (e.shiftKey && (tool === 'rect' || tool === 'ellipse')) {
          const size = Math.max(Math.abs(width), Math.abs(height));
          setShapePreview({
            x: shapeStart.x,
            y: shapeStart.y,
            width: width >= 0 ? size : -size,
            height: height >= 0 ? size : -size,
          });
        } else {
          setShapePreview({
            x: shapeStart.x,
            y: shapeStart.y,
            width,
            height,
          });
        }
      } else if (tool === 'select' && selectionBox) {
        setSelectionBox({
          ...selectionBox,
          width: x - selectionBox.x,
          height: y - selectionBox.y,
        });
      }
    }
  }, [isDrawing, tool, shapeStart, selectionBox, updateCursor]);

  // Handle mouse down
  const handleMouseDown = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas || !conn || !activeCanvasId) return;

    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    setIsDrawing(true);

    if (tool === 'brush' || tool === 'eraser') {
      setCurrentStroke([{ x, y }]);
    } else if (tool === 'rect' || tool === 'ellipse' || tool === 'line') {
      setShapeStart({ x, y });
      setShapePreview({ x, y, width: 0, height: 0 });
    } else if (tool === 'select') {
      // Clear previous selection if clicking on empty space
      setSelectedElements(new Set());
      conn.reducers.clearSelection({ canvasId: activeCanvasId });
      setSelectionBox({ x, y, width: 0, height: 0 });
    } else if (tool === 'fill') {
      // Fill tool - create a fill element at click position
      const fillData = JSON.stringify({
        x: Math.round(x),
        y: Math.round(y),
        color: fillColor,
      });
      conn.reducers.createElement({
        canvasId: activeCanvasId,
        elementType: 'fill',
        data: fillData,
      });
    }
  }, [tool, conn, activeCanvasId, fillColor]);

  // Handle mouse up
  const handleMouseUp = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!isDrawing || !conn || !activeCanvasId) {
      setIsDrawing(false);
      return;
    }

    const canvas = canvasRef.current;
    if (!canvas) return;

    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    if (tool === 'brush' || tool === 'eraser') {
      if (currentStroke.length > 0) {
        const strokeData: StrokeData = {
          points: currentStroke,
          color: tool === 'eraser' ? '#060606' : color,
          size: brushSize,
          opacity: tool === 'eraser' ? 100 : opacity,
        };
        conn.reducers.createElement({
          canvasId: activeCanvasId,
          elementType: 'stroke',
          data: JSON.stringify(strokeData),
        });
      }
      setCurrentStroke([]);
    } else if (shapeStart && shapePreview) {
      let finalWidth = shapePreview.width;
      let finalHeight = shapePreview.height;

      // Normalize negative dimensions
      let finalX = shapeStart.x;
      let finalY = shapeStart.y;
      if (finalWidth < 0) {
        finalX += finalWidth;
        finalWidth = -finalWidth;
      }
      if (finalHeight < 0) {
        finalY += finalHeight;
        finalHeight = -finalHeight;
      }

      if (tool === 'rect' || tool === 'ellipse') {
        if (finalWidth > 2 || finalHeight > 2) {
          const shapeData: ShapeData = {
            x: Math.round(finalX),
            y: Math.round(finalY),
            width: Math.round(finalWidth),
            height: Math.round(finalHeight),
            strokeColor: color,
            fillColor: fillColor,
            strokeWidth: brushSize,
          };
          conn.reducers.createElement({
            canvasId: activeCanvasId,
            elementType: tool,
            data: JSON.stringify(shapeData),
          });
        }
      } else if (tool === 'line') {
        const lineData: LineData = {
          x1: Math.round(shapeStart.x),
          y1: Math.round(shapeStart.y),
          x2: Math.round(x),
          y2: Math.round(y),
          color,
          size: brushSize,
        };
        conn.reducers.createElement({
          canvasId: activeCanvasId,
          elementType: 'line',
          data: JSON.stringify(lineData),
        });
      }
      setShapeStart(null);
      setShapePreview(null);
    } else if (tool === 'select' && selectionBox) {
      // Find elements within selection box
      const box = {
        x: selectionBox.width < 0 ? selectionBox.x + selectionBox.width : selectionBox.x,
        y: selectionBox.height < 0 ? selectionBox.y + selectionBox.height : selectionBox.y,
        width: Math.abs(selectionBox.width),
        height: Math.abs(selectionBox.height),
      };

      const newSelection = new Set<bigint>();
      for (const el of activeElements) {
        // Only select own elements
        if (el.ownerId.toHexString() !== myIdentity?.toHexString()) continue;

        const data = JSON.parse(el.data);
        let elementBounds = { x: 0, y: 0, width: 0, height: 0 };

        if (el.elementType === 'stroke') {
          const points = data.points || [];
          if (points.length > 0) {
            const xs = points.map((p: any) => p.x);
            const ys = points.map((p: any) => p.y);
            elementBounds = {
              x: Math.min(...xs),
              y: Math.min(...ys),
              width: Math.max(...xs) - Math.min(...xs),
              height: Math.max(...ys) - Math.min(...ys),
            };
          }
        } else if (el.elementType === 'rect' || el.elementType === 'ellipse') {
          elementBounds = { x: data.x, y: data.y, width: data.width, height: data.height };
        } else if (el.elementType === 'line') {
          elementBounds = {
            x: Math.min(data.x1, data.x2),
            y: Math.min(data.y1, data.y2),
            width: Math.abs(data.x2 - data.x1),
            height: Math.abs(data.y2 - data.y1),
          };
        }

        // Check intersection
        if (box.width > 5 && box.height > 5) {
          if (elementBounds.x < box.x + box.width &&
              elementBounds.x + elementBounds.width > box.x &&
              elementBounds.y < box.y + box.height &&
              elementBounds.y + elementBounds.height > box.y) {
            newSelection.add(el.id);
            conn.reducers.selectElement({ canvasId: activeCanvasId, elementId: el.id });
          }
        }
      }
      setSelectedElements(newSelection);
      setSelectionBox(null);
    }

    setIsDrawing(false);
  }, [isDrawing, tool, currentStroke, shapeStart, shapePreview, selectionBox, conn, activeCanvasId, color, fillColor, brushSize, opacity, activeElements, myIdentity]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!conn || !activeCanvasId) return;

      // Undo: Ctrl+Z
      if (e.ctrlKey && e.key === 'z' && !e.shiftKey) {
        e.preventDefault();
        if (canUndo) {
          conn.reducers.undo({ canvasId: activeCanvasId });
        }
      }
      // Redo: Ctrl+Y or Ctrl+Shift+Z
      else if ((e.ctrlKey && e.key === 'y') || (e.ctrlKey && e.shiftKey && e.key === 'z')) {
        e.preventDefault();
        if (canRedo) {
          conn.reducers.redo({ canvasId: activeCanvasId });
        }
      }
      // Delete: Delete or Backspace
      else if ((e.key === 'Delete' || e.key === 'Backspace') && selectedElements.size > 0) {
        e.preventDefault();
        conn.reducers.deleteSelected({ canvasId: activeCanvasId });
        setSelectedElements(new Set());
      }
      // Copy: Ctrl+C
      else if (e.ctrlKey && e.key === 'c') {
        e.preventDefault();
        conn.reducers.copySelection({ canvasId: activeCanvasId });
      }
      // Paste: Ctrl+V
      else if (e.ctrlKey && e.key === 'v') {
        e.preventDefault();
        conn.reducers.paste({ canvasId: activeCanvasId, offsetX: 20n, offsetY: 20n });
      }
      // Escape: Deselect
      else if (e.key === 'Escape') {
        conn.reducers.clearSelection({ canvasId: activeCanvasId });
        setSelectedElements(new Set());
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [conn, activeCanvasId, canUndo, canRedo, selectedElements]);

  // Draw canvas
  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;

    // Set canvas size
    canvas.width = container.clientWidth;
    canvas.height = container.clientHeight;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Clear canvas
    ctx.fillStyle = '#060606';
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // Draw grid
    ctx.strokeStyle = '#141416';
    ctx.lineWidth = 1;
    const gridSize = 20;
    for (let x = 0; x < canvas.width; x += gridSize) {
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, canvas.height);
      ctx.stroke();
    }
    for (let y = 0; y < canvas.height; y += gridSize) {
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(canvas.width, y);
      ctx.stroke();
    }

    // Draw elements
    for (const el of activeElements) {
      const data = JSON.parse(el.data);
      const isSelected = selectedElements.has(el.id);

      if (el.elementType === 'stroke') {
        const points = data.points || [];
        if (points.length < 2) continue;

        ctx.beginPath();
        ctx.strokeStyle = data.color;
        ctx.lineWidth = data.size;
        ctx.lineCap = 'round';
        ctx.lineJoin = 'round';
        ctx.globalAlpha = (data.opacity || 100) / 100;
        ctx.moveTo(points[0].x, points[0].y);
        for (let i = 1; i < points.length; i++) {
          ctx.lineTo(points[i].x, points[i].y);
        }
        ctx.stroke();
        ctx.globalAlpha = 1;
      } else if (el.elementType === 'rect') {
        ctx.fillStyle = data.fillColor || 'transparent';
        ctx.strokeStyle = data.strokeColor;
        ctx.lineWidth = data.strokeWidth;
        ctx.fillRect(data.x, data.y, data.width, data.height);
        ctx.strokeRect(data.x, data.y, data.width, data.height);
      } else if (el.elementType === 'ellipse') {
        ctx.beginPath();
        ctx.ellipse(
          data.x + data.width / 2,
          data.y + data.height / 2,
          data.width / 2,
          data.height / 2,
          0, 0, Math.PI * 2
        );
        ctx.fillStyle = data.fillColor || 'transparent';
        ctx.fill();
        ctx.strokeStyle = data.strokeColor;
        ctx.lineWidth = data.strokeWidth;
        ctx.stroke();
      } else if (el.elementType === 'line') {
        ctx.beginPath();
        ctx.strokeStyle = data.color;
        ctx.lineWidth = data.size;
        ctx.lineCap = 'round';
        ctx.moveTo(data.x1, data.y1);
        ctx.lineTo(data.x2, data.y2);
        ctx.stroke();
      } else if (el.elementType === 'fill') {
        // Simple fill representation - draw a colored circle
        ctx.beginPath();
        ctx.fillStyle = data.color;
        ctx.arc(data.x, data.y, 10, 0, Math.PI * 2);
        ctx.fill();
      }

      // Draw selection highlight
      if (isSelected) {
        let bounds = { x: 0, y: 0, width: 0, height: 0 };
        if (el.elementType === 'stroke') {
          const points = data.points || [];
          if (points.length > 0) {
            const xs = points.map((p: any) => p.x);
            const ys = points.map((p: any) => p.y);
            bounds = {
              x: Math.min(...xs) - 5,
              y: Math.min(...ys) - 5,
              width: Math.max(...xs) - Math.min(...xs) + 10,
              height: Math.max(...ys) - Math.min(...ys) + 10,
            };
          }
        } else if (el.elementType === 'rect' || el.elementType === 'ellipse') {
          bounds = { x: data.x - 5, y: data.y - 5, width: data.width + 10, height: data.height + 10 };
        } else if (el.elementType === 'line') {
          bounds = {
            x: Math.min(data.x1, data.x2) - 5,
            y: Math.min(data.y1, data.y2) - 5,
            width: Math.abs(data.x2 - data.x1) + 10,
            height: Math.abs(data.y2 - data.y1) + 10,
          };
        }

        ctx.strokeStyle = '#02befa';
        ctx.lineWidth = 2;
        ctx.setLineDash([5, 5]);
        ctx.strokeRect(bounds.x, bounds.y, bounds.width, bounds.height);
        ctx.setLineDash([]);
      }
    }

    // Draw current stroke preview
    if (currentStroke.length > 1) {
      ctx.beginPath();
      ctx.strokeStyle = tool === 'eraser' ? '#060606' : color;
      ctx.lineWidth = brushSize;
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';
      ctx.globalAlpha = tool === 'eraser' ? 1 : opacity / 100;
      ctx.moveTo(currentStroke[0].x, currentStroke[0].y);
      for (let i = 1; i < currentStroke.length; i++) {
        ctx.lineTo(currentStroke[i].x, currentStroke[i].y);
      }
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    // Draw shape preview
    if (shapePreview && shapeStart) {
      ctx.strokeStyle = color;
      ctx.fillStyle = fillColor;
      ctx.lineWidth = brushSize;

      let x = shapePreview.x;
      let y = shapePreview.y;
      let w = shapePreview.width;
      let h = shapePreview.height;

      if (w < 0) { x += w; w = -w; }
      if (h < 0) { y += h; h = -h; }

      if (tool === 'rect') {
        ctx.fillRect(x, y, w, h);
        ctx.strokeRect(x, y, w, h);
      } else if (tool === 'ellipse') {
        ctx.beginPath();
        ctx.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
      } else if (tool === 'line') {
        ctx.beginPath();
        ctx.moveTo(shapeStart.x, shapeStart.y);
        ctx.lineTo(shapeStart.x + shapePreview.width, shapeStart.y + shapePreview.height);
        ctx.stroke();
      }
    }

    // Draw selection box
    if (selectionBox && Math.abs(selectionBox.width) > 2 && Math.abs(selectionBox.height) > 2) {
      let x = selectionBox.x;
      let y = selectionBox.y;
      let w = selectionBox.width;
      let h = selectionBox.height;
      if (w < 0) { x += w; w = -w; }
      if (h < 0) { y += h; h = -h; }

      ctx.strokeStyle = '#02befa';
      ctx.fillStyle = 'rgba(2, 190, 250, 0.1)';
      ctx.lineWidth = 2;
      ctx.setLineDash([5, 5]);
      ctx.fillRect(x, y, w, h);
      ctx.strokeRect(x, y, w, h);
      ctx.setLineDash([]);
    }

  }, [activeElements, currentStroke, shapePreview, shapeStart, selectionBox, tool, color, fillColor, brushSize, opacity, selectedElements]);

  // Resize handler
  useEffect(() => {
    const handleResize = () => {
      const canvas = canvasRef.current;
      const container = containerRef.current;
      if (canvas && container) {
        canvas.width = container.clientWidth;
        canvas.height = container.clientHeight;
      }
    };

    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []);

  // Show loading state
  if (usersLoading || canvasesLoading) {
    return (
      <div className="loading">
        <div className="spinner" />
      </div>
    );
  }

  // Show welcome screen if no display name
  if (!currentUser) {
    return (
      <div className="welcome-screen">
        <div className="logo-icon" style={{ width: 64, height: 64, fontSize: 32, marginBottom: 24 }}>🎨</div>
        <h1 className="welcome-title">Paint App</h1>
        <p className="welcome-subtitle">Real-time collaborative drawing with SpacetimeDB</p>
        <div className="welcome-form">
          <input
            type="text"
            className="input"
            placeholder="Enter your display name"
            value={displayName}
            onChange={e => setDisplayName(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleSetName()}
          />
          <button className="btn btn-primary" onClick={handleSetName}>
            Join
          </button>
        </div>
      </div>
    );
  }

  // Get member count for each canvas
  const getCanvasMemberCount = (canvasId: bigint) => 
    canvasMembers.filter(m => m.canvasId === canvasId && m.isActive).length;

  return (
    <div className="app-container">
      {/* Header */}
      <header className="header">
        <div className="header-left">
          <div className="logo">
            <div className="logo-icon">🎨</div>
            Paint App
          </div>
          {activeCanvas && (
            <span style={{ color: 'var(--stdb-text-muted)' }}>
              / {activeCanvas.name}
            </span>
          )}
        </div>
        <div className="user-info">
          <span className="status-dot online" />
          <span className="user-name">{currentUser.displayName}</span>
        </div>
      </header>

      <div className="main-content">
        {/* Sidebar */}
        <aside className="sidebar">
          <div className="sidebar-header">
            <span className="sidebar-title">Canvases</span>
            <button className="btn btn-primary" onClick={() => setShowNewCanvasModal(true)}>
              + New
            </button>
          </div>
          <div className="canvas-list">
            {canvases.length === 0 ? (
              <div className="empty-state" style={{ padding: 20 }}>
                <p className="empty-state-text">No canvases yet. Create one to start drawing!</p>
              </div>
            ) : (
              canvases.map(canvas => (
                <div
                  key={canvas.id.toString()}
                  className={`canvas-item ${activeCanvasId === canvas.id ? 'active' : ''}`}
                  onClick={() => handleJoinCanvas(canvas.id)}
                >
                  <div className="canvas-item-name">{canvas.name}</div>
                  <div className="canvas-item-meta">
                    <span className="canvas-badge">{getCanvasMemberCount(canvas.id)}</span>
                    {' '}active
                  </div>
                </div>
              ))
            )}
          </div>
        </aside>

        {/* Canvas Area */}
        <div className="canvas-area">
          {activeCanvas ? (
            <>
              {/* Toolbar */}
              <div className="toolbar">
                <div className="toolbar-section">
                  <span className="toolbar-label">Tools</span>
                  <button className={`btn btn-icon ${tool === 'brush' ? 'active' : ''}`} onClick={() => setTool('brush')} title="Brush (B)">
                    ✏️
                  </button>
                  <button className={`btn btn-icon ${tool === 'eraser' ? 'active' : ''}`} onClick={() => setTool('eraser')} title="Eraser (E)">
                    🧹
                  </button>
                  <button className={`btn btn-icon ${tool === 'line' ? 'active' : ''}`} onClick={() => setTool('line')} title="Line (L)">
                    📏
                  </button>
                  <button className={`btn btn-icon ${tool === 'rect' ? 'active' : ''}`} onClick={() => setTool('rect')} title="Rectangle (R)">
                    ⬜
                  </button>
                  <button className={`btn btn-icon ${tool === 'ellipse' ? 'active' : ''}`} onClick={() => setTool('ellipse')} title="Ellipse (O)">
                    ⭕
                  </button>
                  <button className={`btn btn-icon ${tool === 'fill' ? 'active' : ''}`} onClick={() => setTool('fill')} title="Fill (F)">
                    🪣
                  </button>
                  <button className={`btn btn-icon ${tool === 'select' ? 'active' : ''}`} onClick={() => setTool('select')} title="Select (V)">
                    👆
                  </button>
                </div>

                <div className="toolbar-section">
                  <span className="toolbar-label">Stroke</span>
                  <input
                    type="color"
                    className="color-picker"
                    value={color}
                    onChange={e => setColor(e.target.value)}
                    title="Stroke color"
                  />
                  <span className="toolbar-label">Fill</span>
                  <input
                    type="color"
                    className="color-picker"
                    value={fillColor.slice(0, 7)}
                    onChange={e => setFillColor(e.target.value + '80')}
                    title="Fill color"
                  />
                </div>

                <div className="toolbar-section">
                  <span className="toolbar-label">Size</span>
                  <input
                    type="range"
                    className="size-slider"
                    min="1"
                    max="50"
                    value={brushSize}
                    onChange={e => setBrushSize(parseInt(e.target.value))}
                  />
                  <span style={{ minWidth: 30, textAlign: 'center' }}>{brushSize}</span>
                </div>

                <div className="toolbar-section">
                  <span className="toolbar-label">Opacity</span>
                  <input
                    type="range"
                    className="size-slider"
                    min="1"
                    max="100"
                    value={opacity}
                    onChange={e => setOpacity(parseInt(e.target.value))}
                  />
                  <span style={{ minWidth: 30, textAlign: 'center' }}>{opacity}%</span>
                </div>

                <div className="toolbar-section undo-redo-group">
                  <button
                    className="btn btn-icon btn-undo-redo"
                    onClick={() => activeCanvasId && conn?.reducers.undo({ canvasId: activeCanvasId })}
                    disabled={!canUndo}
                    title="Undo (Ctrl+Z)"
                    style={{ opacity: canUndo ? 1 : 0.4 }}
                  >
                    ↩️
                  </button>
                  <button
                    className="btn btn-icon btn-undo-redo"
                    onClick={() => activeCanvasId && conn?.reducers.redo({ canvasId: activeCanvasId })}
                    disabled={!canRedo}
                    title="Redo (Ctrl+Y)"
                    style={{ opacity: canRedo ? 1 : 0.4 }}
                  >
                    ↪️
                  </button>
                </div>

                <div className="toolbar-section" style={{ borderRight: 'none', marginLeft: 'auto' }}>
                  <button className="btn btn-secondary" onClick={handleLeaveCanvas}>
                    Leave Canvas
                  </button>
                </div>
              </div>

              {/* Canvas */}
              <div className="canvas-container" ref={containerRef}>
                <canvas
                  ref={canvasRef}
                  className="drawing-canvas"
                  onMouseMove={handleMouseMove}
                  onMouseDown={handleMouseDown}
                  onMouseUp={handleMouseUp}
                  onMouseLeave={handleMouseUp}
                />

                {/* Remote cursors */}
                {activeCursors.map(cursor => {
                  const cursorUser = users.find(u => u.identity.toHexString() === cursor.userId.toHexString());
                  const lastUpdate = Number(cursor.lastUpdate.microsSinceUnixEpoch / 1000n);
                  const isFaded = now - lastUpdate > 5000;
                  const cursorColor = getUserColor(cursor.userId.toHexString());

                  return (
                    <div
                      key={cursor.userId.toHexString()}
                      className={`cursor-presence ${isFaded ? 'cursor-faded' : ''}`}
                      style={{
                        transform: `translate(${Number(cursor.x)}px, ${Number(cursor.y)}px)`,
                      }}
                    >
                      <div className="cursor-dot" style={{ backgroundColor: cursorColor }} />
                      <div className="cursor-label">
                        {cursorUser?.displayName || 'Unknown'}
                        <span className="cursor-tool">{getToolIcon(cursor.tool)}</span>
                      </div>
                    </div>
                  );
                })}
              </div>

              {/* Active users bar */}
              <div className="active-users">
                <span className="active-users-label">Active users:</span>
                {activeMembersData.map(member => {
                  const memberUser = users.find(u => u.identity.toHexString() === member.userId.toHexString());
                  const memberColor = getUserColor(member.userId.toHexString());
                  return (
                    <div
                      key={member.id.toString()}
                      className="user-avatar"
                      style={{ backgroundColor: memberColor }}
                      title={memberUser?.displayName || 'Unknown'}
                    >
                      {(memberUser?.displayName || '?')[0].toUpperCase()}
                    </div>
                  );
                })}
              </div>
            </>
          ) : (
            <div className="empty-state">
              <div className="empty-state-icon">🎨</div>
              <h2 className="empty-state-title">Select a Canvas</h2>
              <p className="empty-state-text">
                Choose a canvas from the sidebar or create a new one to start drawing.
              </p>
            </div>
          )}
        </div>
      </div>

      {/* New Canvas Modal */}
      {showNewCanvasModal && (
        <div className="modal-overlay" onClick={() => setShowNewCanvasModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h2 className="modal-title">Create New Canvas</h2>
            <div className="form-group">
              <label className="form-label">Canvas Name</label>
              <input
                type="text"
                className="input"
                style={{ width: '100%' }}
                placeholder="Enter canvas name"
                value={newCanvasName}
                onChange={e => setNewCanvasName(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleCreateCanvas()}
                autoFocus
              />
            </div>
            <div className="modal-actions">
              <button className="btn btn-secondary" onClick={() => setShowNewCanvasModal(false)}>
                Cancel
              </button>
              <button className="btn btn-primary" onClick={handleCreateCanvas}>
                Create
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
