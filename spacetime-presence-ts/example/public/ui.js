import {
  activeRoom,
  activeServer,
  amServerOwner,
  attachmentsByMessage,
  canDeleteMessage,
  canEditMessage,
  globalPresenceBySubject,
  hex,
  latestMessageByRoom,
  messageAuthorName,
  messageById,
  messageSummary,
  myMemberships,
  myReadCursorByRoom,
  myServers,
  roomMembers,
  roomsInServer,
  statusOf,
  threadMessageById,
  threadMessagesForRoot,
  typingForRoom,
  userByHex,
  userByUserId,
} from './chat-model.js';
import { applyChatData, chatState as state } from './chat-state.js';

const $ = id => document.getElementById(id);
const REACTIONS = [
  { id: '+1', label: '\u{1F44D}' },
  { id: 'heart', label: '\u2764\uFE0F' },
  { id: 'joy', label: '\u{1F602}' },
  { id: 'wow', label: '\u{1F62E}' },
  { id: 'sad', label: '\u{1F622}' },
  { id: 'fire', label: '\u{1F525}' },
];

let typingTimer = null;
let typingRenewTimer = null;
let pendingAtts = []; // { id, file, name, mimeType, bytes, previewUrl }
let pendingAttSeq = 0;
let replyTargetId = null;
let editTargetId = null;
let activeThreadRootMessageId = null;
let threadEditTargetId = null;
let composerRateLimitTimer = null;
const ATT_MAX_BYTES = 4_000_000;
const ATT_MAX_COUNT = 5;
const AUTH_TOKEN_KEY = 'chat:auth_token';
const STDB_TOKEN_KEY = 'chat:stdb_token';
const attachmentBlobUrls = new Map();

function fmtBytes(n) {
  if (!Number.isFinite(n)) return '?';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

function authHeaderCandidates() {
  try {
    const tokens = [
      localStorage.getItem(AUTH_TOKEN_KEY),
      localStorage.getItem(STDB_TOKEN_KEY),
    ].filter((token, idx, arr) => token && arr.indexOf(token) === idx);
    return tokens.map(token => ({ authorization: `Bearer ${token}` }));
  } catch {
    return [];
  }
}

async function attachmentBlobUrl(fileId, url) {
  const cacheKey = `file:${fileId}`;
  let blobUrl = attachmentBlobUrls.get(cacheKey);
  if (blobUrl) return blobUrl;
  if (window.chat?.getAttachmentFile) {
    const file = await window.chat.getAttachmentFile(BigInt(fileId));
    blobUrl = URL.createObjectURL(
      new Blob([file.bytes], { type: file.mimeType })
    );
  } else if (url) {
    const attempts = [...authHeaderCandidates(), {}];
    let res = null;
    for (const headers of attempts) {
      res = await fetch(url, { headers, credentials: 'same-origin' });
      if (res.ok) break;
      if (res.status !== 401 && res.status !== 403) break;
    }
    if (!res) throw new Error('no_response');
    if (!res.ok) throw new Error(`http_${res.status}`);
    blobUrl = URL.createObjectURL(await res.blob());
  } else {
    throw new Error('missing_file_url');
  }
  attachmentBlobUrls.set(cacheKey, blobUrl);
  return blobUrl;
}

async function hydrateAttachmentImages(root = document) {
  const imgs = [...root.querySelectorAll('img[data-file-id]')];
  for (const img of imgs) {
    const fileId = img.dataset.fileId;
    const url = img.dataset.fileUrl;
    if (!fileId || img.dataset.loaded === '1') continue;
    img.dataset.loaded = '1';
    try {
      const blobUrl = await attachmentBlobUrl(fileId, url);
      img.src = blobUrl;
      const preview = img.closest('[data-preview-file-id]');
      if (preview) preview.dataset.previewSrc = blobUrl;
    } catch (err) {
      img.dataset.loaded = '0';
      img.classList.add('broken');
      img.alt = `${img.alt || 'attachment'} (failed to load)`;
      console.warn('attachment image load failed', url, err);
    }
  }
}

function closeImageLightbox() {
  const box = $('imageLightbox');
  box.classList.remove('open');
  $('imageLightboxImg').removeAttribute('src');
  $('imageLightboxCaption').textContent = '';
}

function showImageLightbox(src, name) {
  $('imageLightboxImg').src = src;
  $('imageLightboxImg').alt = name || 'attachment';
  $('imageLightboxCaption').textContent = name || '';
  $('imageLightbox').classList.add('open');
  $('imageLightboxClose').focus();
}

async function openAttachmentPreview(button) {
  const fileId = button.dataset.previewFileId;
  const url = button.dataset.previewUrl;
  const name = button.dataset.previewName || 'attachment';
  if (!fileId) return;
  try {
    const src =
      button.dataset.previewSrc || (await attachmentBlobUrl(fileId, url));
    button.dataset.previewSrc = src;
    showImageLightbox(src, name);
  } catch (err) {
    console.warn('attachment preview failed', err);
    setResult('Image preview failed.', false);
  }
}

document.addEventListener('click', e => {
  const button = e.target.closest('[data-preview-file-id]');
  if (!button) return;
  e.preventDefault();
  openAttachmentPreview(button);
});
$('imageLightboxClose').addEventListener('click', closeImageLightbox);
$('imageLightbox').addEventListener('click', e => {
  if (e.target === $('imageLightbox')) closeImageLightbox();
});
document.addEventListener('keydown', e => {
  if (e.key === 'Escape' && $('imageLightbox').classList.contains('open')) {
    closeImageLightbox();
  }
});

function focusAuthPanel() {
  document.querySelector('#auth-panel input')?.focus();
}

function setResult(text, ok = true) {
  if (!text) return;
  const host =
    document.getElementById('toastHost') ||
    (() => {
      const d = document.createElement('div');
      d.id = 'toastHost';
      d.className = 'toast-host';
      document.body.appendChild(d);
      return d;
    })();
  const el = document.createElement('div');
  el.className = 'toast' + (ok ? '' : ' error');
  el.textContent = text;
  host.appendChild(el);
  setTimeout(() => {
    el.classList.add('hide');
    setTimeout(() => el.remove(), 250);
  }, 2200);
}
function reportAsync(label, promise) {
  Promise.resolve(promise).catch(err =>
    setResult(`${label} failed: ${err.message ?? err}`, false)
  );
}
function resetAtMs(row) {
  return Number(row.resetAt.microsSinceUnixEpoch) / 1000;
}
function currentSendLimit() {
  if (!state.authenticated || !activeRoom()) return null;
  const row = (state.rateLimitStatus || []).find(
    r => r.scope === 'chat.send_message'
  );
  if (!row || row.remaining > 0) return null;
  const remainingSeconds = Math.ceil((resetAtMs(row) - Date.now()) / 1000);
  return remainingSeconds > 0 ? { ...row, remainingSeconds } : null;
}
function renderComposerRateLimit() {
  const el = $('composerRateLimit');
  if (!el) return;
  const limited = currentSendLimit();
  el.hidden = !limited;
  el.textContent = limited
    ? `Rate limited. Try again in ${limited.remainingSeconds}s.`
    : '';
}
function updateRateLimitTicker() {
  const limited = currentSendLimit();
  if (limited && !composerRateLimitTimer) {
    composerRateLimitTimer = setInterval(updateComposerState, 250);
  } else if (!limited && composerRateLimitTimer) {
    clearInterval(composerRateLimitTimer);
    composerRateLimitTimer = null;
  }
}
function renderUserBar() {
  if (!state.authenticated) return;
  const users = userByHex();
  const me = users.get(state.meHex);
  const fallback =
    state.userEmail || (state.meHex ? state.meHex.slice(-6) : '');
  const label = me?.displayName || fallback;
  const avatarSeed = label || '?';
  $('userBarName').textContent = label;
  $('userAvatar').textContent = (avatarSeed[0] || '?').toUpperCase();
  const dn = $('displayName');
  if (dn && document.activeElement !== dn) dn.value = me?.displayName || '';
}

function setAuthedUi(user) {
  const openBtn = $('openAuthBtn');
  const userBar = $('openUserMenuBtn');
  const shell = document.querySelector('.shell');
  const app = document.querySelector('.app');
  if (user) {
    openBtn.classList.add('hidden');
    userBar.classList.remove('hidden');
    state.userEmail = user.email || '';
    renderUserBar();
    shell.classList.remove('signed-out');
    app.classList.remove('signed-out');
  } else {
    openBtn.classList.remove('hidden');
    userBar.classList.add('hidden');
    $('userBarName').textContent = '';
    $('userAvatar').textContent = '';
    state.userEmail = '';
    shell.classList.add('signed-out');
    app.classList.add('signed-out');
    closeUserMenu();
  }
}
function updateComposerState() {
  const room = activeRoom();
  const enabled = state.authenticated && Boolean(room);
  const sendLimited = Boolean(currentSendLimit());
  $('messageInput').disabled = !enabled || sendLimited;
  $('sendBtn').disabled = !enabled || sendLimited;
  $('sendBtn').title = sendLimited ? 'Rate limited' : 'Send';
  $('attachBtn').disabled = !enabled || sendLimited;
  $('composerRow').classList.toggle('rate-limited', sendLimited);
  $('threadInput').disabled = sendLimited;
  $('threadComposer').querySelector('button').disabled = sendLimited;
  $('saveProfileBtn').disabled = !state.authenticated;
  $('displayName').disabled = !state.authenticated;
  $('status').disabled = !state.authenticated;
  $('typingLine').classList.toggle('hidden', !state.authenticated);
  $('composerRow').classList.toggle('hidden', !state.authenticated);
  if (!enabled) clearComposerTarget();
  else renderComposerReply();
  $('toggleMembersBtn').classList.toggle('hidden', !room);
  $('membersPanel').classList.toggle('no-room', !room);
  if (!state.authenticated)
    $('messageInput').placeholder = 'Sign in to send messages';
  else if (!room) $('messageInput').placeholder = 'Select a channel';
  else $('messageInput').placeholder = `Message #${room.name}`;
  renderComposerRateLimit();
  updateRateLimitTicker();
}
function requireAuthAction() {
  if (state.authenticated) return true;
  focusAuthPanel();
  setResult('Sign in to continue.', false);
  return false;
}
function microsToDate(micros) {
  return new Date(Number(micros) / 1000);
}
function fmtTime(ts) {
  return microsToDate(ts.microsSinceUnixEpoch).toLocaleTimeString();
}
function editIcon() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="16" height="16"><path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"/></svg>';
}
function deleteIcon() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="16" height="16"><path d="M3 6h18"/><path d="M8 6V4h8v2"/><path d="M19 6l-1 14H6L5 6"/></svg>';
}
function renderReplyReference(message, users) {
  const parentId = message.replyToMessageId;
  if (parentId === undefined || parentId === null) return '';
  const parent = messageById(parentId);
  return `<button type="button" class="reply-reference" data-jump-message="${parentId.toString()}">
    <span class="reply-author">${escapeHtml(messageAuthorName(parent, users))}</span>
    <span class="reply-text">${escapeHtml(messageSummary(parent))}</span>
  </button>`;
}
function renderComposerReply() {
  const preview = $('replyPreview');
  if (editTargetId !== null) {
    const target = messageById(editTargetId);
    const valid = Boolean(
      target && activeRoom() && target.roomId === activeRoom().id
    );
    preview.classList.toggle('hidden', !valid);
    preview.classList.toggle('editing', true);
    if (valid) {
      $('replyPreviewLabel').textContent = 'Editing message';
      $('replyPreviewText').textContent = messageSummary(target);
    } else {
      editTargetId = null;
    }
    return;
  }
  preview.classList.toggle('editing', false);
  const target = replyTargetId === null ? null : messageById(replyTargetId);
  const room = activeRoom();
  const valid = Boolean(target && room && target.roomId === room.id);
  preview.classList.toggle('hidden', !valid);
  if (!valid) return;
  $('replyPreviewLabel').textContent =
    `Replying to ${messageAuthorName(target)}`;
  $('replyPreviewText').textContent = messageSummary(target);
}
function setReplyTarget(messageId) {
  const target = messageById(messageId);
  const room = activeRoom();
  if (!target || !room || target.roomId !== room.id) return;
  clearEditTarget();
  replyTargetId = messageId;
  renderComposerReply();
  $('messageInput').focus();
}
function clearReplyTarget() {
  replyTargetId = null;
  $('replyPreview').classList.remove('editing');
  $('replyPreview').classList.add('hidden');
}
function setEditTarget(messageId) {
  const target = messageById(messageId);
  const room = activeRoom();
  if (!target || !room || target.roomId !== room.id || !canEditMessage(target))
    return;
  clearReplyTarget();
  clearPendingAtts();
  editTargetId = messageId;
  $('messageInput').value = target.content || '';
  renderComposerReply();
  $('messageInput').focus();
}
function clearEditTarget() {
  editTargetId = null;
  $('replyPreview').classList.remove('editing');
  $('replyPreview').classList.add('hidden');
}
function clearComposerTarget() {
  clearReplyTarget();
  clearEditTarget();
}
function closeThreadPanel() {
  activeThreadRootMessageId = null;
  clearThreadEditTarget();
  $('threadPanel').hidden = true;
}
function openThread(rootMessageId) {
  const root = messageById(rootMessageId);
  const room = activeRoom();
  if (!root || !room || root.roomId !== room.id) return;
  activeThreadRootMessageId = rootMessageId;
  closePinnedPanel();
  closeSearchPanel();
  renderThreadPanel();
  $('threadPanel').hidden = false;
  $('threadInput').focus();
}
function renderThreadPanel() {
  const panel = $('threadPanel');
  if (activeThreadRootMessageId === null) {
    panel.hidden = true;
    return;
  }
  const root = messageById(activeThreadRootMessageId);
  const room = activeRoom();
  if (!root || !room || root.roomId !== room.id) {
    closeThreadPanel();
    return;
  }
  const users = userByHex();
  const threadMessages = threadMessagesForRoot(root.id);
  if (threadEditTargetId !== null && !threadMessageById(threadEditTargetId))
    clearThreadEditTarget();
  $('threadHead').textContent = `Thread in #${room.name}`;
  $('threadRoot').innerHTML = `
    <div class="thread-root-label">Original message</div>
    <div class="overlay-msg-head">
      <span class="overlay-msg-author">${escapeHtml(messageAuthorName(root, users))}</span>
      <span class="overlay-msg-time">${fmtTime(root.createdAt)}</span>
    </div>
    <div class="overlay-msg-body">${escapeHtml(messageSummary(root))}</div>
  `;
  $('threadList').innerHTML =
    threadMessages.length === 0
      ? '<li class="overlay-empty">No thread replies yet.</li>'
      : threadMessages
          .map(
            m => `
      <li class="thread-msg" data-thread-message-id="${m.id.toString()}">
        <div class="thread-msg-head">
          <span class="thread-msg-author">${escapeHtml(messageAuthorName(m, users))}</span>
          <span class="thread-msg-time">${fmtTime(m.createdAt)}</span>
          ${m.editedAt ? '<span class="edited-tag">(edited)</span>' : ''}
          ${
            canEditMessage(m) || canDeleteMessage(m)
              ? `<span class="thread-msg-actions">
            ${canEditMessage(m) ? `<button type="button" data-thread-edit="${m.id.toString()}" title="Edit" aria-label="Edit">${editIcon()}</button>` : ''}
            ${canDeleteMessage(m) ? `<button type="button" data-thread-delete="${m.id.toString()}" class="danger" title="Delete" aria-label="Delete">${deleteIcon()}</button>` : ''}
          </span>`
              : ''
          }
        </div>
        <div class="thread-msg-body">${escapeHtml(m.content)}</div>
      </li>
    `
          )
          .join('');
  $('threadList')
    .querySelectorAll('[data-thread-edit]')
    .forEach(btn => {
      btn.addEventListener('click', () =>
        setThreadEditTarget(BigInt(btn.dataset.threadEdit))
      );
    });
  $('threadList')
    .querySelectorAll('[data-thread-delete]')
    .forEach(btn => {
      btn.addEventListener('click', async () => {
        if (!confirm('Delete this thread message?')) return;
        try {
          await window.chat.deleteThreadMessage(
            BigInt(btn.dataset.threadDelete)
          );
        } catch (err) {
          setResult(`thread delete failed: ${err.message ?? err}`, false);
        }
      });
    });
}
function setThreadEditTarget(messageId) {
  const target = threadMessageById(messageId);
  if (!target || !canEditMessage(target)) return;
  threadEditTargetId = messageId;
  $('threadInput').value = target.content || '';
  $('threadInput').placeholder = 'Edit thread message';
  $('threadInput').focus();
}
function clearThreadEditTarget() {
  threadEditTargetId = null;
  const input = $('threadInput');
  if (input) {
    input.value = '';
    input.placeholder = 'Reply in thread';
  }
}
function bindRoomActions(container) {
  container.querySelectorAll('[data-room-id]').forEach(node => {
    node.addEventListener('click', async e => {
      const target = e.target.closest('[data-action]') || e.target;
      const roomId = BigInt(node.dataset.roomId);
      if (target?.dataset?.action === 'leave') {
        if (!requireAuthAction()) return;
        try {
          await window.chat.leaveRoom(roomId);
          if (state.activeRoomId === roomId) {
            state.activeRoomId = null;
            clearComposerTarget();
            closeThreadPanel();
            window.chat.setActiveRoom(null);
          }
        } catch (err) {
          console.error('leaveRoom failed', err);
          setResult(`leave failed: ${err.message ?? err}`, false);
        }
        return;
      }
      if (target?.dataset?.action === 'settings') {
        e.stopPropagation();
        if (!requireAuthAction()) return;
        openChannelSettings(roomId);
        return;
      }
      if (!requireAuthAction()) return;
      const joined = new Set(myMemberships().map(x => x.toString()));
      if (!joined.has(roomId.toString())) {
        try {
          await window.chat.joinRoom(roomId);
        } catch (err) {
          console.error('joinRoom failed', err);
          setResult(`open failed: ${err.message ?? err}`, false);
          return;
        }
      }
      if (state.activeRoomId && state.activeRoomId !== roomId) {
        stopTypingNow();
        clearComposerTarget();
        closeThreadPanel();
      }
      state.activeRoomId = roomId;
      window.chat.setActiveRoom(roomId);
      try {
        await window.chat.markRoomRead(roomId);
      } catch (err) {
        console.warn('markRoomRead failed', err);
      }
      renderAll();
    });
  });
}

function renderMessageAttachments(
  messageId,
  groupedAtts = attachmentsByMessage()
) {
  const list = groupedAtts.get(messageId);
  if (!list || list.length === 0) return '';
  return `<div class="msg-attachments">${list
    .map(a => {
      const isImg = a.mimeType.startsWith('image/');
      const url = `/files?id=${encodeURIComponent(a.fileId.toString())}`;
      const fname = a.filename || `attachment-${a.id.toString()}`;
      if (isImg) {
        return `<button type="button" class="msg-att-preview" data-preview-file-id="${a.fileId.toString()}" data-preview-url="${url}" data-preview-name="${escapeHtml(fname)}" aria-label="Preview ${escapeHtml(fname)}"><img class="msg-att-img" data-file-id="${a.fileId.toString()}" data-file-url="${url}" alt="${escapeHtml(fname)}"></button>`;
      }
      return `<a class="msg-att-file" href="${url}" download="${escapeHtml(fname)}" target="_blank" rel="noopener">
      <span>${escapeHtml(fname)}</span>
      <span class="msg-att-size">${fmtBytes(Number(a.size))}</span>
    </a>`;
    })
    .join('')}</div>`;
}

function hueFromString(s) {
  let h = 0;
  for (let i = 0; i < s.length; i++) {
    h = (h * 31 + s.charCodeAt(i)) | 0;
  }
  return Math.abs(h) % 360;
}
function avatarSwatch(seed, label, sizePx) {
  const hue = hueFromString(seed || '?');
  const text = (label || '?').trim().charAt(0).toUpperCase() || '?';
  return `<span class="avatar" style="--av-bg: hsl(${hue} 60% 42%); --av-size: ${sizePx}px;">${escapeHtml(text)}</span>`;
}

function renderServerRail() {
  const rail = $('serverRail');
  if (!rail) return;
  const mine = myServers();
  rail.innerHTML = mine
    .map(s => {
      const initials = (s.name || '?').trim().slice(0, 2).toUpperCase() || '?';
      const isActive = state.activeServerId === s.id;
      return `<li><button class="server-pill${isActive ? ' active' : ''}" data-server-id="${s.id.toString()}" title="${escapeHtml(s.name)}">${escapeHtml(initials)}</button></li>`;
    })
    .join('');
  rail.querySelectorAll('[data-server-id]').forEach(btn => {
    btn.addEventListener('click', () => {
      const id = BigInt(btn.dataset.serverId);
      if (state.activeServerId === id) return;
      window.chat.setActiveServer(id);
    });
  });
}

function renderSidebarHead() {
  const srv = activeServer();
  const nameEl = $('activeServerName');
  if (nameEl)
    nameEl.textContent = srv
      ? srv.name
      : state.authenticated && myServers().length === 0
        ? 'No server'
        : 'SpacetimeDB';
  $('sidebarHead').classList.toggle('hidden', !state.authenticated);
}

function renderRooms() {
  const roomList = $('roomList');
  const latestByRoom = latestMessageByRoom();
  const myCursor = myReadCursorByRoom();

  const visibleRooms =
    state.activeServerId !== null ? roomsInServer(state.activeServerId) : [];

  const grouped = new Map();
  for (const r of visibleRooms) {
    const cat =
      r.category && r.category.trim() ? r.category.trim() : 'Text Channels';
    if (!grouped.has(cat)) grouped.set(cat, []);
    grouped.get(cat).push(r);
  }
  const catNames = [...grouped.keys()].sort((a, b) => {
    if (a === 'Text Channels') return -1;
    if (b === 'Text Channels') return 1;
    return a.localeCompare(b);
  });

  const isAdmin =
    (state.admins || []).length === 0 ||
    (state.admins || []).includes(state.meHex);
  const renderRow = r => {
    const latest = latestByRoom.get(r.id);
    const read = myCursor.get(r.id) ?? 0n;
    const unread = latest ? latest.id > read : false;
    const lockIcon = r.isPrivate
      ? '<span class="lock-icon" title="private">🔒</span>'
      : '';
    const ownsRoom = r.createdByUserId === state.userId;
    const canEdit = ownsRoom || isAdmin;
    const gearSvg =
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="13" height="13"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>';
    const settingsBtn = canEdit
      ? `<button class="btn tiny ghost" data-action="settings" title="Channel settings" aria-label="Channel settings">${gearSvg}</button>`
      : '';
    const leaveBtn = !canEdit
      ? '<button class="btn tiny ghost" data-action="leave" title="Leave channel">×</button>'
      : '';
    const action = `${settingsBtn}${leaveBtn}`;
    const badge = unread ? '<span class="unread-dot"></span>' : '';
    return `<li class="room-row${state.activeRoomId === r.id ? ' active' : ''}${unread ? ' has-unread' : ''}" data-room-id="${r.id.toString()}" data-action="open">
      <span class="channel-hash">#</span>
      <span class="room-name">${escapeHtml(r.name)}</span>
      ${lockIcon}${badge}
      <span class="row-actions">${action}</span>
    </li>`;
  };

  if (state.activeServerId === null) {
    if (myServers().length === 0) {
      roomList.innerHTML = `<div class="sidebar-empty">
        <p>No servers yet.</p>
        <button type="button" class="btn primary block" id="sidebarCreateFirstServer">Create your first server</button>
      </div>`;
      $('sidebarCreateFirstServer')?.addEventListener(
        'click',
        openCreateServerModal
      );
    } else {
      roomList.innerHTML = `<div class="sidebar-empty"><p>Pick a server on the left.</p></div>`;
    }
    return;
  }
  if (visibleRooms.length === 0) {
    roomList.innerHTML = `<div class="sidebar-empty">
      <p>No channels in this server yet.</p>
      <button type="button" class="btn primary block" id="sidebarCreateFirst">Create a channel</button>
    </div>`;
    $('sidebarCreateFirst')?.addEventListener('click', openCreateRoomModal);
    return;
  }

  const sections = catNames
    .map(cat => {
      const rows = grouped.get(cat).map(renderRow).join('');
      return `<details class="category" open>
      <summary class="category-header">
        <span class="category-chevron"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg></span>
        <span class="category-name">${escapeHtml(cat.toUpperCase())}</span>
        <button type="button" class="category-add" title="Create channel" aria-label="Create channel">+</button>
      </summary>
      <ul class="category-rooms">${rows}</ul>
    </details>`;
    })
    .join('');

  roomList.innerHTML = sections;
  bindRoomActions(roomList);
}

function renderMessages() {
  const room = activeRoom();
  const ul = $('messageList');
  if (!state.authenticated) {
    ul.innerHTML = '';
    return;
  }
  if (!room) {
    const mine = myServers();
    if (mine.length === 0) {
      ul.innerHTML = `<li class="empty-state"><h3>Welcome</h3><p>Create your first server to start chatting.</p><button type="button" class="btn primary" id="emptyCreateBtn">Create a server</button></li>`;
      $('emptyCreateBtn')?.addEventListener('click', openCreateServerModal);
      $('roomTitle').textContent = '';
    } else if (state.activeServerId === null) {
      ul.innerHTML = `<li class="empty-state"><h3>Pick a server</h3><p>Choose a server on the left.</p></li>`;
      $('roomTitle').textContent = '';
    } else if (roomsInServer(state.activeServerId).length === 0) {
      ul.innerHTML = `<li class="empty-state"><h3>No channels yet</h3><p>Create a channel to start chatting in this server.</p><button type="button" class="btn primary" id="emptyCreateRoomBtn">Create a channel</button></li>`;
      $('emptyCreateRoomBtn')?.addEventListener('click', openCreateRoomModal);
      $('roomTitle').textContent = '';
    } else {
      ul.innerHTML = `<li class="empty-state"><h3>No channel selected</h3><p>Pick a channel on the left to start.</p></li>`;
      $('roomTitle').textContent = 'Select a channel';
    }
    const hash = document.getElementById('channelHashIcon');
    if (hash) hash.hidden = true;
    return;
  }
  const hashEl = document.getElementById('channelHashIcon');
  if (hashEl) hashEl.hidden = false;
  $('roomTitle').textContent = room.name;
  const users = userByHex();
  const roomMessages = state.messages
    .filter(m => m.roomId === room.id)
    .slice(-300);
  if (roomMessages.length === 0) {
    ul.innerHTML = `<li class="empty-state"><h3>Welcome to #${escapeHtml(room.name)}!</h3><p>This is the start of the #${escapeHtml(room.name)} channel.</p></li>`;
    return;
  }

  const groupedReactions = new Map();
  for (const r of state.reactions) {
    let byEmoji = groupedReactions.get(r.messageId);
    if (!byEmoji) {
      byEmoji = new Map();
      groupedReactions.set(r.messageId, byEmoji);
    }
    let entry = byEmoji.get(r.emoji);
    if (!entry) {
      entry = { count: 0, mine: false };
      byEmoji.set(r.emoji, entry);
    }
    entry.count++;
    if (hex(r.identity) === state.meHex) entry.mine = true;
  }

  const groupedAtts = attachmentsByMessage();
  const CHUNK_GAP_MICROS = 5n * 60n * 1_000_000n; // 5 minutes
  const chipMap = new Map(REACTIONS.map(x => [x.id, x.label]));

  let prev = null;
  const html = roomMessages
    .map(m => {
      const authorHex = hex(m.author);
      const author = users.get(authorHex);
      const name = author?.displayName || authorHex.slice(-6);

      const sameAuthor = prev && hex(prev.author) === authorHex;
      const gap = prev
        ? m.createdAt.microsSinceUnixEpoch - prev.createdAt.microsSinceUnixEpoch
        : null;
      const chunkStart =
        !sameAuthor || (gap !== null && gap > CHUNK_GAP_MICROS);
      prev = m;

      const byEmoji = groupedReactions.get(m.id) || new Map();
      const chips = [...byEmoji.entries()]
        .map(
          ([emoji, meta]) =>
            `<button class="react-btn${meta.mine ? ' mine' : ''}" data-react="${emoji}" data-mid="${m.id.toString()}">${chipMap.get(emoji) || emoji} <span>${meta.count}</span></button>`
        )
        .join('');
      const reactionRow = chips ? `<div class="react-row">${chips}</div>` : '';
      const mine = new Set(
        [...byEmoji.entries()]
          .filter(([, meta]) => meta.mine)
          .map(([emoji]) => emoji)
      );
      const quickReactions = REACTIONS.filter(r => !mine.has(r.id))
        .map(
          r =>
            `<button class="toolbar-react" data-react="${r.id}" data-mid="${m.id.toString()}" title="${r.id}" aria-label="React ${r.id}">${r.label}</button>`
        )
        .join('');
      const editedTag = m.editedAt
        ? ' <span class="edited-tag">(edited)</span>'
        : '';
      const replyRef = renderReplyReference(m, users);

      const isPinned = m.pinnedAt !== undefined && m.pinnedAt !== null;
      const pinIcon = isPinned ? 'active' : '';
      const pinTitle = isPinned ? 'Unpin' : 'Pin';
      const threadMessages = threadMessagesForRoot(m.id);
      const hasThread = threadMessages.length > 0;
      const threadPill = hasThread
        ? `<button type="button" class="thread-pill" data-thread="${m.id.toString()}">${threadMessages.length} thread ${threadMessages.length === 1 ? 'reply' : 'replies'}</button>`
        : '';
      const replySvg =
        '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="16" height="16"><polyline points="9 17 4 12 9 7"/><path d="M20 18v-2a4 4 0 0 0-4-4H4"/></svg>';
      const threadSvg =
        '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="16" height="16"><path d="M21 15a4 4 0 0 1-4 4H8l-5 3V7a4 4 0 0 1 4-4h10a4 4 0 0 1 4 4z"/><path d="M8 9h8"/><path d="M8 13h5"/></svg>';
      const pinSvg =
        '<svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16"><path d="M16 9V4h1a1 1 0 0 0 0-2H7a1 1 0 0 0 0 2h1v5l-3 3v2h6v6h2v-6h6v-2z"/></svg>';
      const toolbarSep = quickReactions
        ? '<span class="msg-toolbar-sep"></span>'
        : '';
      const toolbar = `<div class="msg-toolbar">
      ${quickReactions}
      ${toolbarSep}
      <button data-reply="${m.id.toString()}" title="Reply" aria-label="Reply">${replySvg}</button>
      <button data-thread="${m.id.toString()}" class="${hasThread ? 'has-thread' : ''}" title="Open thread" aria-label="Open thread">${threadSvg}</button>
      <button data-pin="${m.id.toString()}" class="${pinIcon}" title="${pinTitle}" aria-label="${pinTitle}">${pinSvg}</button>
      ${canEditMessage(m) ? `<button data-edit="${m.id.toString()}" title="Edit" aria-label="Edit">${editIcon()}</button>` : ''}
      ${canDeleteMessage(m) ? `<button data-delete="${m.id.toString()}" title="Delete" aria-label="Delete">${deleteIcon()}</button>` : ''}
    </div>`;
      const pinnedMarker = isPinned
        ? `<span class="pinned-marker">${pinSvg}pinned</span>`
        : '';

      if (chunkStart) {
        return `<li class="msg chunk-start${isPinned ? ' pinned' : ''}" data-message-id="${m.id.toString()}">
        ${avatarSwatch(authorHex, name, 36)}
        <div class="msg-col">
          <div class="msg-head">
            <span class="msg-author">${escapeHtml(name)}</span>
            <span class="msg-time">${fmtTime(m.createdAt)}</span>
            ${pinnedMarker}
          </div>
          ${replyRef}
          <div class="msg-body">${escapeHtml(m.content)}${editedTag}</div>
          ${renderMessageAttachments(m.id, groupedAtts)}
          ${reactionRow}${threadPill}
        </div>
        ${toolbar}
      </li>`;
      }
      return `<li class="msg chunk-cont${isPinned ? ' pinned' : ''}" data-message-id="${m.id.toString()}">
      <span class="msg-stamp-rail" title="${fmtTime(m.createdAt)}"></span>
      <div class="msg-col">
        ${replyRef}
        <div class="msg-body">${escapeHtml(m.content)}${editedTag}${pinnedMarker}</div>
        ${renderMessageAttachments(m.id, groupedAtts)}
        ${reactionRow}${threadPill}
      </div>
      ${toolbar}
    </li>`;
    })
    .join('');
  ul.innerHTML = html;
  hydrateAttachmentImages(ul);

  ul.querySelectorAll('[data-react]').forEach(btn => {
    btn.addEventListener('click', () => {
      if (!requireAuthAction()) return;
      const mid = BigInt(btn.dataset.mid);
      const emoji = btn.dataset.react;
      try {
        reportAsync('reaction', window.chat.toggleReaction(mid, emoji));
      } catch (err) {
        setResult(`reaction failed: ${err.message ?? err}`, false);
      }
    });
  });
  ul.querySelectorAll('[data-reply]').forEach(btn => {
    btn.addEventListener('click', () => {
      if (!requireAuthAction()) return;
      setReplyTarget(BigInt(btn.dataset.reply));
    });
  });
  ul.querySelectorAll('[data-thread]').forEach(btn => {
    btn.addEventListener('click', () => {
      if (!requireAuthAction()) return;
      openThread(BigInt(btn.dataset.thread));
    });
  });
  ul.querySelectorAll('[data-jump-message]').forEach(btn => {
    btn.addEventListener('click', () => {
      const target = ul.querySelector(
        `[data-message-id="${btn.dataset.jumpMessage}"]`
      );
      if (target)
        target.scrollIntoView({ block: 'center', behavior: 'smooth' });
    });
  });
  ul.querySelectorAll('[data-pin]').forEach(btn => {
    btn.addEventListener('click', () => {
      if (!requireAuthAction()) return;
      const mid = BigInt(btn.dataset.pin);
      const isPinned = btn.classList.contains('active');
      try {
        if (isPinned) reportAsync('unpin', window.chat.unpinMessage(mid));
        else reportAsync('pin', window.chat.pinMessage(mid));
      } catch (err) {
        setResult(`pin failed: ${err.message ?? err}`, false);
      }
    });
  });
  ul.querySelectorAll('[data-edit]').forEach(btn => {
    btn.addEventListener('click', () => {
      if (!requireAuthAction()) return;
      setEditTarget(BigInt(btn.dataset.edit));
    });
  });
  ul.querySelectorAll('[data-delete]').forEach(btn => {
    btn.addEventListener('click', async () => {
      if (!requireAuthAction()) return;
      if (!confirm('Delete this message?')) return;
      try {
        await window.chat.deleteMessage(BigInt(btn.dataset.delete));
      } catch (err) {
        setResult(`delete failed: ${err.message ?? err}`, false);
      }
    });
  });
}

function closePinnedPanel() {
  $('pinnedPanel').hidden = true;
}
function closeSearchPanel() {
  $('searchPanel').hidden = true;
  $('searchList').innerHTML = '';
}
function renderPinnedPanel() {
  const room = activeRoom();
  const list = $('pinnedList');
  if (!room) {
    list.innerHTML = '<li class="overlay-empty">No channel selected.</li>';
    return;
  }
  const users = userByHex();
  const pinned = state.messages
    .filter(m => m.roomId === room.id && m.pinnedAt)
    .sort((a, b) =>
      a.pinnedAt.microsSinceUnixEpoch < b.pinnedAt.microsSinceUnixEpoch ? 1 : -1
    );
  if (pinned.length === 0) {
    list.innerHTML =
      '<li class="overlay-empty">No pinned messages in this channel.</li>';
    return;
  }
  const groupedAtts = attachmentsByMessage();
  list.innerHTML = pinned
    .map(m => {
      const author = users.get(hex(m.author));
      const name = author?.displayName || hex(m.author).slice(-6);
      return `<li class="overlay-msg" data-mid="${m.id.toString()}">
      <div class="overlay-msg-head">
        <span class="overlay-msg-author">${escapeHtml(name)}</span>
        <span class="overlay-msg-time">${fmtTime(m.createdAt)}</span>
      </div>
      <div class="overlay-msg-body">${escapeHtml(m.content)}</div>
      ${renderMessageAttachments(m.id, groupedAtts)}
    </li>`;
    })
    .join('');
  hydrateAttachmentImages(list);
}
$('togglePinnedBtn').addEventListener('click', () => {
  if ($('pinnedPanel').hidden) {
    closeSearchPanel();
    renderPinnedPanel();
    $('pinnedPanel').hidden = false;
  } else {
    closePinnedPanel();
  }
});
$('closePinnedBtn').addEventListener('click', closePinnedPanel);

$('searchForm').addEventListener('submit', async e => {
  e.preventDefault();
  if (!requireAuthAction()) return;
  const room = activeRoom();
  if (!room) return;
  const q = $('searchInput').value.trim();
  if (!q) {
    closeSearchPanel();
    return;
  }
  closePinnedPanel();
  $('searchHead').textContent =
    `Search: "${q.length > 30 ? q.slice(0, 30) + '…' : q}"`;
  $('searchList').innerHTML = '<li class="overlay-empty">Searching…</li>';
  $('searchPanel').hidden = false;
  try {
    const results = await window.chat.searchMessages(room.id, q);
    const users = userByHex();
    if (!results || results.length === 0) {
      $('searchList').innerHTML = '<li class="overlay-empty">No matches.</li>';
      return;
    }
    $('searchList').innerHTML = results
      .map(m => {
        const author = users.get(hex(m.author));
        const name = author?.displayName || hex(m.author).slice(-6);
        return `<li class="overlay-msg" data-mid="${m.id.toString()}">
        <div class="overlay-msg-head">
          <span class="overlay-msg-author">${escapeHtml(name)}</span>
          <span class="overlay-msg-time">${fmtTime(m.createdAt)}</span>
        </div>
        <div class="overlay-msg-body">${escapeHtml(m.content)}</div>
      </li>`;
      })
      .join('');
  } catch (err) {
    $('searchList').innerHTML =
      `<li class="overlay-empty">Error: ${escapeHtml(err.message ?? String(err))}</li>`;
  }
});
$('closeSearchBtn').addEventListener('click', closeSearchPanel);

function renderTyping() {
  const room = activeRoom();
  if (!room) {
    $('typingLine').textContent = '';
    return;
  }
  const users = userByHex();
  const typing = typingForRoom(room.id)
    .filter(id => id !== state.meHex)
    .map(id => users.get(id)?.displayName || id.slice(-6));
  if (typing.length === 0) $('typingLine').textContent = '';
  else if (typing.length === 1)
    $('typingLine').textContent = `${typing[0]} is typing...`;
  else $('typingLine').textContent = `${typing.length} people are typing...`;
}

function renderMembers() {
  const room = activeRoom();
  const list = $('memberList');
  if (!state.authenticated) {
    list.innerHTML = '<li class="empty">Sign in to view members.</li>';
    return;
  }
  if (!room) {
    list.innerHTML = '<li class="empty">Select a channel.</li>';
    return;
  }
  const presenceMap = globalPresenceBySubject();
  const usersByUid = userByUserId();
  const rows = roomMembers(room.id)
    .map(m => {
      const user = usersByUid.get(m.userId);
      const h = user ? hex(user.identity) : '';
      const status = statusOf(h, presenceMap);
      return {
        hex: h,
        name: user?.displayName || m.userId.slice(-6),
        status,
        mine: m.userId === state.userId,
      };
    })
    .sort((a, b) => a.name.localeCompare(b.name));

  const online = rows.filter(
    m => m.status !== 'invisible' && m.status !== 'offline'
  );
  const offline = rows.filter(
    m => m.status === 'invisible' || m.status === 'offline'
  );

  const renderRow =
    m => `<li class="member-row${m.status === 'invisible' || m.status === 'offline' ? ' offline' : ''}">
    <span class="member-avatar-wrap">
      ${avatarSwatch(m.hex, m.name, 32)}
      <span class="presence-pip ${m.status}"></span>
    </span>
    <span class="member-name">${escapeHtml(m.name)}${m.mine ? " <span class='member-you'>(you)</span>" : ''}</span>
  </li>`;

  const sections = [];
  if (online.length)
    sections.push(
      `<li class="member-section">Online (${online.length})</li>`,
      ...online.map(renderRow)
    );
  if (offline.length)
    sections.push(
      `<li class="member-section">Offline (${offline.length})</li>`,
      ...offline.map(renderRow)
    );
  list.innerHTML = sections.join('');
  $('memberCount').textContent = `${rows.length}`;
}

function scrollMessagesToBottom() {
  const node = $('messageScroll');
  node.scrollTop = node.scrollHeight;
}
function renderAll() {
  renderServerRail();
  renderSidebarHead();
  renderRooms();
  renderMessages();
  renderTyping();
  renderMembers();
  renderUserBar();
  updateComposerState();
  renderThreadPanel();
  if (!$('pinnedPanel').hidden) renderPinnedPanel();
}
function escapeHtml(s) {
  return String(s ?? '').replace(
    /[&<>"']/g,
    c =>
      ({
        '&': '&amp;',
        '<': '&lt;',
        '>': '&gt;',
        '"': '&quot;',
        "'": '&#39;',
      })[c]
  );
}

function scheduleTyping() {
  if (!state.authenticated) return;
  const room = activeRoom();
  if (!room) return;
  try {
    void window.chat.startTyping(room.id).catch(() => {});
  } catch {
    // The connection can close while the typing update is queued.
  }
  if (typingTimer) clearTimeout(typingTimer);
  if (typingRenewTimer) clearInterval(typingRenewTimer);
  typingTimer = setTimeout(() => {
    try {
      void window.chat.stopTyping(room.id).catch(() => {});
    } catch {
      // The connection can close before the timer fires.
    }
  }, 1800);
  typingRenewTimer = setInterval(() => {
    try {
      void window.chat.startTyping(room.id).catch(() => {});
    } catch {
      // The connection can close while the typing state is renewed.
    }
  }, 1500);
}

function stopTypingNow() {
  const room = activeRoom();
  if (!room) return;
  if (typingTimer) clearTimeout(typingTimer);
  if (typingRenewTimer) clearInterval(typingRenewTimer);
  typingTimer = null;
  typingRenewTimer = null;
  try {
    void window.chat.stopTyping(room.id).catch(() => {});
  } catch {
    // The connection can close while the typing state is cleared.
  }
}

function dismissBootSplash() {
  const splash = document.getElementById('bootSplash');
  const app = document.querySelector('.app');
  if (app) app.classList.remove('loading');
  if (splash) {
    splash.classList.add('fading');
    setTimeout(() => splash.remove(), 250);
  }
}
window.addEventListener('chat:ready', dismissBootSplash);
// Fallback in case chat:ready never fires.
setTimeout(dismissBootSplash, 4000);

window.addEventListener('chat:conn', e => {
  const { state, detail } = e.detail;
  for (const pill of [$('conn'), $('connAnon')]) {
    if (!pill) continue;
    pill.innerHTML = `<span class="conn-dot"></span><span>${state === 'connected' ? 'connected' : detail || state}</span>`;
    pill.className = `conn ${state === 'connected' ? 'good' : 'bad'}`;
  }
});

window.addEventListener('chat:data', e => {
  const next = e.detail;
  const roomChanged = state.activeRoomId !== next.activeRoomId;
  applyChatData(next);
  if (roomChanged) {
    clearComposerTarget();
    closeThreadPanel();
  }
  // Auto-pick a server when none is active and the user belongs to at least one.
  if (state.authenticated && state.activeServerId === null) {
    const mine = myServers();
    if (mine.length > 0) {
      window.chat.setActiveServer(mine[0].id);
      return; // setActiveServer fires emitData → another chat:data event will refresh
    }
  }
  renderAll();
  if (roomChanged) scrollMessagesToBottom();
});

window.addEventListener('chat:me', e => {
  state.meHex = e.detail.meHex;
});
window.addEventListener('chat:auth', e => {
  const user = e.detail.user || null;
  state.authenticated = Boolean(user);
  state.userId = user?.userId || null;
  if (!user && state.activeRoomId !== null) {
    state.activeRoomId = null;
    clearComposerTarget();
    closeThreadPanel();
    window.chat.setActiveRoom(null);
  }
  setAuthedUi(user);
  renderAll();
});

$('openAuthBtn').addEventListener('click', () => {
  focusAuthPanel();
});

$('authLogoutBtn').addEventListener('click', async () => {
  try {
    await window.chat.logout();
    setResult('Signed out.');
  } catch (err) {
    setResult(`logout failed: ${err.message ?? err}`, false);
  }
});

$('saveProfileBtn').addEventListener('click', async () => {
  if (!requireAuthAction()) return;
  try {
    const displayName = $('displayName').value.trim();
    const status = $('status').value;
    if (displayName) await window.chat.setDisplayName(displayName);
    await window.chat.setStatus(status);
    await window.chat.heartbeat();
    setResult('Profile updated.');
  } catch (err) {
    setResult(`profile update failed: ${err.message ?? err}`, false);
  }
});

function openCreateRoomModal() {
  if (!requireAuthAction()) return;
  if (state.activeServerId === null) {
    openCreateServerModal();
    return;
  }
  $('createRoomModal').classList.add('open');
  $('roomName').focus();
}
function closeCreateRoomModal() {
  $('createRoomModal').classList.remove('open');
  $('roomName').value = '';
  $('roomCategory').value = '';
}
$('openCreateRoomBtn')?.addEventListener('click', openCreateRoomModal);
$('closeCreateRoomBtn')?.addEventListener('click', closeCreateRoomModal);
$('createRoomModal')?.addEventListener('click', e => {
  if (e.target.id === 'createRoomModal') closeCreateRoomModal();
});
document.getElementById('roomList')?.addEventListener('click', e => {
  if (e.target.closest('.category-add')) {
    e.preventDefault();
    e.stopPropagation();
    openCreateRoomModal();
  }
});

$('createRoomBtn').addEventListener('click', async () => {
  if (!requireAuthAction()) return;
  const serverId = state.activeServerId;
  if (serverId === null) {
    setResult('Pick a server first.', false);
    return;
  }
  const name = $('roomName').value.trim();
  if (!name) return;
  const isPrivate = $('roomPrivacy').value === 'private';
  const category = $('roomCategory').value.trim() || undefined;
  const beforeIds = new Set(state.rooms.map(r => r.id.toString()));
  const waitForRoom = e => {
    const match = e.detail.rooms.find(
      r =>
        !beforeIds.has(r.id.toString()) &&
        r.name === name &&
        r.serverId === serverId &&
        r.createdByUserId === state.userId
    );
    if (!match) return;
    window.removeEventListener('chat:data', waitForRoom);
    if (state.activeRoomId && state.activeRoomId !== match.id) {
      stopTypingNow();
      clearComposerTarget();
      closeThreadPanel();
    }
    state.activeRoomId = match.id;
    window.chat.setActiveRoom(match.id);
  };
  window.addEventListener('chat:data', waitForRoom);
  try {
    await window.chat.createRoom(serverId, name, isPrivate, category);
    closeCreateRoomModal();
  } catch (err) {
    window.removeEventListener('chat:data', waitForRoom);
    setResult(`create room failed: ${err.message ?? err}`, false);
  }
});

function openCreateServerModal() {
  if (!requireAuthAction()) return;
  $('createServerModal').classList.add('open');
  $('serverNameInput').focus();
}
function closeCreateServerModal() {
  $('createServerModal').classList.remove('open');
  $('serverNameInput').value = '';
}
$('createServerBtn')?.addEventListener('click', openCreateServerModal);
$('closeCreateServerBtn')?.addEventListener('click', closeCreateServerModal);
$('createServerModal')?.addEventListener('click', e => {
  if (e.target.id === 'createServerModal') closeCreateServerModal();
});
$('createServerSubmitBtn')?.addEventListener('click', async () => {
  if (!requireAuthAction()) return;
  const name = $('serverNameInput').value.trim();
  if (!name) return;
  const waitForServer = e => {
    const match = e.detail.servers.find(
      s => s.name === name && s.createdByUserId === state.userId
    );
    if (!match) return;
    window.removeEventListener('chat:data', waitForServer);
    window.chat.setActiveServer(match.id);
  };
  window.addEventListener('chat:data', waitForServer);
  try {
    await window.chat.createServer(name);
    closeCreateServerModal();
  } catch (err) {
    window.removeEventListener('chat:data', waitForServer);
    setResult(`create server failed: ${err.message ?? err}`, false);
  }
});

function openRenameServerModal() {
  const srv = activeServer();
  if (!srv) return;
  $('renameServerInput').value = srv.name;
  $('renameServerModal').classList.add('open');
  $('renameServerInput').focus();
}
function closeRenameServerModal() {
  $('renameServerModal').classList.remove('open');
  $('renameServerInput').value = '';
}
$('closeRenameServerBtn')?.addEventListener('click', closeRenameServerModal);
$('renameServerModal')?.addEventListener('click', e => {
  if (e.target.id === 'renameServerModal') closeRenameServerModal();
});
$('renameServerSubmitBtn')?.addEventListener('click', async () => {
  const srv = activeServer();
  if (!srv) {
    closeRenameServerModal();
    return;
  }
  const name = $('renameServerInput').value.trim();
  if (!name || name === srv.name) {
    closeRenameServerModal();
    return;
  }
  try {
    await window.chat.renameServer(srv.id, name);
    closeRenameServerModal();
  } catch (err) {
    setResult(`rename failed: ${err.message ?? err}`, false);
  }
});

// Sidebar header dropdown (server menu)
function toggleServerMenu(force) {
  const menu = $('serverMenu');
  if (!menu) return;
  if (force === false || !menu.hidden) menu.hidden = true;
  else menu.hidden = false;
}
$('serverHeadBtn')?.addEventListener('click', e => {
  e.stopPropagation();
  if (!state.activeServerId) {
    if (state.authenticated) openCreateServerModal();
    return;
  }
  toggleServerMenu();
});
document.addEventListener('click', e => {
  const menu = $('serverMenu');
  if (!menu || menu.hidden) return;
  if (e.target.closest('#serverMenu') || e.target.closest('#serverHeadBtn'))
    return;
  toggleServerMenu(false);
});
$('serverMenu')?.addEventListener('click', async e => {
  const action = e.target.closest('[data-srv-action]')?.dataset.srvAction;
  if (!action) return;
  toggleServerMenu(false);
  const srv = activeServer();
  if (!srv) return;
  if (action === 'rename') {
    if (!amServerOwner(srv.id)) {
      setResult('Only the owner can rename.', false);
      return;
    }
    openRenameServerModal();
  } else if (action === 'delete') {
    if (!amServerOwner(srv.id)) {
      setResult('Only the owner can delete.', false);
      return;
    }
    if (
      !confirm(
        `Delete server "${srv.name}"? This removes all channels and messages.`
      )
    )
      return;
    try {
      await window.chat.deleteServer(srv.id);
      window.chat.setActiveServer(null);
    } catch (err) {
      setResult(`delete failed: ${err.message ?? err}`, false);
    }
  } else if (action === 'leave') {
    if (amServerOwner(srv.id)) {
      setResult("Owners can't leave; delete the server instead.", false);
      return;
    }
    if (!confirm(`Leave "${srv.name}"?`)) return;
    try {
      await window.chat.leaveServer(srv.id);
      window.chat.setActiveServer(null);
    } catch (err) {
      setResult(`leave failed: ${err.message ?? err}`, false);
    }
  }
});

let csRoomId = null;
function openChannelSettings(roomId) {
  const room = state.rooms.find(r => r.id === roomId);
  if (!room) return;
  csRoomId = roomId;
  $('csName').value = room.name;
  $('csName').disabled = false;
  $('csCategory').value = room.category || '';
  $('csPrivacy').value = room.isPrivate ? 'private' : 'public';
  $('csPrivacy').disabled = false;
  $('csDeleteBtn').disabled = false;
  $('csDeleteBtn').title = '';
  $('channelSettingsModal').classList.add('open');
  $('csName').focus();
}
function closeChannelSettings() {
  csRoomId = null;
  $('channelSettingsModal').classList.remove('open');
}
$('closeChannelSettingsBtn')?.addEventListener('click', closeChannelSettings);
$('channelSettingsModal')?.addEventListener('click', e => {
  if (e.target.id === 'channelSettingsModal') closeChannelSettings();
});
$('csSaveBtn')?.addEventListener('click', async () => {
  if (csRoomId === null) return;
  const room = state.rooms.find(r => r.id === csRoomId);
  if (!room) {
    closeChannelSettings();
    return;
  }
  const newName = $('csName').value.trim();
  const newCategory = $('csCategory').value.trim() || undefined;
  const newPrivate = $('csPrivacy').value === 'private';
  try {
    if (newName && newName !== room.name)
      await window.chat.renameRoom(csRoomId, newName);
    if ((room.category || '') !== (newCategory || ''))
      await window.chat.setRoomCategory(csRoomId, newCategory);
    if (newPrivate !== room.isPrivate)
      await window.chat.setRoomPrivacy(csRoomId, newPrivate);
    closeChannelSettings();
  } catch (err) {
    setResult(`save failed: ${err.message ?? err}`, false);
  }
});
$('csDeleteBtn')?.addEventListener('click', async () => {
  if (csRoomId === null) return;
  const room = state.rooms.find(r => r.id === csRoomId);
  if (!room) return;
  if (!confirm(`Delete #${room.name}? This removes all messages and members.`))
    return;
  try {
    const deletedId = csRoomId;
    await window.chat.deleteRoom(deletedId);
    closeChannelSettings();
    if (state.activeRoomId === deletedId) {
      state.activeRoomId = null;
      clearComposerTarget();
      closeThreadPanel();
      window.chat.setActiveRoom(null);
    }
  } catch (err) {
    setResult(`delete failed: ${err.message ?? err}`, false);
  }
});

function openUserMenu() {
  $('userMenu').hidden = false;
}
function closeUserMenu() {
  $('userMenu').hidden = true;
}
function toggleUserMenu(e) {
  e.stopPropagation();
  if ($('userMenu').hidden) openUserMenu();
  else closeUserMenu();
}
$('openUserMenuBtn')?.addEventListener('click', toggleUserMenu);
document.addEventListener('click', e => {
  const menu = $('userMenu');
  if (!menu || menu.hidden) return;
  if (menu.contains(e.target)) return;
  if (e.target.closest('#openUserMenuBtn')) return;
  closeUserMenu();
});
document.addEventListener('keydown', e => {
  if (e.key === 'Escape') {
    closeUserMenu();
    closeCreateRoomModal();
    closeChannelSettings();
    closeCreateServerModal();
    closeRenameServerModal();
    toggleServerMenu(false);
  }
});

$('toggleMembersBtn').addEventListener('click', () => {
  $('membersPanel').classList.toggle('collapsed');
});

$('homeBtn').addEventListener('click', () => {
  if (!state.authenticated) {
    focusAuthPanel();
    return;
  }
  const mine = myServers();
  if (mine.length === 0) {
    openCreateServerModal();
    return;
  }
  window.chat.setActiveServer(mine[0].id);
});

function renderPendingAtts() {
  const root = $('pendingAttachments');
  if (pendingAtts.length === 0) {
    root.hidden = true;
    root.innerHTML = '';
    return;
  }
  root.hidden = false;
  root.innerHTML = pendingAtts
    .map(p => {
      const isImg = p.mimeType.startsWith('image/');
      const preview = isImg
        ? `<img src="${p.previewUrl}" alt="${escapeHtml(p.name)}">`
        : `<div class="pending-att-meta">${escapeHtml(p.name)}</div>`;
      const meta = isImg
        ? `<div class="pending-att-meta">${escapeHtml(p.name)} - ${fmtBytes(p.bytes.length)}</div>`
        : `<div class="pending-att-meta">${fmtBytes(p.bytes.length)}</div>`;
      return `<div class="pending-att" data-pid="${p.id}">
      ${preview}
      ${meta}
      <button type="button" class="pending-att-x" data-remove="${p.id}" aria-label="Remove">×</button>
    </div>`;
    })
    .join('');
  root.querySelectorAll('[data-remove]').forEach(btn => {
    btn.addEventListener('click', () => {
      const id = Number(btn.dataset.remove);
      const idx = pendingAtts.findIndex(p => p.id === id);
      if (idx < 0) return;
      const removed = pendingAtts.splice(idx, 1)[0];
      if (removed?.previewUrl) URL.revokeObjectURL(removed.previewUrl);
      renderPendingAtts();
    });
  });
}

async function readFileAsBytes(file) {
  const buf = await file.arrayBuffer();
  return new Uint8Array(buf);
}

$('attachBtn')?.addEventListener('click', () => {
  if (!requireAuthAction()) return;
  $('attachInput').click();
});
$('attachInput')?.addEventListener('change', async e => {
  const files = [...(e.target.files || [])];
  e.target.value = '';
  for (const f of files) {
    if (pendingAtts.length >= ATT_MAX_COUNT) {
      setResult(`Max ${ATT_MAX_COUNT} attachments.`, false);
      break;
    }
    if (f.size > ATT_MAX_BYTES) {
      setResult(
        `${f.name}: too large (${fmtBytes(f.size)} > ${fmtBytes(ATT_MAX_BYTES)}).`,
        false
      );
      continue;
    }
    try {
      const bytes = await readFileAsBytes(f);
      const mimeType = f.type || 'application/octet-stream';
      const previewUrl = mimeType.startsWith('image/')
        ? URL.createObjectURL(f)
        : null;
      pendingAtts.push({
        id: ++pendingAttSeq,
        file: f,
        name: f.name,
        mimeType,
        bytes,
        previewUrl,
      });
    } catch (err) {
      setResult(`${f.name}: read failed - ${err.message ?? err}`, false);
    }
  }
  renderPendingAtts();
});

function clearPendingAtts() {
  for (const p of pendingAtts)
    if (p.previewUrl) URL.revokeObjectURL(p.previewUrl);
  pendingAtts = [];
  renderPendingAtts();
}

async function sendCurrentMessage() {
  if (!state.authenticated) {
    console.warn('send blocked: not authenticated');
    focusAuthPanel();
    return;
  }
  const room = activeRoom();
  if (!room) {
    console.warn(
      'send blocked: no active room (activeRoomId=',
      state.activeRoomId,
      ')'
    );
    setResult('Select a room first.', false);
    return;
  }
  if (currentSendLimit()) {
    updateComposerState();
    return;
  }
  const content = $('messageInput').value.trim();
  if (editTargetId !== null) {
    if (!content) return;
    try {
      await window.chat.editMessage(editTargetId, content);
      $('messageInput').value = '';
      clearComposerTarget();
      clearPendingAtts();
    } catch (err) {
      console.error('editMessage failed', err);
      setResult(`edit failed: ${err.message ?? err}`, false);
    }
    return;
  }
  const atts = pendingAtts.map(p => ({
    mimeType: p.mimeType,
    filename: p.name,
    bytes: p.bytes,
  }));
  if (!content && atts.length === 0) return;
  const replyTo = replyTargetId === null ? undefined : replyTargetId;
  try {
    await window.chat.sendMessage(room.id, content, replyTo, atts);
    $('messageInput').value = '';
    clearComposerTarget();
    clearPendingAtts();
    stopTypingNow();
    try {
      await window.chat.markRoomRead(room.id);
    } catch (e) {
      console.warn('markRoomRead after send failed', e);
    }
    scrollMessagesToBottom();
  } catch (err) {
    console.error('sendMessage failed', err);
    setResult(`send failed: ${err.message ?? err}`, false);
  }
}

$('sendBtn').addEventListener('click', sendCurrentMessage);
$('cancelReplyBtn').addEventListener('click', () => {
  clearComposerTarget();
  $('messageInput').value = '';
});
$('closeThreadBtn').addEventListener('click', closeThreadPanel);
$('threadComposer').addEventListener('submit', async e => {
  e.preventDefault();
  if (!requireAuthAction()) return;
  if (activeThreadRootMessageId === null) return;
  if (currentSendLimit()) {
    updateComposerState();
    return;
  }
  const content = $('threadInput').value.trim();
  if (!content) return;
  try {
    if (threadEditTargetId !== null) {
      await window.chat.editThreadMessage(threadEditTargetId, content);
      clearThreadEditTarget();
      return;
    }
    await window.chat.sendThreadMessage(activeThreadRootMessageId, content);
    $('threadInput').value = '';
  } catch (err) {
    console.error('sendThreadMessage failed', err);
    setResult(`thread send failed: ${err.message ?? err}`, false);
  }
});
$('messageInput').addEventListener('keydown', e => {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    sendCurrentMessage();
    return;
  }
  scheduleTyping();
});
$('messageInput').addEventListener('input', scheduleTyping);
$('messageInput').addEventListener('blur', stopTypingNow);
