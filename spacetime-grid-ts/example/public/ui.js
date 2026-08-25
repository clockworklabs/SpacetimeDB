import {
  HEX_MAX_X,
  HEX_MAX_Y,
  HEX_MIN_X,
  HEX_MIN_Y,
  HEX_SIZE,
  PAD,
  axialHexDistance,
  cellKey,
  cellsWithinHexDistance,
  hexCenter,
  hexCorners,
  isInHexShape,
  pixelToHex,
  samplePathPixels,
} from './hex-geometry.js';

const $ = id => document.getElementById(id);
let toastTimer = null;
function toast(kind, msg) {
  const el = $('toast');
  el.textContent = msg;
  el.className = `toast ${kind} show`;
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    el.classList.remove('show');
  }, 3000);
}

// Authentication view
let authMode = 'login';
function setAuthMode(m) {
  authMode = m;
  const title = $('auth-title'),
    sub = $('auth-sub'),
    submit = $('auth-submit');
  const togglePrompt = $('toggle-prompt'),
    toggleLink = $('toggle-link');
  const forgotFoot = $('forgot-link').parentElement;
  const nameField = $('auth-name-field'),
    passField = $('auth-pass').closest('.auth-field');
  if (m === 'signup') {
    title.textContent = 'Create an account';
    sub.textContent = 'Sign up to play.';
    submit.textContent = 'Create account';
    togglePrompt.textContent = 'Already have an account?';
    toggleLink.textContent = 'Sign in';
    forgotFoot.hidden = true;
    nameField.hidden = false;
    passField.hidden = false;
    $('auth-pass').autocomplete = 'new-password';
  } else if (m === 'forgot') {
    title.textContent = 'Reset password';
    sub.textContent = "Enter your email and we'll send a reset link.";
    submit.textContent = 'Send reset link';
    togglePrompt.textContent = 'Remembered it?';
    toggleLink.textContent = 'Sign in';
    forgotFoot.hidden = true;
    nameField.hidden = true;
    passField.hidden = true;
  } else {
    title.textContent = 'Welcome to Grid';
    sub.textContent = 'Sign in to continue.';
    submit.textContent = 'Sign in';
    togglePrompt.textContent = "Don't have an account?";
    toggleLink.textContent = 'Sign up';
    forgotFoot.hidden = false;
    nameField.hidden = true;
    passField.hidden = false;
    $('auth-pass').autocomplete = 'current-password';
  }
}
$('toggle-link').addEventListener('click', () =>
  setAuthMode(authMode === 'login' ? 'signup' : 'login')
);
$('forgot-link').addEventListener('click', () => setAuthMode('forgot'));
$('auth-form').addEventListener('submit', async e => {
  e.preventDefault();
  if (!window.auth) return;
  const email = $('auth-email').value.trim();
  const password = $('auth-pass').value;
  $('auth-submit').disabled = true;
  try {
    if (authMode === 'signup') {
      const name = $('auth-name').value.trim() || undefined;
      await window.auth.signup({ email, password, name });
    } else if (authMode === 'forgot') {
      await window.auth.forgotPassword(email);
      toast('ok', 'Reset link sent (dev mailer logs to STDB console).');
      setAuthMode('login');
    } else {
      await window.auth.login({ email, password });
    }
  } catch (err) {
    toast('err', err.message ?? String(err));
  } finally {
    $('auth-submit').disabled = false;
  }
});
window.addEventListener('auth:server-config', e => {
  const oauth = e.detail?.oauth || {};
  const google = $('oauth-google');
  const github = $('oauth-github');
  google.disabled = !oauth.google;
  github.disabled = !oauth.github;
  google.title = oauth.google
    ? ''
    : 'Set GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET in .env';
  github.title = oauth.github
    ? ''
    : 'Set GITHUB_CLIENT_ID and GITHUB_CLIENT_SECRET in .env';
  google.setAttribute('aria-disabled', String(!oauth.google));
  github.setAttribute('aria-disabled', String(!oauth.github));
});
$('oauth-google').addEventListener('click', () => {
  if ($('oauth-google').disabled) {
    toast('err', 'Google OAuth is not configured');
    return;
  }
  window.auth?.oauthStart('google');
});
$('oauth-github').addEventListener('click', () => {
  if ($('oauth-github').disabled) {
    toast('err', 'GitHub OAuth is not configured');
    return;
  }
  window.auth?.oauthStart('github');
});
$('btn-logout').addEventListener('click', () => window.auth?.logout());

// State and view routing
let state = null;
let selectedUnitId = null;
let reachableCache = null; // { entityId, cells: Set<"q,r"> } for Dijkstra reachability
let attackableCache = null; // Set<"q,r"> within the selected unit's attack range
// Active move animations: entityId → { pathPx: [(x,y)…], startMs, durationMs }.
// While present, drawBoard renders the unit at the interpolated pixel
// position while the data x/y already contains the destination.
const animatingUnits = new Map();
const MS_PER_HEX = 110; // tune for snappiness
let rafScheduled = false;
function scheduleAnimFrame() {
  const hasWork = () =>
    animatingUnits.size > 0 ||
    attackFlashes.length > 0 ||
    visualHp.size > 0 ||
    ghostUnits.size > 0 ||
    damageNumbers.length > 0;
  if (rafScheduled || !hasWork()) return;
  rafScheduled = true;
  requestAnimationFrame(() => {
    rafScheduled = false;
    const now = performance.now();
    // Prune finished move animations.
    for (const [id, a] of animatingUnits) {
      if (now >= a.startMs + a.durationMs) animatingUnits.delete(id);
    }
    // Prune expired attack flashes.
    for (let i = attackFlashes.length - 1; i >= 0; i--) {
      if (now >= attackFlashes[i].startMs + attackFlashes[i].durationMs) {
        attackFlashes.splice(i, 1);
      }
    }
    // Prune expired damage numbers.
    for (let i = damageNumbers.length - 1; i >= 0; i--) {
      if (now >= damageNumbers[i].startMs + damageNumbers[i].durationMs) {
        damageNumbers.splice(i, 1);
      }
    }
    renderMatch();
    if (hasWork()) scheduleAnimFrame();
  });
}
function showAuth() {
  $('auth-shell').hidden = false;
  $('shell').hidden = true;
}
function showShell() {
  $('auth-shell').hidden = true;
  $('shell').hidden = false;
}
function showLobby() {
  $('lobby').hidden = false;
  $('match-view').hidden = true;
}
function showMatchView() {
  $('lobby').hidden = true;
  $('match-view').hidden = false;
}

window.addEventListener('grid:ready', () => {
  /* initial render kicked by auth:state */
});

window.addEventListener('grid:auth', e => {
  const u = e.detail.user;
  if (u) {
    showShell();
    $('user-name').textContent = u.name || u.email;
    $('user-avatar').textContent = (u.name || u.email)
      .slice(0, 1)
      .toUpperCase();
  } else {
    showAuth();
  }
});

window.addEventListener('grid:state', e => {
  state = e.detail;
  renderAll();
});

// Visual overrides applied while attack animations are playing.
// Keep the data-state HP/death hidden until the flash completes,
// so the player sees: move → pause → flash → HP drop / death.
const visualHp = new Map(); // entityId → preferred HP (overrides data)
const ghostUnits = new Map(); // entityId → { x, y, ownerUserId, typeId, currentHp }; rendered during removal animations
const attackFlashes = []; // [{ from, to, startMs, durationMs }]
const damageNumbers = []; // [{ x, y, dmg, killed, startMs, durationMs }]
const FLASH_MS = 220;
const DAMAGE_FLOAT_MS = 950;

// Spawn a floating "-N" damage indicator at (px, py). Drifts up + fades.
function spawnDamageNumber(px, py, dmg, killed, atMs) {
  damageNumbers.push({
    x: px,
    y: py - HEX_SIZE * 0.4,
    dmg,
    killed,
    startMs: atMs ?? performance.now(),
    durationMs: DAMAGE_FLOAT_MS,
  });
  scheduleAnimFrame();
}

// Snapshot a target's current visual state, push an attack flash + a
// floating damage number, then release the HP/ghost override after
// FLASH_MS so the data-state HP/death is shown only after the flash
// plays. Used by both player + AI attacks.
function flashAttack(attackerId, target, dmg, killed) {
  visualHp.set(target.entityId, target.currentHp);
  ghostUnits.set(target.entityId, {
    entityId: target.entityId,
    x: target.x,
    y: target.y,
    ownerUserId: target.ownerUserId,
    typeId: target.typeId,
    currentHp: target.currentHp,
  });
  const startMs = performance.now();
  attackFlashes.push({
    attackerId,
    targetX: target.x,
    targetY: target.y,
    startMs,
    durationMs: FLASH_MS,
  });
  // Damage number pops at the midpoint of the flash so it reads as
  // "the hit landed, here's how much it cost you."
  const { cx, cy } = hexCenter(target.x, target.y);
  spawnDamageNumber(cx, cy, dmg, killed, startMs + FLASH_MS / 2);
  scheduleAnimFrame();
  setTimeout(() => {
    visualHp.delete(target.entityId);
    ghostUnits.delete(target.entityId);
    renderMatch();
  }, FLASH_MS);
}

// Schedule each AI-turn event and release its visual override. The event
// list arrives in execution order.
window.addEventListener('grid:ai-events', e => {
  const events = e.detail?.events ?? [];
  let cursorMs = performance.now();
  const PAUSE_MS = 80;

  for (const ev of events) {
    let moveEndMs = cursorMs;

    // 1. Move animation (if any).
    if (Array.isArray(ev.movePath) && ev.movePath.length >= 2) {
      const pathPx = ev.movePath.map(c => {
        const { cx, cy } = hexCenter(c.x, c.y);
        return { x: cx, y: cy };
      });
      const hops = pathPx.length - 1;
      const durationMs = Math.max(180, hops * MS_PER_HEX);
      animatingUnits.set(ev.entityId, {
        pathPx,
        startMs: cursorMs,
        durationMs,
      });
      moveEndMs = cursorMs + durationMs;
      cursorMs = moveEndMs + PAUSE_MS;
    }

    // 2. Attack (if any). Preserve the target's pre-attack visual until
    //    the flash fires; if killed, ghost-render the target so it stays
    //    on screen even though state has already deleted it.
    if (ev.attack) {
      const a = ev.attack;
      // Lock the target's visible HP to its pre-attack value until the flash.
      visualHp.set(a.targetId, a.targetPreHp);
      // Render a ghost for a defeated target during its removal animation.
      if (a.killed) {
        ghostUnits.set(a.targetId, {
          entityId: a.targetId,
          x: a.targetX,
          y: a.targetY,
          ownerUserId: a.targetOwner,
          typeId: a.targetTypeId,
          currentHp: a.targetPreHp,
        });
      }
      const attackStartMs = cursorMs;
      // Schedule the flash + release of the visual overrides.
      attackFlashes.push({
        attackerId: ev.entityId,
        targetX: a.targetX,
        targetY: a.targetY,
        startMs: attackStartMs,
        durationMs: FLASH_MS,
      });
      // Floating "-N" damage number, scheduled to pop mid-flash.
      const { cx, cy } = hexCenter(a.targetX, a.targetY);
      spawnDamageNumber(
        cx,
        cy,
        a.damage,
        a.killed,
        attackStartMs + FLASH_MS / 2
      );
      // When the flash fires, release HP override + ghost.
      setTimeout(
        () => {
          visualHp.delete(a.targetId);
          ghostUnits.delete(a.targetId);
          renderMatch();
        },
        attackStartMs - performance.now() + FLASH_MS
      );
      cursorMs += FLASH_MS + PAUSE_MS;
    }
  }

  if (
    animatingUnits.size > 0 ||
    attackFlashes.length > 0 ||
    ghostUnits.size > 0
  ) {
    renderMatch();
    scheduleAnimFrame();
  }
});

// Default to auth view until grid:state arrives.
showAuth();

// Rendering
function renderAll() {
  if (!state) return;
  renderLobby();
  if (state.activeMatchId !== null && state.activeMatch) {
    showMatchView();
    renderMatch();
  } else {
    showLobby();
  }
}

// Tag → lowercase string for CSS class names and display text.
// (Enum tags from the bindings are PascalCase: 'Waiting', 'Active', etc.)
function statusKey(s) {
  return s?.tag ? s.tag.toLowerCase() : 'unknown';
}

// Resolve seats from match_participant rows.
function seatsForMatch(matchId) {
  const seats = {};
  for (const p of state.participants ?? []) {
    if (p.matchId === matchId) seats[p.seatIdx] = p.userId;
  }
  return seats;
}
function actorById(id) {
  return id ? (state.actors ?? []).find(a => a.actorId === id) : null;
}
function displayName(actor, fallbackId) {
  if (!actor) return fallbackId ? fallbackId.slice(0, 8) : 'Unknown';
  return actor.name || actor.actorId.slice(0, 8);
}

function textSpan(className, text) {
  const span = document.createElement('span');
  span.className = className;
  span.textContent = text;
  return span;
}

function hint(text) {
  const div = document.createElement('div');
  div.className = 'hint';
  div.textContent = text;
  return div;
}

function infoRow(label, value) {
  const row = document.createElement('div');
  row.className = 'row';
  row.appendChild(textSpan('label', label));
  row.appendChild(textSpan('value', value));
  return row;
}

function renderLobby() {
  const list = $('match-list');
  const my = state.myUserId;
  const myMatches = state.matches ?? [];
  const openOther = (state.openMatches ?? []).filter(
    o => !myMatches.some(m => m.matchId === o.matchId)
  );
  if (myMatches.length === 0 && openOther.length === 0) {
    const empty = document.createElement('div');
    empty.className = 'lobby-empty';
    empty.textContent = 'No missions in this sector. Deploy one to begin.';
    list.replaceChildren(empty);
    return;
  }
  list.replaceChildren();

  // Open matches anyone can join.
  for (const o of openOther) {
    const row = document.createElement('div');
    row.className = 'match-row';
    const meta = document.createElement('div');
    meta.className = 'match-meta';
    const hostName = displayName(actorById(o.hostUserId), o.hostUserId);
    meta.appendChild(textSpan('id', `#${o.matchId}`));
    meta.appendChild(textSpan('players', `${hostName} waiting for opponent`));
    const status = document.createElement('span');
    status.className = 'match-status waiting';
    status.textContent = 'waiting';
    const actions = document.createElement('div');
    actions.style.display = 'flex';
    actions.style.gap = '8px';
    actions.style.alignItems = 'center';
    actions.appendChild(status);
    const btn = document.createElement('button');
    btn.className = 'btn small primary';
    btn.textContent = 'Join';
    btn.addEventListener('click', async () => {
      try {
        await window.grid.joinMatch(o.matchId);
        window.grid.setActiveMatch(o.matchId);
      } catch (err) {
        toast('err', err.message ?? String(err));
      }
    });
    actions.appendChild(btn);
    row.appendChild(meta);
    row.appendChild(actions);
    list.appendChild(row);
  }

  for (const m of myMatches) {
    const row = document.createElement('div');
    row.className = 'match-row';
    const meta = document.createElement('div');
    meta.className = 'match-meta';
    const seats = seatsForMatch(m.matchId);
    const hostUid = seats[0];
    const oppUid = seats[1];
    const isVsAi = oppUid === window.grid.AI_BOT_USER_ID;
    const hostName = displayName(actorById(hostUid), hostUid);
    const oppName = isVsAi
      ? 'Alien Hive'
      : displayName(actorById(oppUid), oppUid);
    meta.appendChild(textSpan('id', `#${m.matchId}`));
    const players = textSpan('players', `${hostName} vs ${oppName}`);
    if (isVsAi) {
      players.append(' ');
      players.appendChild(textSpan('mode-badge ai', 'SOLO'));
    }
    meta.appendChild(players);
    const status = document.createElement('span');
    status.className = `match-status ${statusKey(m.status)}`;
    status.textContent = statusKey(m.status);
    const actions = document.createElement('div');
    actions.style.display = 'flex';
    actions.style.gap = '8px';
    actions.style.alignItems = 'center';
    actions.appendChild(status);

    const inMatch = Object.values(seats).includes(my);
    if (m.status.tag === 'Waiting' && !inMatch) {
      const btn = document.createElement('button');
      btn.className = 'btn small primary';
      btn.textContent = 'Join';
      btn.addEventListener('click', async () => {
        try {
          await window.grid.joinMatch(m.matchId);
          window.grid.setActiveMatch(m.matchId);
        } catch (err) {
          toast('err', err.message ?? String(err));
        }
      });
      actions.appendChild(btn);
    } else if (m.status.tag === 'Active' && inMatch) {
      const btn = document.createElement('button');
      btn.className = 'btn small primary';
      btn.textContent = 'Resume';
      btn.addEventListener('click', () =>
        window.grid.setActiveMatch(m.matchId)
      );
      actions.appendChild(btn);
    } else if (m.status.tag === 'Waiting' && inMatch) {
      const btn = document.createElement('button');
      btn.className = 'btn small';
      btn.textContent = 'View';
      btn.addEventListener('click', () =>
        window.grid.setActiveMatch(m.matchId)
      );
      actions.appendChild(btn);
    } else {
      const btn = document.createElement('button');
      btn.className = 'btn small';
      btn.textContent = 'Spectate';
      btn.addEventListener('click', () =>
        window.grid.setActiveMatch(m.matchId)
      );
      actions.appendChild(btn);
    }
    row.appendChild(meta);
    row.appendChild(actions);
    list.appendChild(row);
  }
}

function renderMatch() {
  const m = state.activeMatch;
  const grid = state.activeGrid;
  if (!m || !grid) return;
  const my = state.myUserId;
  const seats = seatsForMatch(m.matchId);
  const mySeatIdx = Object.entries(seats).find(([, uid]) => uid === my)?.[0];
  const isHost = mySeatIdx === '0';
  const myTurn =
    m.status.tag === 'Active' &&
    mySeatIdx !== undefined &&
    Number(mySeatIdx) === m.currentSeatIdx;

  const isVsAi = seats[1] === window.grid.AI_BOT_USER_ID;
  $('match-title').textContent =
    `Sector ${m.matchId}${isVsAi ? ' · solo' : ''}`;
  $('match-subtitle').textContent = `, turn ${m.turnNumber}`;

  const banner = $('turn-banner');
  if (m.status.tag === 'Waiting') {
    banner.className = 'turn-banner waiting';
    banner.textContent = isHost
      ? 'Waiting for an opponent to join…'
      : 'Match is waiting for a host';
  } else if (m.status.tag === 'Ended') {
    banner.className = 'turn-banner';
    banner.textContent =
      m.winnerUserId === my ? 'You won this match.' : 'Match ended.';
  } else if (myTurn) {
    banner.className = 'turn-banner mine';
    banner.textContent = 'Your turn';
  } else {
    banner.className = 'turn-banner theirs';
    banner.textContent = isVsAi ? 'Aliens advancing…' : "Opponent's turn";
  }

  $('btn-end-turn').disabled = !myTurn;

  // Sidebar unit lists
  const my0 = [];
  const en = [];
  for (const u of state.units) {
    const type = state.unitTypes.find(t => t.typeId === u.typeId);
    const entity = state.entities.find(e => e.id === u.entityId);
    const item = { u, type, entity };
    (u.ownerUserId === my ? my0 : en).push(item);
  }
  function pill(item) {
    const div = document.createElement('div');
    div.className = `unit-pill ${item.u.ownerUserId === my ? 'mine' : 'enemy'}`;
    const left = document.createElement('span');
    left.textContent = `${item.type?.glyph ?? '?'} ${item.type?.name ?? item.u.typeId} (${item.entity?.x},${item.entity?.y})`;
    const right = document.createElement('span');
    right.textContent = `${item.u.currentHp}/${item.type?.hp ?? '?'} HP`;
    div.appendChild(left);
    div.appendChild(right);
    return div;
  }
  const myList = $('my-units');
  const enList = $('enemy-units');
  myList.replaceChildren();
  enList.replaceChildren();
  for (const x of my0) myList.appendChild(pill(x));
  for (const x of en) enList.appendChild(pill(x));
  if (my0.length === 0) myList.appendChild(hint('no units'));
  if (en.length === 0) enList.appendChild(hint('no units'));

  // Selected-unit info
  const selUnit =
    selectedUnitId !== null
      ? state.units.find(u => u.entityId === selectedUnitId)
      : null;
  const selEntity = selUnit
    ? state.entities.find(e => e.id === selUnit.entityId)
    : null;
  const selType = selUnit
    ? state.unitTypes.find(t => t.typeId === selUnit.typeId)
    : null;
  const sel = $('selected-unit-info');
  if (selUnit && selType && selEntity) {
    const card = document.createElement('div');
    card.className = 'selected-unit';
    card.appendChild(infoRow('Unit', `${selType.glyph} ${selType.name}`));
    card.appendChild(infoRow('Position', `(${selEntity.x}, ${selEntity.y})`));
    card.appendChild(infoRow('HP', `${selUnit.currentHp} / ${selType.hp}`));
    card.appendChild(
      infoRow(
        'Movement',
        `${selType.movement} (used: ${selUnit.hasMoved ? 'yes' : 'no'})`
      )
    );
    card.appendChild(
      infoRow(
        'Attack',
        `${selType.attackDmg} dmg · range ${selType.attackRange} (used: ${selUnit.hasAttacked ? 'yes' : 'no'})`
      )
    );
    sel.replaceChildren(card);
  } else {
    sel.replaceChildren(hint('click one of your units to select'));
  }

  // Canvas
  drawBoard(grid, state.entities, state.cells, state.units, m, my);

  // End-modal
  if (m.status.tag === 'Ended') {
    $('end-title').textContent = m.winnerUserId === my ? 'Victory!' : 'Defeat';
    $('end-title').className = m.winnerUserId === my ? 'win' : 'lose';
    const winner = actorById(m.winnerUserId);
    const winnerName = winner
      ? winner.name || m.winnerUserId.slice(0, 8)
      : 'someone';
    $('end-body').textContent =
      m.winnerUserId === my
        ? `You secured the sector.`
        : `${winnerName} overran the outpost.`;
    $('end-backdrop').classList.add('open');
  } else {
    $('end-backdrop').classList.remove('open');
  }
}

$('end-back-to-lobby').addEventListener('click', () => {
  $('end-backdrop').classList.remove('open');
  window.grid.setActiveMatch(null);
});
$('btn-leave-match').addEventListener('click', () => {
  selectedUnitId = null;
  reachableCache = null;
  attackableCache = null;
  window.grid.setActiveMatch(null);
});
$('btn-create-match').addEventListener('click', async () => {
  try {
    const r = await window.grid.createMatch(false);
    window.grid.setActiveMatch(r.matchId);
  } catch (err) {
    toast('err', err.message ?? String(err));
  }
});
$('btn-create-match-ai').addEventListener('click', async () => {
  try {
    const r = await window.grid.createMatch(true);
    window.grid.setActiveMatch(r.matchId);
  } catch (err) {
    toast('err', err.message ?? String(err));
  }
});
$('btn-end-turn').addEventListener('click', async () => {
  if (!state?.activeMatchId) return;
  try {
    selectedUnitId = null;
    reachableCache = null;
    attackableCache = null;
    await window.grid.endTurn(state.activeMatchId);
  } catch (err) {
    toast('err', err.message ?? String(err));
  }
});

// Canvas drawing
function drawBoard(grid, entities, cells, units, match, myUserId) {
  const canvas = $('board-canvas');
  // Canvas only needs to hold the hex-shape bounding box (computed at
  // module load from HEX_RADIUS). Out-of-hex axial cells aren't drawn.
  const W = Math.ceil(HEX_MAX_X - HEX_MIN_X + 2 * PAD);
  const H = Math.ceil(HEX_MAX_Y - HEX_MIN_Y + 2 * PAD);
  canvas.width = W;
  canvas.height = H;
  const ctx = canvas.getContext('2d');
  // Match the surrounding panel with a solid STDB shade7 background.
  ctx.fillStyle =
    getComputedStyle(document.body).getPropertyValue('--color-shade7').trim() ||
    '#0b1114';
  ctx.fillRect(0, 0, W, H);
  // A deterministic pale-blue starfield supports the alien-planet view.
  const starSeed = (grid.width * 31 + grid.height) | 0;
  let s = starSeed;
  for (let i = 0; i < 60; i++) {
    s = (s * 1664525 + 1013904223) >>> 0;
    const sx = ((s % 1000) / 1000) * W;
    s = (s * 1664525 + 1013904223) >>> 0;
    const sy = ((s % 1000) / 1000) * H;
    s = (s * 1664525 + 1013904223) >>> 0;
    const a = 0.12 + ((s % 1000) / 1000) * 0.25;
    ctx.fillStyle = `rgba(160, 180, 200, ${a})`;
    ctx.fillRect(sx, sy, 1, 1);
  }

  const cellMap = new Map(cells.map(c => [cellKey(c.x, c.y), c]));
  const reachable = reachableCache?.cells ?? null;

  // 1. Draw all hex cells (STDB-palette only)
  //   regolith = default dark teal plain      (shade5/shade4)
  //   crater   = burned, impassable obstacle  (shade7 + dim red edge)
  //   void     = outside the hex play area and omitted from rendering
  for (let y = 0; y < grid.height; y++) {
    for (let x = 0; x < grid.width; x++) {
      if (!isInHexShape(x, y)) continue;
      const { cx, cy } = hexCenter(x, y);
      const corners = hexCorners(cx, cy, HEX_SIZE - 1);
      const cell = cellMap.get(cellKey(x, y));
      let fill = '#0e161a'; // shade6 regolith (default)
      let stroke = '#162d38'; // shade1
      if (cell && cell.terrain === 'crater') {
        fill = '#080c0e';
        stroke = '#3a1a22';
      }
      const isReachable = reachable && reachable.has(cellKey(x, y));
      const isAttackable =
        attackableCache && attackableCache.has(cellKey(x, y));
      ctx.beginPath();
      ctx.moveTo(corners[0][0], corners[0][1]);
      for (let i = 1; i < 6; i++) ctx.lineTo(corners[i][0], corners[i][1]);
      ctx.closePath();
      ctx.fillStyle = fill;
      ctx.fill();
      if (isAttackable) {
        // STDB red at 28% marks hostile movement range.
        ctx.fillStyle = 'rgba(255, 76, 76, 0.28)';
        ctx.fill();
        ctx.strokeStyle = '#ff4c4c';
        ctx.lineWidth = 1.5;
      } else if (isReachable) {
        // STDB blue at 20% marks friendly movement range.
        ctx.fillStyle = 'rgba(2, 190, 250, 0.20)';
        ctx.fill();
        ctx.strokeStyle = '#02befa';
        ctx.lineWidth = 1.5;
      } else {
        ctx.strokeStyle = stroke;
        ctx.lineWidth = 1;
      }
      ctx.stroke();
    }
  }

  // 2. Draw units (data state + ghost-rendered killed units pending flash).
  //    The helper draws live units and ghosts with identical logic.
  const drawUnit = (u, posX, posY, hpForBar) => {
    const type = state.unitTypes.find(t => t.typeId === u.typeId);
    const isMine = u.ownerUserId === myUserId;
    // Use STDB blue for the player's landing party and green for the xeno hive.
    const color = isMine ? '#02befa' : '#4cf490';
    ctx.beginPath();
    ctx.arc(posX, posY, HEX_SIZE * 0.55, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.fill();
    ctx.strokeStyle = u.entityId === selectedUnitId ? '#fbdc8e' : '#000a';
    ctx.lineWidth = u.entityId === selectedUnitId ? 3 : 2;
    ctx.stroke();
    // Glyph
    ctx.fillStyle = '#060606';
    ctx.font = 'bold 14px "Source Code Pro", monospace';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(type?.glyph ?? '?', posX, posY);
    // HP bar
    const hpPct = type ? hpForBar / type.hp : 0;
    const barW = HEX_SIZE * 0.9;
    ctx.fillStyle = '#000a';
    ctx.fillRect(posX - barW / 2, posY + HEX_SIZE * 0.6, barW, 4);
    ctx.fillStyle =
      hpPct > 0.5 ? '#4cf490' : hpPct > 0.25 ? '#fbdc8e' : '#ff4c4c';
    ctx.fillRect(posX - barW / 2, posY + HEX_SIZE * 0.6, barW * hpPct, 4);
    // Greyed out if hasMoved + hasAttacked (turn done)
    if (isMine && u.hasMoved && u.hasAttacked) {
      ctx.fillStyle = 'rgba(0,0,0,0.4)';
      ctx.beginPath();
      ctx.arc(posX, posY, HEX_SIZE * 0.55, 0, Math.PI * 2);
      ctx.fill();
    }
  };

  for (const u of units) {
    // Skip if this unit is being ghost-rendered (its data state may
    // be stale during the animation; the ghost below supplies the visual).
    if (ghostUnits.has(u.entityId)) continue;
    const ent = entities.find(e => e.id === u.entityId);
    if (!ent) continue;
    // Use the interpolated pixel position while a unit is moving. Its
    // data x/y already contains the destination.
    let cx, cy;
    const anim = animatingUnits.get(u.entityId);
    if (anim) {
      const t = Math.min(
        1,
        (performance.now() - anim.startMs) / anim.durationMs
      );
      const p = samplePathPixels(anim.pathPx, t);
      cx = p.x;
      cy = p.y;
    } else {
      const c = hexCenter(ent.x, ent.y);
      cx = c.cx;
      cy = c.cy;
    }
    // Use visual HP override if pending attack hasn't fired yet.
    const hp = visualHp.has(u.entityId)
      ? visualHp.get(u.entityId)
      : u.currentHp;
    drawUnit(u, cx, cy, hp);
  }

  // 3. Ghost-render defeated AI targets until their attack flash runs.
  for (const g of ghostUnits.values()) {
    const { cx, cy } = hexCenter(g.x, g.y);
    drawUnit(
      {
        entityId: g.entityId,
        ownerUserId: g.ownerUserId,
        typeId: g.typeId,
        hasMoved: true,
        hasAttacked: true,
      },
      cx,
      cy,
      visualHp.has(g.entityId) ? visualHp.get(g.entityId) : g.currentHp
    );
  }

  // 4. Draw a bright red attack beam from attacker to target.
  for (const f of attackFlashes) {
    const now = performance.now();
    const t = Math.min(1, Math.max(0, (now - f.startMs) / f.durationMs));
    if (t <= 0 || t >= 1) continue;
    // Attacker pixel position: animated if mid-move, else data.
    let fx, fy;
    const aAnim = animatingUnits.get(f.attackerId);
    if (aAnim) {
      const at = Math.min(1, (now - aAnim.startMs) / aAnim.durationMs);
      const p = samplePathPixels(aAnim.pathPx, at);
      fx = p.x;
      fy = p.y;
    } else {
      const aEnt = entities.find(e => e.id === f.attackerId);
      if (!aEnt) continue;
      const p = hexCenter(aEnt.x, aEnt.y);
      fx = p.cx;
      fy = p.cy;
    }
    const tEnd = hexCenter(f.targetX, f.targetY);
    // Pulse: stroke fades out across the flash duration.
    const alpha = 0.9 * (1 - t);
    ctx.strokeStyle = `rgba(255, 76, 76, ${alpha})`;
    ctx.lineWidth = 3;
    ctx.beginPath();
    ctx.moveTo(fx, fy);
    ctx.lineTo(tEnd.cx, tEnd.cy);
    ctx.stroke();
    // Impact ring at target.
    ctx.strokeStyle = `rgba(255, 156, 61, ${alpha})`;
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(tEnd.cx, tEnd.cy, HEX_SIZE * (0.4 + t * 0.6), 0, Math.PI * 2);
    ctx.stroke();
  }

  // 5. Draw floating damage numbers with upward drift and fade above the
  //    flash and impact ring.
  const nowMs = performance.now();
  for (const d of damageNumbers) {
    const t = (nowMs - d.startMs) / d.durationMs;
    if (t < 0 || t > 1) continue;
    // Fade in 0..0.15, hold to 0.7, fade out to 1.0.
    let alpha;
    if (t < 0.15) alpha = t / 0.15;
    else if (t > 0.7) alpha = (1 - t) / 0.3;
    else alpha = 1;
    alpha = Math.max(0, Math.min(1, alpha));
    // Ease-out drift upward.
    const drift = HEX_SIZE * 1.1 * (1 - Math.pow(1 - t, 2));
    const px = d.x;
    const py = d.y - drift;
    const text = d.killed ? `−${d.dmg} KO` : `−${d.dmg}`;
    ctx.font = d.killed
      ? 'bold 18px "Source Code Pro", monospace'
      : 'bold 15px "Source Code Pro", monospace';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    // Dark stroke first so the number stays readable over any hex.
    ctx.strokeStyle = `rgba(0, 0, 0, ${alpha * 0.85})`;
    ctx.lineWidth = 3;
    ctx.strokeText(text, px, py);
    ctx.fillStyle = d.killed
      ? `rgba(255, 76, 76, ${alpha})`
      : `rgba(255, 156, 61, ${alpha})`;
    ctx.fillText(text, px, py);
  }
}

// Click handlers
$('board-canvas').addEventListener('click', async e => {
  if (!state?.activeMatch || !state.activeGrid) return;
  const m = state.activeMatch;
  const grid = state.activeGrid;
  const seats = seatsForMatch(m.matchId);
  const myTurn =
    m.status.tag === 'Active' && seats[m.currentSeatIdx] === state.myUserId;
  if (!myTurn) return;

  const rect = e.target.getBoundingClientRect();
  const px = e.clientX - rect.left;
  const py = e.clientY - rect.top;
  const hex = pixelToHex(px, py, grid.width, grid.height);
  if (!hex) return;

  const entityAtHex = state.entities.find(
    e2 => e2.x === hex.x && e2.y === hex.y
  );
  const unitAtHex = entityAtHex
    ? state.units.find(u => u.entityId === entityAtHex.id)
    : null;

  // Click on my own unit -> select + compute movement + INFLUENCE overlays.
  // Influence is the union of cells within attackRange of every
  // movement-reachable cell, including the origin.
  if (unitAtHex && unitAtHex.ownerUserId === state.myUserId) {
    selectedUnitId = unitAtHex.entityId;
    const type = state.unitTypes.find(t => t.typeId === unitAtHex.typeId);
    // Reachable cells from server (Dijkstra, honors terrain + entity blocking).
    let reachCells = [{ x: hex.x, y: hex.y, cost: 0 }];
    if (type && !unitAtHex.hasMoved) {
      try {
        const r = await window.grid.getCellsInRange(
          grid.id,
          hex.x,
          hex.y,
          type.movement
        );
        reachCells = r;
      } catch {
        reachCells = [{ x: hex.x, y: hex.y, cost: 0 }];
      }
    }
    reachableCache = {
      entityId: unitAtHex.entityId,
      cells: new Set(reachCells.map(c => cellKey(c.x, c.y))),
    };
    // Influence range = "movement zone with attack range tacked on the
    // outside". For each reachable cell, fan out by attackRange. Strip
    // the cells that are already in the movement zone so cyan shows the
    // movement halo and red shows only the OUTER attack ring.
    if (type && !unitAtHex.hasAttacked) {
      const influence = new Set();
      for (const r of reachCells) {
        const ring = cellsWithinHexDistance(
          grid.width,
          grid.height,
          r.x,
          r.y,
          type.attackRange
        );
        for (const k of ring) influence.add(k);
      }
      for (const k of reachableCache.cells) influence.delete(k);
      attackableCache = influence;
    } else {
      attackableCache = null;
    }
    renderMatch();
    return;
  }

  // Click on enemy unit while one of mine is selected -> auto-move + attack
  if (unitAtHex && selectedUnitId !== null) {
    const attacker = state.units.find(u => u.entityId === selectedUnitId);
    const attackerEnt = attacker
      ? state.entities.find(e => e.id === selectedUnitId)
      : null;
    const type = attacker
      ? state.unitTypes.find(t => t.typeId === attacker.typeId)
      : null;
    if (!attacker || !attackerEnt || !type) return;
    if (attacker.hasAttacked) {
      toast('err', 'this unit has already attacked');
      return;
    }

    const targetId = unitAtHex.entityId;
    const attackerId = selectedUnitId;
    const targetX = hex.x,
      targetY = hex.y;

    // Attack from the current position when the target is in range.
    const fromHere = axialHexDistance(
      attackerEnt.x,
      attackerEnt.y,
      targetX,
      targetY
    );
    if (fromHere <= type.attackRange) {
      selectedUnitId = null;
      reachableCache = null;
      attackableCache = null;
      // Snapshot the target so its HP/death is held until the flash fires.
      const targetEnt = state.entities.find(en => en.id === targetId);
      if (targetEnt) {
        const dmg = type.attackDmg;
        const killed = unitAtHex.currentHp - dmg <= 0;
        flashAttack(
          attackerId,
          {
            entityId: targetId,
            x: targetEnt.x,
            y: targetEnt.y,
            ownerUserId: unitAtHex.ownerUserId,
            typeId: unitAtHex.typeId,
            currentHp: unitAtHex.currentHp,
          },
          dmg,
          killed
        );
      }
      renderMatch();
      try {
        await window.grid.attackUnit(attackerId, targetId);
      } catch (err) {
        toast('err', err.message ?? String(err));
      }
      return;
    }

    // Need to move first. Pick the CHEAPEST reachable cell that puts the
    // target within attackRange. Has to be one cellsInRange returned.
    if (
      attacker.hasMoved ||
      !reachableCache ||
      reachableCache.entityId !== attackerId
    ) {
      toast('err', 'target out of range');
      return;
    }
    let bestStep = null;
    for (const k of reachableCache.cells) {
      const [sx, sy] = k.split(',').map(Number);
      if (axialHexDistance(sx, sy, targetX, targetY) <= type.attackRange) {
        if (
          bestStep === null ||
          axialHexDistance(attackerEnt.x, attackerEnt.y, sx, sy) <
            axialHexDistance(
              attackerEnt.x,
              attackerEnt.y,
              bestStep.x,
              bestStep.y
            )
        ) {
          bestStep = { x: sx, y: sy };
        }
      }
    }
    if (!bestStep) {
      toast('err', 'target out of range');
      return;
    }

    // Drop highlights immediately. Then move (animated), then attack.
    selectedUnitId = null;
    reachableCache = null;
    attackableCache = null;
    renderMatch();
    try {
      const { path } = await window.grid.moveUnit(
        attackerId,
        bestStep.x,
        bestStep.y
      );
      const pathPx = path.map(c => {
        const { cx, cy } = hexCenter(c.x, c.y);
        return { x: cx, y: cy };
      });
      if (pathPx.length >= 2) {
        const hops = pathPx.length - 1;
        const durationMs = Math.max(180, hops * MS_PER_HEX);
        animatingUnits.set(attackerId, {
          pathPx,
          startMs: performance.now(),
          durationMs,
        });
        scheduleAnimFrame();
        // Hold the attack until the move animation completes so the unit
        // is visibly adjacent before the HP drop on the target.
        await new Promise(r => setTimeout(r, durationMs));
      }
      // Snapshot target's current visual state before the RPC, then
      // flash + release after FLASH_MS. Re-fetch from state in case
      // anything changed during the move animation.
      const targetEnt = state.entities.find(en => en.id === targetId);
      const targetUnit = state.units.find(u => u.entityId === targetId);
      if (targetEnt && targetUnit) {
        const dmg = type.attackDmg;
        const killed = targetUnit.currentHp - dmg <= 0;
        flashAttack(
          attackerId,
          {
            entityId: targetId,
            x: targetEnt.x,
            y: targetEnt.y,
            ownerUserId: targetUnit.ownerUserId,
            typeId: targetUnit.typeId,
            currentHp: targetUnit.currentHp,
          },
          dmg,
          killed
        );
      }
      await window.grid.attackUnit(attackerId, targetId);
    } catch (err) {
      toast('err', err.message ?? String(err));
    }
    return;
  }

  // Click empty cell while a unit is selected + cell is reachable -> move
  if (
    selectedUnitId !== null &&
    reachableCache?.entityId === selectedUnitId &&
    reachableCache.cells.has(cellKey(hex.x, hex.y))
  ) {
    const movingId = selectedUnitId;
    // Drop highlights immediately so the player sees the action commit.
    selectedUnitId = null;
    reachableCache = null;
    attackableCache = null;
    renderMatch();
    try {
      const { path } = await window.grid.moveUnit(movingId, hex.x, hex.y);
      // Convert the axial path into pixel centers and lerp through them.
      const pathPx = path.map(c => {
        const { cx, cy } = hexCenter(c.x, c.y);
        return { x: cx, y: cy };
      });
      // path includes the origin cell; need >= 2 points to animate.
      if (pathPx.length >= 2) {
        const hops = pathPx.length - 1;
        animatingUnits.set(movingId, {
          pathPx,
          startMs: performance.now(),
          durationMs: Math.max(180, hops * MS_PER_HEX),
        });
        scheduleAnimFrame();
      }
    } catch (err) {
      toast('err', err.message ?? String(err));
    }
  }
});
