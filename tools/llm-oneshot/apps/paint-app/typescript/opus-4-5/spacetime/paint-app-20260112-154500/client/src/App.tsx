import { useEffect, useMemo, useState, useRef } from 'react';
import { Identity } from 'spacetimedb';
import { SpacetimeDBProvider, useTable } from 'spacetimedb/react';
import { DbConnection, tables, ErrorContext } from './module_bindings';
import { MODULE_NAME, SPACETIMEDB_URI } from './config';

// Global connection state
declare global {
  interface Window {
    __db_conn: DbConnection | null;
    __my_identity: Identity | null;
  }
}
window.__db_conn = null;
window.__my_identity = null;

// ============================================================================
// TYPES
// ============================================================================

type Tool =
  | 'select'
  | 'brush'
  | 'eraser'
  | 'rectangle'
  | 'ellipse'
  | 'line'
  | 'arrow'
  | 'text'
  | 'sticky'
  | 'comment';

interface Point {
  x: number;
  y: number;
}

// Generic row types - useTable returns [readonly rows[], isLoading]
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type RowType = { [key: string]: any };

// ============================================================================
// MAIN APP WRAPPER
// ============================================================================

export default function App() {
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const connectionBuilder = useMemo(
    () =>
      DbConnection.builder()
        .withUri(SPACETIMEDB_URI)
        .withModuleName(MODULE_NAME)
        .withToken(localStorage.getItem('paint_auth_token') || undefined)
        .onConnect((conn, identity, token) => {
          console.log('Connected!', identity.toHexString());
          localStorage.setItem('paint_auth_token', token);
          window.__db_conn = conn;
          window.__my_identity = identity;
          conn
            .subscriptionBuilder()
            .onApplied(() => {
              console.log('Subscriptions applied!');
              setConnected(true);
            })
            .onError(err => {
              console.error('Subscription error:', err);
              setError('Subscription error: ' + String(err));
            })
            .subscribeToAllTables();
        })
        .onConnectError((_ctx: ErrorContext, err: Error) => {
          console.error('Connection error:', err);
          setError('Connection error: ' + err.message);
          if (
            err.message?.includes('Unauthorized') ||
            err.message?.includes('401')
          ) {
            localStorage.removeItem('paint_auth_token');
          }
        })
        .onDisconnect(() => {
          console.log('Disconnected');
          setConnected(false);
        }),
    []
  );

  return (
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      {error ? (
        <ErrorScreen error={error} />
      ) : connected ? (
        <PaintApp />
      ) : (
        <LoadingScreen />
      )}
    </SpacetimeDBProvider>
  );
}

function LoadingScreen() {
  return (
    <div
      className="app"
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
      }}
    >
      <div className="loading">
        <div className="loading-spinner" />
        <span>Connecting to SpacetimeDB...</span>
      </div>
    </div>
  );
}

function ErrorScreen({ error }: { error: string }) {
  const handleRetry = () => {
    localStorage.removeItem('paint_auth_token');
    window.location.reload();
  };

  return (
    <div
      className="app"
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
      }}
    >
      <div style={{ textAlign: 'center', padding: '2rem' }}>
        <div style={{ fontSize: '2rem', marginBottom: '1rem' }}>❌</div>
        <div style={{ color: 'var(--stdb-red)', marginBottom: '1rem' }}>
          {error}
        </div>
        <button className="btn btn-primary" onClick={handleRetry}>
          Retry
        </button>
      </div>
    </div>
  );
}

// ============================================================================
// PAINT APP
// ============================================================================

function PaintApp() {
  const conn = window.__db_conn;
  const [users] = useTable(tables.user);
  const [canvases] = useTable(tables.canvas);
  const [canvasMembers] = useTable(tables.canvasMember);
  const [canvasPresences] = useTable(tables.canvasPresence);
  const [layers] = useTable(tables.layer);
  const [strokes] = useTable(tables.stroke);
  const [shapes] = useTable(tables.shape);
  const [textElements] = useTable(tables.textElement);
  const [cursors] = useTable(tables.cursor);
  const [selections] = useTable(tables.selection);
  const [comments] = useTable(tables.comment);
  const [commentReplies] = useTable(tables.commentReply);
  const [chatMessages] = useTable(tables.chatMessage);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [activityEntries] = useTable(tables.activityEntry);
  const [versions] = useTable(tables.version);
  const [notifications] = useTable(tables.notification);

  const [activeCanvasId, setActiveCanvasId] = useState<bigint | null>(null);
  const [activeTool, setActiveTool] = useState<Tool>('brush');
  const [activeLayerId, setActiveLayerId] = useState<bigint | null>(null);
  const [strokeColor, setStrokeColor] = useState('#4cf490');
  const [fillColor, setFillColor] = useState('transparent');
  const [brushSize, setBrushSize] = useState(4);
  const [showChat, setShowChat] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showShare, setShowShare] = useState(false);
  const [showRightPanel, setShowRightPanel] = useState(true);
  const [rightPanelTab, setRightPanelTab] = useState<
    'users' | 'activity' | 'versions' | 'comments'
  >('users');

  const myIdentity = window.__my_identity;
  const myUser = users.find(
    u => u.identity.toHexString() === myIdentity?.toHexString()
  );

  // Get current canvas data
  const activeCanvas = canvases.find(c => c.id === activeCanvasId);
  const canvasLayers = layers
    .filter(l => l.canvasId === activeCanvasId)
    .sort((a, b) => a.orderIndex - b.orderIndex);
  const canvasStrokes = strokes.filter(s => s.canvasId === activeCanvasId);
  const canvasShapes = shapes.filter(s => s.canvasId === activeCanvasId);
  const canvasTexts = textElements.filter(t => t.canvasId === activeCanvasId);
  const canvasCursors = cursors.filter(c => c.canvasId === activeCanvasId);
  const canvasSelections = selections.filter(
    s => s.canvasId === activeCanvasId
  );
  const canvasComments = comments.filter(c => c.canvasId === activeCanvasId);
  const canvasChat = [
    ...chatMessages.filter(m => m.canvasId === activeCanvasId),
  ].sort((a, b) =>
    Number(a.createdAt.microsSinceUnixEpoch - b.createdAt.microsSinceUnixEpoch)
  );
  const canvasActivity = [
    ...activityEntries.filter(a => a.canvasId === activeCanvasId),
  ]
    .sort((a, b) =>
      Number(
        b.createdAt.microsSinceUnixEpoch - a.createdAt.microsSinceUnixEpoch
      )
    )
    .slice(0, 100);
  const canvasVersions = [
    ...versions.filter(v => v.canvasId === activeCanvasId),
  ].sort((a, b) =>
    Number(b.createdAt.microsSinceUnixEpoch - a.createdAt.microsSinceUnixEpoch)
  );
  const canvasPresenceList = canvasPresences.filter(
    p => p.canvasId === activeCanvasId
  );
  const canvasTyping = typingIndicators.filter(
    t => t.canvasId === activeCanvasId
  );

  // Get my membership
  const myMembership = canvasMembers.find(
    m =>
      m.canvasId === activeCanvasId &&
      m.userIdentity.toHexString() === myIdentity?.toHexString()
  );
  const isViewer = myMembership?.role === 'viewer';

  // Handle share link URL parameter
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const joinToken = params.get('join');
    if (joinToken && conn) {
      conn.reducers.joinCanvasViaLink({ shareLinkToken: joinToken });
      // Clear the URL parameter
      window.history.replaceState({}, '', window.location.pathname);
    }
  }, [conn]);

  // Auto-select first layer
  useEffect(() => {
    if (canvasLayers.length > 0 && !activeLayerId) {
      setActiveLayerId(canvasLayers[0].id);
    }
  }, [canvasLayers, activeLayerId]);

  // My canvases
  const myCanvases = canvases.filter(c => {
    const isMember = canvasMembers.some(
      m =>
        m.canvasId === c.id &&
        m.userIdentity.toHexString() === myIdentity?.toHexString()
    );
    return isMember;
  });

  // Unread notifications
  const unreadNotifications = notifications.filter(
    n => n.userIdentity.toHexString() === myIdentity?.toHexString() && !n.read
  );

  // Unread chat count (simplified)
  const unreadChatCount = showChat ? 0 : Math.min(canvasChat.length, 5);

  return (
    <div className="app">
      <AppHeader
        myUser={myUser}
        activeCanvas={activeCanvas}
        canvasPresenceList={[...canvasPresenceList]}
        unreadNotifications={unreadNotifications.length}
        unreadChatCount={unreadChatCount}
        showChat={showChat}
        setShowChat={setShowChat}
        setShowSettings={setShowSettings}
        setShowShare={setShowShare}
        isViewer={isViewer || false}
      />

      <div className="main-content">
        <LeftSidebar
          canvases={[...myCanvases]}
          activeCanvasId={activeCanvasId}
          setActiveCanvasId={setActiveCanvasId}
          conn={conn}
          canvasMembers={[...canvasMembers]}
        />

        {activeCanvas ? (
          <>
            <div className="canvas-area">
              <Toolbar
                activeTool={activeTool}
                setActiveTool={setActiveTool}
                strokeColor={strokeColor}
                setStrokeColor={setStrokeColor}
                fillColor={fillColor}
                setFillColor={setFillColor}
                brushSize={brushSize}
                setBrushSize={setBrushSize}
                isViewer={isViewer || false}
                conn={conn}
                activeCanvasId={activeCanvasId}
              />

              <DrawingCanvas
                conn={conn}
                canvasId={activeCanvasId!}
                layerId={activeLayerId}
                layers={[...canvasLayers]}
                strokes={[...canvasStrokes]}
                shapes={[...canvasShapes]}
                textElements={[...canvasTexts]}
                cursors={[...canvasCursors]}
                selections={[...canvasSelections]}
                comments={[...canvasComments]}
                users={[...users]}
                activeTool={activeTool}
                strokeColor={strokeColor}
                fillColor={fillColor}
                brushSize={brushSize}
                isViewer={isViewer || false}
                myIdentity={myIdentity}
              />

              <LayersPanel
                layers={[...canvasLayers]}
                activeLayerId={activeLayerId}
                setActiveLayerId={setActiveLayerId}
                conn={conn}
                canvasId={activeCanvasId!}
                users={[...users]}
                isViewer={isViewer || false}
              />
            </div>

            {/* Panel toggle button */}
            <button
              className="panel-toggle-btn"
              onClick={() => setShowRightPanel(!showRightPanel)}
              data-tooltip={showRightPanel ? 'Hide panel' : 'Show panel'}
              style={{
                position: 'absolute',
                right: showRightPanel ? '300px' : '0',
                top: '50%',
                transform: 'translateY(-50%)',
                zIndex: 60,
              }}
            >
              {showRightPanel ? '▶' : '◀'}
            </button>

            {showRightPanel && (
              <RightPanel
                activeTab={rightPanelTab}
                setActiveTab={setRightPanelTab}
                presences={[...canvasPresenceList]}
                users={[...users]}
                activities={canvasActivity}
                versions={canvasVersions}
                comments={[...canvasComments]}
                commentReplies={[...commentReplies]}
                conn={conn}
                canvasId={activeCanvasId}
                myIdentity={myIdentity}
              />
            )}
          </>
        ) : (
          <div
            className="canvas-area"
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
            }}
          >
            <div className="empty-state">
              <div className="empty-state-icon">🎨</div>
              <div className="empty-state-text">
                Select a canvas to start drawing
              </div>
            </div>
          </div>
        )}
      </div>

      {showChat && activeCanvasId !== null && (
        <ChatPanel
          messages={canvasChat}
          users={[...users]}
          typingIndicators={[...canvasTyping]}
          conn={conn}
          canvasId={activeCanvasId}
          myIdentity={myIdentity}
          onClose={() => setShowChat(false)}
        />
      )}

      {showSettings && (
        <SettingsModal
          myUser={myUser}
          conn={conn}
          onClose={() => setShowSettings(false)}
        />
      )}

      {showShare && activeCanvas && (
        <ShareModal
          canvas={activeCanvas}
          members={[...canvasMembers]}
          users={[...users]}
          conn={conn}
          myIdentity={myIdentity}
          onClose={() => setShowShare(false)}
        />
      )}

      {isViewer && (
        <div
          className="view-only-badge"
          style={{ position: 'fixed', bottom: '1rem', left: '220px' }}
        >
          View Only
        </div>
      )}
    </div>
  );
}

// ============================================================================
// HEADER
// ============================================================================

interface AppHeaderProps {
  myUser: RowType | undefined;
  activeCanvas: RowType | undefined;
  canvasPresenceList: RowType[];
  unreadNotifications: number;
  unreadChatCount: number;
  showChat: boolean;
  setShowChat: (v: boolean) => void;
  setShowSettings: (v: boolean) => void;
  setShowShare: (v: boolean) => void;
  isViewer: boolean;
}

function AppHeader({
  myUser,
  activeCanvas,
  canvasPresenceList,
  unreadNotifications,
  unreadChatCount,
  showChat,
  setShowChat,
  setShowSettings,
  setShowShare,
  isViewer,
}: AppHeaderProps) {
  return (
    <header className="app-header">
      <div className="app-title">
        <span className="logo">●</span>
        Paint App
        {activeCanvas && (
          <span style={{ color: 'var(--text-muted)', fontWeight: 400 }}>
            {' '}
            / {activeCanvas.name}
          </span>
        )}
      </div>

      <div style={{ display: 'flex', alignItems: 'center', gap: '1rem' }}>
        {activeCanvas && (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '0.5rem',
              color: 'var(--text-muted)',
              fontSize: '0.875rem',
            }}
          >
            <span>👁</span>
            <span>{canvasPresenceList.length} viewing</span>
          </div>
        )}

        {isViewer && <span className="view-only-badge">View Only</span>}

        {activeCanvas && (
          <button
            className="btn btn-primary"
            onClick={() => setShowShare(true)}
            style={{ padding: '0.4rem 0.75rem' }}
          >
            Share
          </button>
        )}

        <button
          className="btn btn-icon"
          style={{ position: 'relative' }}
          onClick={() => setShowChat(!showChat)}
          data-tooltip="Chat"
        >
          💬
          {unreadChatCount > 0 && (
            <span className="notification-badge">{unreadChatCount}</span>
          )}
        </button>

        <button
          className="btn btn-icon"
          style={{ position: 'relative' }}
          data-tooltip="Notifications"
        >
          🔔
          {unreadNotifications > 0 && (
            <span className="notification-badge">{unreadNotifications}</span>
          )}
        </button>

        <div
          className="user-avatar"
          style={{
            background: myUser?.avatarColor || '#4cf490',
            cursor: 'pointer',
          }}
          onClick={() => setShowSettings(true)}
          data-tooltip="Settings"
        >
          {myUser?.displayName?.slice(0, 2).toUpperCase() || '??'}
        </div>
      </div>
    </header>
  );
}

// ============================================================================
// LEFT SIDEBAR
// ============================================================================

interface LeftSidebarProps {
  canvases: RowType[];
  activeCanvasId: bigint | null;
  setActiveCanvasId: (id: bigint | null) => void;
  conn: DbConnection | null;
  canvasMembers: RowType[];
}

function LeftSidebar({
  canvases,
  activeCanvasId,
  setActiveCanvasId,
  conn,
  canvasMembers,
}: LeftSidebarProps) {
  const [showNewCanvas, setShowNewCanvas] = useState(false);
  const [newCanvasName, setNewCanvasName] = useState('');

  const handleCreateCanvas = () => {
    if (!conn || !newCanvasName.trim()) return;
    conn.reducers.createCanvas({ name: newCanvasName.trim() });
    setNewCanvasName('');
    setShowNewCanvas(false);
  };

  const handleJoinCanvas = (canvasId: bigint) => {
    if (!conn) return;
    conn.reducers.joinCanvas({ canvasId });
    setActiveCanvasId(canvasId);
  };

  return (
    <div className="sidebar">
      <div className="sidebar-section">
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            marginBottom: '0.75rem',
          }}
        >
          <h3 style={{ margin: 0 }}>Canvases</h3>
          <button
            className="btn btn-icon"
            onClick={() => setShowNewCanvas(!showNewCanvas)}
            data-tooltip="New Canvas"
          >
            +
          </button>
        </div>

        {showNewCanvas && (
          <div style={{ marginBottom: '0.75rem' }}>
            <input
              type="text"
              className="form-input"
              placeholder="Canvas name..."
              value={newCanvasName}
              onChange={e => setNewCanvasName(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleCreateCanvas()}
              autoFocus
            />
            <div
              style={{ display: 'flex', gap: '0.5rem', marginTop: '0.5rem' }}
            >
              <button
                className="btn btn-primary"
                onClick={handleCreateCanvas}
                style={{ flex: 1 }}
              >
                Create
              </button>
              <button className="btn" onClick={() => setShowNewCanvas(false)}>
                Cancel
              </button>
            </div>
          </div>
        )}

        <div className="canvas-list">
          {canvases.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-text">No canvases yet</div>
            </div>
          ) : (
            canvases.map(canvas => {
              const members = canvasMembers.filter(
                m => m.canvasId === canvas.id
              );
              const memberCount = members.length;
              const lastActive = new Date(
                Number(canvas.lastActivityAt.microsSinceUnixEpoch / 1000n)
              );
              const daysAgo = Math.floor(
                (Date.now() - lastActive.getTime()) / (1000 * 60 * 60 * 24)
              );

              return (
                <div
                  key={canvas.id.toString()}
                  className={`canvas-item ${canvas.id === activeCanvasId ? 'active' : ''}`}
                  onClick={() => handleJoinCanvas(canvas.id)}
                >
                  <div
                    style={{
                      width: 32,
                      height: 32,
                      background: 'var(--bg-hover)',
                      borderRadius: 6,
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                    }}
                  >
                    🎨
                  </div>
                  <div className="canvas-item-info">
                    <div className="canvas-item-name">{canvas.name}</div>
                    <div className="canvas-item-meta">
                      {memberCount} member{memberCount !== 1 ? 's' : ''} •
                      {daysAgo === 0
                        ? ' Today'
                        : daysAgo === 1
                          ? ' Yesterday'
                          : ` ${daysAgo} days ago`}
                    </div>
                  </div>
                  {canvas.isPrivate && (
                    <span style={{ color: 'var(--text-muted)' }}>🔒</span>
                  )}
                </div>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// TOOLBAR
// ============================================================================

interface ToolbarProps {
  activeTool: Tool;
  setActiveTool: (t: Tool) => void;
  strokeColor: string;
  setStrokeColor: (c: string) => void;
  fillColor: string;
  setFillColor: (c: string) => void;
  brushSize: number;
  setBrushSize: (s: number) => void;
  isViewer: boolean;
  conn: DbConnection | null;
  activeCanvasId: bigint | null;
}

function Toolbar({
  activeTool,
  setActiveTool,
  strokeColor,
  setStrokeColor,
  fillColor,
  setFillColor,
  brushSize,
  setBrushSize,
  isViewer,
  conn,
  activeCanvasId,
}: ToolbarProps) {
  const tools: { id: Tool; icon: string; label: string; shortcut: string }[] = [
    { id: 'select', icon: '⬚', label: 'Select', shortcut: 'V' },
    { id: 'brush', icon: '🖌', label: 'Brush', shortcut: 'B' },
    { id: 'eraser', icon: '🧹', label: 'Eraser', shortcut: 'E' },
    { id: 'rectangle', icon: '▢', label: 'Rectangle', shortcut: 'R' },
    { id: 'ellipse', icon: '○', label: 'Ellipse', shortcut: 'O' },
    { id: 'line', icon: '/', label: 'Line', shortcut: 'L' },
    { id: 'arrow', icon: '➝', label: 'Arrow', shortcut: 'A' },
    { id: 'text', icon: 'T', label: 'Text', shortcut: 'T' },
    { id: 'sticky', icon: '📝', label: 'Sticky', shortcut: 'S' },
    { id: 'comment', icon: '💬', label: 'Comment', shortcut: 'C' },
  ];

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement
      )
        return;

      // Undo: Ctrl+Z
      if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !e.shiftKey) {
        e.preventDefault();
        if (conn && activeCanvasId && !isViewer) {
          conn.reducers.undo({ canvasId: activeCanvasId });
        }
        return;
      }

      // Redo: Ctrl+Y or Ctrl+Shift+Z
      if (
        (e.ctrlKey || e.metaKey) &&
        (e.key === 'y' || (e.key === 'z' && e.shiftKey))
      ) {
        e.preventDefault();
        if (conn && activeCanvasId && !isViewer) {
          conn.reducers.redo({ canvasId: activeCanvasId });
        }
        return;
      }

      const shortcuts: Record<string, Tool> = {
        v: 'select',
        b: 'brush',
        e: 'eraser',
        r: 'rectangle',
        o: 'ellipse',
        l: 'line',
        a: 'arrow',
        t: 'text',
        s: 'sticky',
        c: 'comment',
      };

      const tool = shortcuts[e.key.toLowerCase()];
      if (tool && !isViewer) setActiveTool(tool);

      if (e.key === 'Delete' || e.key === 'Backspace') {
        if (conn && activeCanvasId && !isViewer) {
          conn.reducers.deleteSelected({ canvasId: activeCanvasId });
        }
      }

      if (e.key === 'Escape') {
        if (conn && activeCanvasId) {
          conn.reducers.clearSelection({ canvasId: activeCanvasId });
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [conn, activeCanvasId, isViewer, setActiveTool]);

  const handleClearCanvas = () => {
    if (!conn || !activeCanvasId || isViewer) return;
    if (confirm('Clear all content from this canvas?')) {
      conn.reducers.clearCanvas({ canvasId: activeCanvasId });
    }
  };

  return (
    <div className="toolbar">
      <div className="toolbar-group">
        {tools.map(tool => (
          <button
            key={tool.id}
            className={`tool-btn ${activeTool === tool.id ? 'active' : ''}`}
            onClick={() => setActiveTool(tool.id)}
            data-tooltip={`${tool.label} (${tool.shortcut})`}
            disabled={isViewer && tool.id !== 'select' && tool.id !== 'comment'}
          >
            {tool.icon}
          </button>
        ))}
      </div>

      <div className="toolbar-group">
        <div className="color-picker">
          <label style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}>
            Stroke
          </label>
          <input
            type="color"
            value={strokeColor}
            onChange={e => setStrokeColor(e.target.value)}
            disabled={isViewer}
          />
        </div>
        <div className="color-picker">
          <label style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}>
            Fill
          </label>
          <input
            type="color"
            value={fillColor === 'transparent' ? '#000000' : fillColor}
            onChange={e => setFillColor(e.target.value)}
            disabled={isViewer}
          />
          <button
            className="btn btn-icon"
            onClick={() => setFillColor('transparent')}
            style={{
              width: 24,
              height: 24,
              fontSize: '0.75rem',
              opacity: fillColor === 'transparent' ? 1 : 0.5,
            }}
            data-tooltip="No fill"
            disabled={isViewer}
          >
            ∅
          </button>
        </div>
      </div>

      <div className="toolbar-group">
        <label style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}>
          Size
        </label>
        <input
          type="range"
          min="1"
          max="50"
          value={brushSize}
          onChange={e => setBrushSize(Number(e.target.value))}
          style={{ width: 80 }}
          disabled={isViewer}
        />
        <span
          style={{ fontSize: '0.75rem', color: 'var(--text-muted)', width: 24 }}
        >
          {brushSize}
        </span>
      </div>

      <div className="toolbar-group" style={{ marginLeft: 'auto' }}>
        <button
          className="btn btn-danger"
          onClick={handleClearCanvas}
          disabled={isViewer}
        >
          Clear
        </button>
      </div>
    </div>
  );
}

// ============================================================================
// DRAWING CANVAS
// ============================================================================

interface DrawingCanvasProps {
  conn: DbConnection | null;
  canvasId: bigint;
  layerId: bigint | null;
  layers: RowType[];
  strokes: RowType[];
  shapes: RowType[];
  textElements: RowType[];
  cursors: RowType[];
  selections: RowType[];
  comments: RowType[];
  users: RowType[];
  activeTool: Tool;
  strokeColor: string;
  fillColor: string;
  brushSize: number;
  isViewer: boolean;
  myIdentity: Identity | null;
}

function DrawingCanvas({
  conn,
  canvasId,
  layerId,
  layers,
  strokes,
  shapes,
  textElements,
  cursors,
  selections,
  comments,
  users,
  activeTool,
  strokeColor,
  fillColor,
  brushSize,
  isViewer,
  myIdentity,
}: DrawingCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [isDrawing, setIsDrawing] = useState(false);
  const [currentPoints, setCurrentPoints] = useState<Point[]>([]);
  const [shapeStart, setShapeStart] = useState<Point | null>(null);
  const [shapeEnd, setShapeEnd] = useState<Point | null>(null);
  const [shapePreview, setShapePreview] = useState<{
    x: number;
    y: number;
    width: number;
    height: number;
  } | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState<Point | null>(null);
  const [dragOffset, setDragOffset] = useState<Point | null>(null);

  // Canvas size - use ResizeObserver to detect container size changes (including panel toggle)
  const [canvasSize, setCanvasSize] = useState({ width: 800, height: 600 });

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const updateSize = () => {
      setCanvasSize({
        width: container.clientWidth,
        height: container.clientHeight,
      });
    };

    // Use ResizeObserver to detect container size changes
    const resizeObserver = new ResizeObserver(updateSize);
    resizeObserver.observe(container);
    updateSize(); // Initial size

    return () => resizeObserver.disconnect();
  }, []);

  // Draw everything
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // Draw by layer order
    for (const layer of layers) {
      if (!layer.visible) continue;

      ctx.globalAlpha = layer.opacity;

      // Get all elements on this layer
      const layerStrokes = strokes.filter(s => s.layerId === layer.id);
      const layerShapes = shapes.filter(s => s.layerId === layer.id);
      const layerTexts = textElements.filter(t => t.layerId === layer.id);

      // Combine and sort by creation time for proper z-order
      type DrawElement =
        | { type: 'stroke'; data: (typeof layerStrokes)[0]; time: bigint }
        | { type: 'shape'; data: (typeof layerShapes)[0]; time: bigint }
        | { type: 'text'; data: (typeof layerTexts)[0]; time: bigint };

      const allElements: DrawElement[] = [
        ...layerStrokes.map(s => ({
          type: 'stroke' as const,
          data: s,
          time: s.createdAt.microsSinceUnixEpoch,
        })),
        ...layerShapes.map(s => ({
          type: 'shape' as const,
          data: s,
          time: s.createdAt.microsSinceUnixEpoch,
        })),
        ...layerTexts.map(t => ({
          type: 'text' as const,
          data: t,
          time: t.createdAt.microsSinceUnixEpoch,
        })),
      ].sort((a, b) => Number(a.time - b.time));

      for (const element of allElements) {
        if (element.type === 'stroke') {
          const stroke = element.data;
          const points: Point[] = JSON.parse(stroke.points);
          if (points.length < 2) continue;

          ctx.beginPath();
          ctx.strokeStyle = stroke.tool === 'eraser' ? '#141416' : stroke.color;
          ctx.lineWidth = stroke.brushSize;
          ctx.lineCap = 'round';
          ctx.lineJoin = 'round';

          ctx.moveTo(points[0].x, points[0].y);
          for (let i = 1; i < points.length; i++) {
            ctx.lineTo(points[i].x, points[i].y);
          }
          ctx.stroke();
        } else if (element.type === 'shape') {
          const shape = element.data;
          ctx.beginPath();
          ctx.strokeStyle = shape.strokeColor;
          ctx.fillStyle = shape.fillColor || 'transparent';
          ctx.lineWidth = shape.strokeWidth;

          if (shape.shapeType === 'rectangle') {
            if (shape.fillColor && shape.fillColor !== 'transparent') {
              ctx.fillRect(shape.x, shape.y, shape.width, shape.height);
            }
            ctx.strokeRect(shape.x, shape.y, shape.width, shape.height);
          } else if (shape.shapeType === 'ellipse') {
            ctx.ellipse(
              shape.x + shape.width / 2,
              shape.y + shape.height / 2,
              Math.abs(shape.width / 2),
              Math.abs(shape.height / 2),
              0,
              0,
              Math.PI * 2
            );
            if (shape.fillColor && shape.fillColor !== 'transparent')
              ctx.fill();
            ctx.stroke();
          } else if (
            shape.shapeType === 'line' ||
            shape.shapeType === 'arrow'
          ) {
            ctx.moveTo(shape.x, shape.y);
            ctx.lineTo(shape.x + shape.width, shape.y + shape.height);
            ctx.stroke();

            if (shape.shapeType === 'arrow') {
              // Draw arrowhead
              const angle = Math.atan2(shape.height, shape.width);
              const headLen = 15;
              ctx.beginPath();
              ctx.moveTo(shape.x + shape.width, shape.y + shape.height);
              ctx.lineTo(
                shape.x + shape.width - headLen * Math.cos(angle - Math.PI / 6),
                shape.y + shape.height - headLen * Math.sin(angle - Math.PI / 6)
              );
              ctx.moveTo(shape.x + shape.width, shape.y + shape.height);
              ctx.lineTo(
                shape.x + shape.width - headLen * Math.cos(angle + Math.PI / 6),
                shape.y + shape.height - headLen * Math.sin(angle + Math.PI / 6)
              );
              ctx.stroke();
            }
          }
        } else if (element.type === 'text') {
          const text = element.data;
          if (text.backgroundColor) {
            ctx.fillStyle = text.backgroundColor;
            ctx.fillRect(text.x, text.y, text.width, text.height);
          }

          ctx.fillStyle = text.textColor;
          const sizes: Record<string, number> = {
            small: 12,
            medium: 16,
            large: 24,
            'x-large': 32,
          };
          ctx.font = `${sizes[text.fontSize] || 16}px ${text.fontFamily}`;
          ctx.fillText(
            text.content,
            text.x + 4,
            text.y + (sizes[text.fontSize] || 16) + 4
          );
        }
      }

      ctx.globalAlpha = 1;
    }

    // Draw current stroke preview
    if (currentPoints.length > 1) {
      ctx.beginPath();
      ctx.strokeStyle = activeTool === 'eraser' ? '#141416' : strokeColor;
      ctx.lineWidth = brushSize;
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';

      ctx.moveTo(currentPoints[0].x, currentPoints[0].y);
      for (let i = 1; i < currentPoints.length; i++) {
        ctx.lineTo(currentPoints[i].x, currentPoints[i].y);
      }
      ctx.stroke();
    }

    // Draw shape preview
    if (shapePreview && shapeStart) {
      ctx.beginPath();
      ctx.strokeStyle = strokeColor;
      ctx.fillStyle = fillColor === 'transparent' ? 'transparent' : fillColor;
      ctx.lineWidth = brushSize;
      ctx.setLineDash([5, 5]);

      if (activeTool === 'rectangle') {
        if (fillColor !== 'transparent')
          ctx.fillRect(
            shapePreview.x,
            shapePreview.y,
            shapePreview.width,
            shapePreview.height
          );
        ctx.strokeRect(
          shapePreview.x,
          shapePreview.y,
          shapePreview.width,
          shapePreview.height
        );
      } else if (activeTool === 'ellipse') {
        ctx.ellipse(
          shapePreview.x + shapePreview.width / 2,
          shapePreview.y + shapePreview.height / 2,
          Math.abs(shapePreview.width / 2),
          Math.abs(shapePreview.height / 2),
          0,
          0,
          Math.PI * 2
        );
        if (fillColor !== 'transparent') ctx.fill();
        ctx.stroke();
      } else if (activeTool === 'line' || activeTool === 'arrow') {
        // Use shapeEnd for correct direction (shapePreview is normalized)
        const endX = shapeEnd ? shapeEnd.x : shapeStart.x + shapePreview.width;
        const endY = shapeEnd ? shapeEnd.y : shapeStart.y + shapePreview.height;
        ctx.moveTo(shapeStart.x, shapeStart.y);
        ctx.lineTo(endX, endY);
        ctx.stroke();

        if (activeTool === 'arrow') {
          const angle = Math.atan2(endY - shapeStart.y, endX - shapeStart.x);
          const headLen = 15;
          ctx.beginPath();
          ctx.moveTo(endX, endY);
          ctx.lineTo(
            endX - headLen * Math.cos(angle - Math.PI / 6),
            endY - headLen * Math.sin(angle - Math.PI / 6)
          );
          ctx.moveTo(endX, endY);
          ctx.lineTo(
            endX - headLen * Math.cos(angle + Math.PI / 6),
            endY - headLen * Math.sin(angle + Math.PI / 6)
          );
          ctx.stroke();
        }
      }

      ctx.setLineDash([]);
    }

    // Draw selections with drag preview
    for (const sel of selections) {
      const user = users.find(
        u => u.identity.toHexString() === sel.userIdentity.toHexString()
      );
      const isMine =
        sel.userIdentity.toHexString() === myIdentity?.toHexString();
      const color = isMine ? '#4cf490' : user?.avatarColor || '#a880ff';

      let bounds: { x: number; y: number; w: number; h: number } | null = null;

      if (sel.elementType === 'shape') {
        const shape = shapes.find(s => s.id === sel.elementId);
        if (shape)
          bounds = { x: shape.x, y: shape.y, w: shape.width, h: shape.height };
      } else if (sel.elementType === 'text') {
        const text = textElements.find(t => t.id === sel.elementId);
        if (text)
          bounds = { x: text.x, y: text.y, w: text.width, h: text.height };
      }

      if (bounds) {
        // Apply drag offset for my selections
        const offsetX = isMine && dragOffset ? dragOffset.x : 0;
        const offsetY = isMine && dragOffset ? dragOffset.y : 0;

        ctx.strokeStyle = color;
        ctx.lineWidth = 2;
        ctx.setLineDash([5, 5]);
        ctx.strokeRect(
          bounds.x + offsetX - 4,
          bounds.y + offsetY - 4,
          bounds.w + 8,
          bounds.h + 8
        );
        ctx.setLineDash([]);

        // Draw move handles for my selections
        if (isMine) {
          ctx.fillStyle = color;
          const handleSize = 8;
          // Corner handles
          ctx.fillRect(
            bounds.x + offsetX - handleSize / 2 - 4,
            bounds.y + offsetY - handleSize / 2 - 4,
            handleSize,
            handleSize
          );
          ctx.fillRect(
            bounds.x + offsetX + bounds.w - handleSize / 2 + 4,
            bounds.y + offsetY - handleSize / 2 - 4,
            handleSize,
            handleSize
          );
          ctx.fillRect(
            bounds.x + offsetX - handleSize / 2 - 4,
            bounds.y + offsetY + bounds.h - handleSize / 2 + 4,
            handleSize,
            handleSize
          );
          ctx.fillRect(
            bounds.x + offsetX + bounds.w - handleSize / 2 + 4,
            bounds.y + offsetY + bounds.h - handleSize / 2 + 4,
            handleSize,
            handleSize
          );
        }
      }
    }
  }, [
    layers,
    strokes,
    shapes,
    textElements,
    selections,
    currentPoints,
    shapePreview,
    shapeStart,
    activeTool,
    strokeColor,
    fillColor,
    brushSize,
    users,
    myIdentity,
    dragOffset,
  ]);

  // Mouse handlers
  const getMousePos = (e: React.MouseEvent): Point => {
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return { x: 0, y: 0 };
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  };

  const handleMouseDown = (e: React.MouseEvent) => {
    if (isViewer && activeTool !== 'select' && activeTool !== 'comment') return;
    if (!layerId) return;

    const pos = getMousePos(e);

    if (activeTool === 'brush' || activeTool === 'eraser') {
      setIsDrawing(true);
      setCurrentPoints([pos]);
    } else if (
      ['rectangle', 'ellipse', 'line', 'arrow', 'text', 'sticky'].includes(
        activeTool
      )
    ) {
      setShapeStart(pos);
      setShapePreview({ x: pos.x, y: pos.y, width: 0, height: 0 });
    } else if (activeTool === 'comment') {
      if (conn) {
        const content = prompt('Enter comment:');
        if (content) {
          conn.reducers.addComment({ canvasId, x: pos.x, y: pos.y, content });
        }
      }
    } else if (activeTool === 'select') {
      // Check if clicking on a shape or text
      let foundElement: {
        type: string;
        id: bigint;
        x: number;
        y: number;
      } | null = null;

      for (const shape of shapes) {
        // For lines, use a larger hit area
        const isLine =
          shape.shapeType === 'line' || shape.shapeType === 'arrow';
        let hitTest = false;
        if (isLine) {
          // Distance from point to line segment
          const x1 = shape.x,
            y1 = shape.y;
          const x2 = shape.x + shape.width,
            y2 = shape.y + shape.height;
          const dist =
            Math.abs(
              (y2 - y1) * pos.x - (x2 - x1) * pos.y + x2 * y1 - y2 * x1
            ) / Math.sqrt((y2 - y1) ** 2 + (x2 - x1) ** 2);
          hitTest = dist < 10;
        } else {
          hitTest =
            pos.x >= shape.x &&
            pos.x <= shape.x + shape.width &&
            pos.y >= shape.y &&
            pos.y <= shape.y + shape.height;
        }
        if (hitTest) {
          foundElement = {
            type: 'shape',
            id: shape.id,
            x: shape.x,
            y: shape.y,
          };
          break;
        }
      }

      if (!foundElement) {
        for (const text of textElements) {
          if (
            pos.x >= text.x &&
            pos.x <= text.x + text.width &&
            pos.y >= text.y &&
            pos.y <= text.y + text.height
          ) {
            foundElement = { type: 'text', id: text.id, x: text.x, y: text.y };
            break;
          }
        }
      }

      if (foundElement && conn) {
        // Check if this element is already selected
        const isAlreadySelected = selections.some(
          s =>
            s.elementType === foundElement!.type &&
            s.elementId === foundElement!.id
        );

        if (isAlreadySelected && !isViewer) {
          // Start dragging
          setIsDragging(true);
          setDragStart(pos);
        } else {
          // Select the element
          conn.reducers.selectElement({
            canvasId,
            elementType: foundElement.type,
            elementId: foundElement.id,
            addToSelection: e.shiftKey,
          });
          // Also start dragging
          if (!isViewer) {
            setIsDragging(true);
            setDragStart(pos);
          }
        }
      } else if (conn) {
        conn.reducers.clearSelection({ canvasId });
      }
    }
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    const pos = getMousePos(e);

    // Update cursor position
    if (conn) {
      conn.reducers.updateCursor({
        canvasId,
        x: pos.x,
        y: pos.y,
        tool: activeTool,
        color: strokeColor,
      });
    }

    // Handle dragging selected elements
    if (isDragging && dragStart) {
      setDragOffset({
        x: pos.x - dragStart.x,
        y: pos.y - dragStart.y,
      });
    }

    if (isDrawing && (activeTool === 'brush' || activeTool === 'eraser')) {
      setCurrentPoints(prev => [...prev, pos]);
    }

    if (shapeStart) {
      let width = pos.x - shapeStart.x;
      let height = pos.y - shapeStart.y;

      // Shift for perfect squares/circles
      if (
        e.shiftKey &&
        (activeTool === 'rectangle' || activeTool === 'ellipse')
      ) {
        const size = Math.max(Math.abs(width), Math.abs(height));
        width = width >= 0 ? size : -size;
        height = height >= 0 ? size : -size;
      }

      // Track end position for lines
      setShapeEnd({ x: shapeStart.x + width, y: shapeStart.y + height });

      setShapePreview({
        x: width >= 0 ? shapeStart.x : shapeStart.x + width,
        y: height >= 0 ? shapeStart.y : shapeStart.y + height,
        width: Math.abs(width),
        height: Math.abs(height),
      });
    }
  };

  const handleMouseUp = () => {
    // Handle drag move
    if (
      isDragging &&
      dragOffset &&
      conn &&
      (dragOffset.x !== 0 || dragOffset.y !== 0)
    ) {
      // Move all selected elements
      for (const sel of selections) {
        if (sel.elementType === 'shape') {
          const shape = shapes.find(s => s.id === sel.elementId);
          if (shape) {
            conn.reducers.updateShape({
              shapeId: shape.id,
              x: shape.x + dragOffset.x,
              y: shape.y + dragOffset.y,
              width: shape.width,
              height: shape.height,
              rotation: shape.rotation,
            });
          }
        } else if (sel.elementType === 'text') {
          const text = textElements.find(t => t.id === sel.elementId);
          if (text) {
            conn.reducers.updateTextElement({
              textId: text.id,
              x: text.x + dragOffset.x,
              y: text.y + dragOffset.y,
              width: text.width,
              height: text.height,
              rotation: text.rotation,
              content: text.content,
            });
          }
        }
      }
    }

    // Reset drag state
    setIsDragging(false);
    setDragStart(null);
    setDragOffset(null);

    if (isDrawing && currentPoints.length > 1 && conn && layerId) {
      conn.reducers.addStroke({
        canvasId,
        layerId,
        tool: activeTool === 'eraser' ? 'eraser' : 'brush',
        color: strokeColor,
        brushSize,
        points: JSON.stringify(currentPoints),
      });
    }

    if (shapeStart && shapePreview && conn && layerId) {
      if (shapePreview.width > 5 || shapePreview.height > 5) {
        if (['rectangle', 'ellipse', 'line', 'arrow'].includes(activeTool)) {
          // For lines/arrows, store start point and delta to end
          const isLine = activeTool === 'line' || activeTool === 'arrow';
          conn.reducers.addShape({
            canvasId,
            layerId,
            shapeType: activeTool,
            x: isLine ? shapeStart.x : shapePreview.x,
            y: isLine ? shapeStart.y : shapePreview.y,
            width:
              isLine && shapeEnd
                ? shapeEnd.x - shapeStart.x
                : shapePreview.width,
            height:
              isLine && shapeEnd
                ? shapeEnd.y - shapeStart.y
                : shapePreview.height,
            strokeColor,
            fillColor: fillColor === 'transparent' ? '' : fillColor,
            strokeWidth: brushSize,
          });
        } else if (activeTool === 'text' || activeTool === 'sticky') {
          const content = prompt('Enter text:');
          if (content) {
            conn.reducers.addTextElement({
              canvasId,
              layerId,
              elementType: activeTool,
              x: shapePreview.x,
              y: shapePreview.y,
              width: Math.max(shapePreview.width, 100),
              height: Math.max(shapePreview.height, 40),
              content,
              fontFamily: 'sans-serif',
              fontSize: 'medium',
              textColor: strokeColor,
              backgroundColor: activeTool === 'sticky' ? '#fbdc8e' : undefined,
            });
          }
        }
      }
    }

    setIsDrawing(false);
    setCurrentPoints([]);
    setShapeStart(null);
    setShapeEnd(null);
    setShapePreview(null);
  };

  return (
    <div className="canvas-container" ref={containerRef}>
      <canvas
        ref={canvasRef}
        className="drawing-canvas"
        width={canvasSize.width}
        height={canvasSize.height}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
      />

      {/* Remote cursors */}
      {cursors
        .filter(c => c.userIdentity.toHexString() !== myIdentity?.toHexString())
        .map(cursor => {
          const user = users.find(
            u => u.identity.toHexString() === cursor.userIdentity.toHexString()
          );
          return (
            <div
              key={cursor.id.toString()}
              className="remote-cursor"
              style={{ transform: `translate(${cursor.x}px, ${cursor.y}px)` }}
            >
              <svg
                className="cursor-pointer"
                viewBox="0 0 24 24"
                fill={user?.avatarColor || '#a880ff'}
              >
                <path d="M5 2l14 14-6 2-2 6L5 2z" />
              </svg>
              <div
                className="cursor-label"
                style={{ background: user?.avatarColor || '#a880ff' }}
              >
                {user?.displayName || 'Unknown'} • {cursor.tool}
              </div>
              <div
                className="color-swatch"
                style={{
                  position: 'absolute',
                  top: 20,
                  left: -10,
                  width: 12,
                  height: 12,
                  background: cursor.color,
                  borderRadius: 2,
                }}
              />
            </div>
          );
        })}

      {/* Comment pins */}
      {comments.map(comment => {
        const user = users.find(
          u => u.identity.toHexString() === comment.authorIdentity.toHexString()
        );
        return (
          <div
            key={comment.id.toString()}
            className={`comment-pin ${comment.resolved ? 'resolved' : ''}`}
            style={{ left: comment.x - 14, top: comment.y - 28 }}
            title={`${user?.displayName}: ${comment.content}`}
          >
            <span className="comment-pin-content">💬</span>
          </div>
        );
      })}
    </div>
  );
}

// ============================================================================
// LAYERS PANEL
// ============================================================================

interface LayersPanelProps {
  layers: RowType[];
  activeLayerId: bigint | null;
  setActiveLayerId: (id: bigint) => void;
  conn: DbConnection | null;
  canvasId: bigint;
  users: RowType[];
  isViewer: boolean;
}

function LayersPanel({
  layers,
  activeLayerId,
  setActiveLayerId,
  conn,
  canvasId,
  users,
  isViewer,
}: LayersPanelProps) {
  const handleAddLayer = () => {
    if (!conn || isViewer) return;
    conn.reducers.createLayer({ canvasId, name: `Layer ${layers.length + 1}` });
  };

  const handleToggleVisibility = (layerId: bigint) => {
    if (!conn || isViewer) return;
    conn.reducers.toggleLayerVisibility({ layerId });
  };

  const handleToggleLock = (layer: RowType) => {
    if (!conn || isViewer) return;
    if (layer.lockedBy) {
      conn.reducers.unlockLayer({ layerId: layer.id });
    } else {
      conn.reducers.lockLayer({ layerId: layer.id });
    }
  };

  return (
    <div className="layers-floating-panel">
      <div className="sidebar-section" style={{ padding: '0.75rem' }}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            marginBottom: '0.5rem',
          }}
        >
          <h3 style={{ margin: 0, fontSize: '0.8rem' }}>Layers</h3>
          <button
            className="btn btn-icon"
            style={{ width: 28, height: 28 }}
            onClick={handleAddLayer}
            disabled={isViewer}
            data-tooltip="Add Layer"
          >
            +
          </button>
        </div>

        <div className="layers-panel">
          {layers.map(layer => {
            const lockedByUser = layer.lockedBy
              ? users.find(
                  u =>
                    u.identity.toHexString() === layer.lockedBy?.toHexString()
                )
              : null;

            return (
              <div
                key={layer.id.toString()}
                className={`layer-item ${layer.id === activeLayerId ? 'active' : ''} ${layer.lockedBy ? 'locked' : ''}`}
                onClick={() => setActiveLayerId(layer.id)}
              >
                <span className="layer-name">{layer.name}</span>
                <div className="layer-actions">
                  <button
                    className={`layer-btn ${layer.visible ? 'active' : ''}`}
                    onClick={e => {
                      e.stopPropagation();
                      handleToggleVisibility(layer.id);
                    }}
                    title={layer.visible ? 'Hide' : 'Show'}
                  >
                    {layer.visible ? '👁' : '👁‍🗨'}
                  </button>
                  <button
                    className={`layer-btn ${layer.lockedBy ? 'active' : ''}`}
                    onClick={e => {
                      e.stopPropagation();
                      handleToggleLock(layer);
                    }}
                    title={
                      lockedByUser
                        ? `Locked by ${lockedByUser.displayName}`
                        : 'Lock'
                    }
                  >
                    {layer.lockedBy ? '🔒' : '🔓'}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// RIGHT PANEL
// ============================================================================

interface RightPanelProps {
  activeTab: 'users' | 'activity' | 'versions' | 'comments';
  setActiveTab: (t: 'users' | 'activity' | 'versions' | 'comments') => void;
  presences: RowType[];
  users: RowType[];
  activities: RowType[];
  versions: RowType[];
  comments: RowType[];
  commentReplies: RowType[];
  conn: DbConnection | null;
  canvasId: bigint | null;
  myIdentity: Identity | null;
}

function RightPanel({
  activeTab,
  setActiveTab,
  presences,
  users,
  activities,
  versions,
  comments,
  commentReplies,
  conn,
  canvasId,
  myIdentity,
}: RightPanelProps) {
  const handleSaveVersion = () => {
    if (!conn || !canvasId) return;
    const name = prompt('Version name (optional):');
    conn.reducers.saveVersion({
      canvasId,
      name: name || undefined,
      description: undefined,
    });
  };

  const handleRestoreVersion = (versionId: bigint) => {
    if (
      !conn ||
      !confirm('Restore this version? Current changes will be saved first.')
    )
      return;
    conn.reducers.restoreVersion({ versionId });
  };

  const handleFollowUser = (targetIdentity: Identity) => {
    if (!conn || !canvasId) return;
    conn.reducers.followUser({ canvasId, targetIdentity });
  };

  const formatTime = (timestamp: { microsSinceUnixEpoch: bigint }) => {
    const date = new Date(Number(timestamp.microsSinceUnixEpoch / 1000n));
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  const unresolvedComments = comments.filter(c => !c.resolved);

  return (
    <div className="right-panel">
      <div className="right-panel-tabs">
        <button
          className={`right-panel-tab ${activeTab === 'users' ? 'active' : ''}`}
          onClick={() => setActiveTab('users')}
        >
          Users ({presences.length})
        </button>
        <button
          className={`right-panel-tab ${activeTab === 'activity' ? 'active' : ''}`}
          onClick={() => setActiveTab('activity')}
        >
          Activity
        </button>
        <button
          className={`right-panel-tab ${activeTab === 'versions' ? 'active' : ''}`}
          onClick={() => setActiveTab('versions')}
        >
          History
        </button>
        <button
          className={`right-panel-tab ${activeTab === 'comments' ? 'active' : ''}`}
          onClick={() => setActiveTab('comments')}
          style={{ position: 'relative' }}
        >
          Comments
          {unresolvedComments.length > 0 && (
            <span
              className="notification-badge"
              style={{ position: 'static', marginLeft: 4 }}
            >
              {unresolvedComments.length}
            </span>
          )}
        </button>
      </div>

      <div className="right-panel-content">
        {activeTab === 'users' && (
          <div className="user-list">
            {presences.map(presence => {
              const user = users.find(
                u =>
                  u.identity.toHexString() ===
                  presence.userIdentity.toHexString()
              );
              const isMe =
                presence.userIdentity.toHexString() ===
                myIdentity?.toHexString();

              return (
                <div key={presence.id.toString()} className="user-item">
                  <div
                    className="user-avatar"
                    style={{ background: user?.avatarColor || '#4cf490' }}
                  >
                    {user?.displayName?.slice(0, 2).toUpperCase() || '??'}
                  </div>
                  <div className="user-info">
                    <div className="user-name">
                      {user?.displayName || 'Unknown'} {isMe && '(you)'}
                    </div>
                    <div className="user-status">
                      <span className={`status-dot ${presence.status}`} />
                      {presence.status} • {presence.currentTool}
                    </div>
                  </div>
                  {!isMe && (
                    <button
                      className="btn btn-icon"
                      onClick={() => handleFollowUser(presence.userIdentity)}
                      title="Follow"
                      style={{ width: 28, height: 28, fontSize: '0.75rem' }}
                    >
                      👁
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        )}

        {activeTab === 'activity' && (
          <div className="activity-feed">
            {activities.length === 0 ? (
              <div className="empty-state">
                <div className="empty-state-text">No activity yet</div>
              </div>
            ) : (
              activities.map(activity => {
                const user = users.find(
                  u =>
                    u.identity.toHexString() ===
                    activity.userIdentity.toHexString()
                );
                return (
                  <div key={activity.id.toString()} className="activity-item">
                    <div
                      className="activity-dot"
                      style={{ background: user?.avatarColor || '#4cf490' }}
                    />
                    <div className="activity-content">
                      <div className="activity-text">
                        <strong>{user?.displayName || 'Someone'}</strong>{' '}
                        {activity.action.replace(/_/g, ' ')}
                      </div>
                      <div className="activity-time">
                        {formatTime(activity.createdAt)}
                      </div>
                    </div>
                  </div>
                );
              })
            )}
          </div>
        )}

        {activeTab === 'versions' && (
          <div>
            <button
              className="btn btn-primary"
              onClick={handleSaveVersion}
              style={{ width: '100%', marginBottom: '0.75rem' }}
            >
              Save Version
            </button>
            <div className="version-list">
              {versions.length === 0 ? (
                <div className="empty-state">
                  <div className="empty-state-text">No versions saved</div>
                </div>
              ) : (
                versions.map(version => {
                  const user = version.createdBy
                    ? users.find(
                        u =>
                          u.identity.toHexString() ===
                          version.createdBy?.toHexString()
                      )
                    : null;
                  return (
                    <div
                      key={version.id.toString()}
                      className="version-item"
                      onClick={() => handleRestoreVersion(version.id)}
                    >
                      <div className="version-name">
                        {version.name || 'Untitled'}
                        {version.isAutoSave && (
                          <span className="version-auto">Auto</span>
                        )}
                      </div>
                      <div className="version-meta">
                        {formatTime(version.createdAt)} •{' '}
                        {user?.displayName || 'System'}
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </div>
        )}

        {activeTab === 'comments' && (
          <div className="activity-feed">
            {comments.length === 0 ? (
              <div className="empty-state">
                <div className="empty-state-text">No comments yet</div>
              </div>
            ) : (
              comments.map(comment => {
                const user = users.find(
                  u =>
                    u.identity.toHexString() ===
                    comment.authorIdentity.toHexString()
                );
                const replies = commentReplies.filter(
                  r => r.commentId === comment.id
                );

                return (
                  <div
                    key={comment.id.toString()}
                    className="activity-item"
                    style={{ opacity: comment.resolved ? 0.5 : 1 }}
                  >
                    <div
                      className="activity-dot"
                      style={{ background: user?.avatarColor || '#fbdc8e' }}
                    />
                    <div className="activity-content">
                      <div className="activity-text">
                        <strong>{user?.displayName || 'Someone'}</strong>:{' '}
                        {comment.content}
                        {comment.resolved && (
                          <span
                            style={{
                              color: 'var(--stdb-green)',
                              marginLeft: '0.5rem',
                            }}
                          >
                            ✓ Resolved
                          </span>
                        )}
                      </div>
                      <div className="activity-time">
                        {formatTime(comment.createdAt)}
                      </div>
                      {replies.length > 0 && (
                        <div
                          style={{
                            marginTop: '0.5rem',
                            paddingLeft: '0.5rem',
                            borderLeft: '2px solid var(--border-color)',
                          }}
                        >
                          {replies.map(reply => {
                            const replyUser = users.find(
                              u =>
                                u.identity.toHexString() ===
                                reply.authorIdentity.toHexString()
                            );
                            return (
                              <div
                                key={reply.id.toString()}
                                style={{
                                  marginTop: '0.25rem',
                                  fontSize: '0.75rem',
                                }}
                              >
                                <strong>
                                  {replyUser?.displayName || 'Someone'}
                                </strong>
                                : {reply.content}
                              </div>
                            );
                          })}
                        </div>
                      )}
                      {!comment.resolved && conn && (
                        <button
                          className="btn"
                          style={{
                            marginTop: '0.5rem',
                            padding: '0.25rem 0.5rem',
                            fontSize: '0.75rem',
                          }}
                          onClick={() =>
                            conn.reducers.resolveComment({
                              commentId: comment.id,
                            })
                          }
                        >
                          Resolve
                        </button>
                      )}
                    </div>
                  </div>
                );
              })
            )}
          </div>
        )}
      </div>
    </div>
  );
}

// ============================================================================
// CHAT PANEL
// ============================================================================

interface ChatPanelProps {
  messages: RowType[];
  users: RowType[];
  typingIndicators: RowType[];
  conn: DbConnection | null;
  canvasId: bigint;
  myIdentity: Identity | null;
  onClose: () => void;
}

function ChatPanel({
  messages,
  users,
  typingIndicators,
  conn,
  canvasId,
  myIdentity,
  onClose,
}: ChatPanelProps) {
  const [message, setMessage] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const handleSend = () => {
    if (!conn || !message.trim()) return;
    conn.reducers.sendChatMessage({ canvasId, content: message.trim() });
    setMessage('');
  };

  const handleTyping = (typing: boolean) => {
    if (!conn) return;
    conn.reducers.setTyping({ canvasId, isTyping: typing });
  };

  const formatTime = (timestamp: { microsSinceUnixEpoch: bigint }) => {
    const date = new Date(Number(timestamp.microsSinceUnixEpoch / 1000n));
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  const typingUsers = typingIndicators
    .filter(t => t.userIdentity.toHexString() !== myIdentity?.toHexString())
    .map(
      t =>
        users.find(
          u => u.identity.toHexString() === t.userIdentity.toHexString()
        )?.displayName || 'Someone'
    );

  return (
    <div className="chat-panel open">
      <div className="panel-header">
        <span className="panel-title">Chat</span>
        <button className="panel-close" onClick={onClose}>
          ✕
        </button>
      </div>

      <div className="chat-messages">
        {messages.map(msg => {
          const user = users.find(
            u => u.identity.toHexString() === msg.authorIdentity.toHexString()
          );
          const isMe =
            msg.authorIdentity.toHexString() === myIdentity?.toHexString();

          return (
            <div key={msg.id.toString()} className="chat-message">
              <div
                className="chat-message-avatar"
                style={{ background: user?.avatarColor || '#4cf490' }}
              />
              <div className="chat-message-content">
                <div className="chat-message-header">
                  <span className="chat-message-author">
                    {user?.displayName || 'Unknown'} {isMe && '(you)'}
                  </span>
                  <span className="chat-message-time">
                    {formatTime(msg.createdAt)}
                  </span>
                </div>
                <div className="chat-message-text">{msg.content}</div>
              </div>
            </div>
          );
        })}
        <div ref={messagesEndRef} />
      </div>

      {typingUsers.length > 0 && (
        <div className="typing-indicator">
          {typingUsers.join(', ')} {typingUsers.length === 1 ? 'is' : 'are'}{' '}
          typing...
        </div>
      )}

      <div className="chat-input-area">
        <div className="chat-input-wrapper">
          <input
            type="text"
            className="chat-input form-input"
            placeholder="Type a message..."
            value={message}
            onChange={e => {
              setMessage(e.target.value);
              handleTyping(e.target.value.length > 0);
            }}
            onKeyDown={e => {
              if (e.key === 'Enter') handleSend();
            }}
            onBlur={() => handleTyping(false)}
          />
          <button className="btn btn-primary" onClick={handleSend}>
            Send
          </button>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// SHARE MODAL
// ============================================================================

interface ShareModalProps {
  canvas: RowType;
  members: RowType[];
  users: RowType[];
  conn: DbConnection | null;
  myIdentity: Identity | null;
  onClose: () => void;
}

function ShareModal({
  canvas,
  members,
  users,
  conn,
  myIdentity,
  onClose,
}: ShareModalProps) {
  const [sharePermission, setSharePermission] = useState<'view' | 'edit'>(
    'view'
  );
  const [copied, setCopied] = useState(false);

  const isOwner =
    canvas.ownerIdentity.toHexString() === myIdentity?.toHexString();
  const shareLink = canvas.shareLinkToken
    ? `${window.location.origin}?join=${canvas.shareLinkToken}`
    : null;

  const handleGenerateLink = () => {
    if (!conn) return;
    conn.reducers.generateShareLink({
      canvasId: canvas.id,
      permission: sharePermission,
    });
  };

  const handleRevokeLink = () => {
    if (!conn) return;
    conn.reducers.revokeShareLink({ canvasId: canvas.id });
  };

  const handleCopyLink = () => {
    if (shareLink) {
      navigator.clipboard.writeText(shareLink);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const handleChangeRole = (memberIdentity: Identity, role: string) => {
    if (!conn) return;
    conn.reducers.setMemberRole({ canvasId: canvas.id, memberIdentity, role });
  };

  const handleRemoveMember = (memberIdentity: Identity) => {
    if (!conn || !confirm('Remove this member?')) return;
    conn.reducers.removeMember({ canvasId: canvas.id, memberIdentity });
  };

  const canvasMembers = members.filter(m => m.canvasId === canvas.id);

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div
        className="modal"
        style={{ maxWidth: 500 }}
        onClick={e => e.stopPropagation()}
      >
        <div className="modal-header">
          <span className="modal-title">Share "{canvas.name}"</span>
          <button className="panel-close" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="modal-body">
          {isOwner && (
            <div style={{ marginBottom: '1.5rem' }}>
              <h4
                style={{
                  marginBottom: '0.75rem',
                  color: 'var(--text-primary)',
                }}
              >
                Share Link
              </h4>

              {shareLink ? (
                <div>
                  <div
                    style={{
                      display: 'flex',
                      gap: '0.5rem',
                      marginBottom: '0.75rem',
                    }}
                  >
                    <input
                      type="text"
                      className="form-input"
                      value={shareLink}
                      readOnly
                      style={{ flex: 1 }}
                    />
                    <button
                      className="btn btn-primary"
                      onClick={handleCopyLink}
                    >
                      {copied ? '✓ Copied!' : 'Copy'}
                    </button>
                  </div>
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '0.75rem',
                    }}
                  >
                    <span
                      style={{
                        fontSize: '0.875rem',
                        color: 'var(--text-muted)',
                      }}
                    >
                      Anyone with the link can{' '}
                      <strong>{canvas.shareLinkPermission}</strong>
                    </span>
                    <button
                      className="btn btn-danger"
                      onClick={handleRevokeLink}
                      style={{ marginLeft: 'auto' }}
                    >
                      Revoke
                    </button>
                  </div>
                </div>
              ) : (
                <div>
                  <div
                    style={{
                      display: 'flex',
                      gap: '0.5rem',
                      alignItems: 'center',
                      marginBottom: '0.75rem',
                    }}
                  >
                    <span
                      style={{
                        fontSize: '0.875rem',
                        color: 'var(--text-muted)',
                      }}
                    >
                      Permission:
                    </span>
                    <select
                      value={sharePermission}
                      onChange={e =>
                        setSharePermission(e.target.value as 'view' | 'edit')
                      }
                      style={{ padding: '0.5rem' }}
                    >
                      <option value="view">Can View</option>
                      <option value="edit">Can Edit</option>
                    </select>
                  </div>
                  <button
                    className="btn btn-primary"
                    onClick={handleGenerateLink}
                  >
                    Generate Share Link
                  </button>
                </div>
              )}
            </div>
          )}

          <h4 style={{ marginBottom: '0.75rem', color: 'var(--text-primary)' }}>
            Members ({canvasMembers.length})
          </h4>
          <div className="user-list">
            {canvasMembers.map(member => {
              const user = users.find(
                u =>
                  u.identity.toHexString() === member.userIdentity.toHexString()
              );
              const isMe =
                member.userIdentity.toHexString() === myIdentity?.toHexString();
              const isMemberOwner = member.role === 'owner';

              return (
                <div key={member.id.toString()} className="user-item">
                  <div
                    className="user-avatar"
                    style={{ background: user?.avatarColor || '#4cf490' }}
                  >
                    {user?.displayName?.slice(0, 2).toUpperCase() || '??'}
                  </div>
                  <div className="user-info">
                    <div className="user-name">
                      {user?.displayName || 'Unknown'} {isMe && '(you)'}
                    </div>
                    <div className="user-status">{member.role}</div>
                  </div>
                  {isOwner && !isMemberOwner && !isMe && (
                    <div style={{ display: 'flex', gap: '0.5rem' }}>
                      <select
                        value={member.role}
                        onChange={e =>
                          handleChangeRole(member.userIdentity, e.target.value)
                        }
                        style={{ padding: '0.25rem', fontSize: '0.75rem' }}
                      >
                        <option value="editor">Editor</option>
                        <option value="viewer">Viewer</option>
                      </select>
                      <button
                        className="btn btn-icon"
                        onClick={() => handleRemoveMember(member.userIdentity)}
                        style={{
                          width: 28,
                          height: 28,
                          color: 'var(--stdb-red)',
                        }}
                        title="Remove"
                      >
                        ✕
                      </button>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
        <div className="modal-footer">
          <button className="btn" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// SETTINGS MODAL
// ============================================================================

interface SettingsModalProps {
  myUser: RowType | undefined;
  conn: DbConnection | null;
  onClose: () => void;
}

function SettingsModal({ myUser, conn, onClose }: SettingsModalProps) {
  const [displayName, setDisplayName] = useState(myUser?.displayName || '');
  const [avatarColor, setAvatarColor] = useState(
    myUser?.avatarColor || '#4cf490'
  );

  const handleSave = () => {
    if (!conn) return;
    if (displayName !== myUser?.displayName) {
      conn.reducers.setDisplayName({ displayName });
    }
    if (avatarColor !== myUser?.avatarColor) {
      conn.reducers.setAvatarColor({ color: avatarColor });
    }
    onClose();
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <span className="modal-title">Settings</span>
          <button className="panel-close" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="modal-body">
          <div className="form-group">
            <label className="form-label">Display Name</label>
            <input
              type="text"
              className="form-input"
              value={displayName}
              onChange={e => setDisplayName(e.target.value)}
              maxLength={50}
            />
          </div>
          <div className="form-group">
            <label className="form-label">Avatar Color</label>
            <div
              style={{ display: 'flex', alignItems: 'center', gap: '0.75rem' }}
            >
              <input
                type="color"
                value={avatarColor}
                onChange={e => setAvatarColor(e.target.value)}
              />
              <div className="user-avatar" style={{ background: avatarColor }}>
                {displayName.slice(0, 2).toUpperCase() || '??'}
              </div>
            </div>
          </div>
        </div>
        <div className="modal-footer">
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn-primary" onClick={handleSave}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
