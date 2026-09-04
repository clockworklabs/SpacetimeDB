const $ = id => document.getElementById(id);

function fmtTimestamp(micros) {
  if (micros == null) return '-';
  return new Date(Number(BigInt(micros) / 1000n)).toLocaleString();
}
function fmtUnixSeconds(s) {
  if (!s) return '-';
  return new Date(s * 1000).toLocaleString();
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
function showToast(kind, msg, dur = 4500) {
  const el = $('toast');
  el.innerHTML = `<div class="toast-msg ${kind === 'ok' ? 'ok' : ''}">${escapeHtml(msg)}</div>`;
  const child = el.firstChild;
  child.addEventListener('click', () => (el.innerHTML = ''));
  setTimeout(() => {
    if (el.firstChild === child) el.innerHTML = '';
  }, dur);
}

window.addEventListener('auth:conn', e => {
  const pill = $('conn-pill');
  const text = $('conn-text');
  pill.classList.remove('good', 'err');
  if (e.detail.state === 'connected') {
    pill.classList.add('good');
    text.textContent = 'connected';
  } else if (e.detail.state === 'connecting' || e.detail.state === 'idle') {
    text.textContent = e.detail.state;
  } else {
    pill.classList.add('err');
    text.textContent = e.detail.detail || 'disconnected';
  }
});
function dismissBootSplash() {
  const splash = document.getElementById('bootSplash');
  if (!splash) return;
  splash.classList.add('fading');
  setTimeout(() => splash.remove(), 250);
}
window.addEventListener('auth:ready', dismissBootSplash);
setTimeout(dismissBootSplash, 4000);

function initial(s) {
  const t = String(s ?? '').trim();
  return t ? t[0].toUpperCase() : '?';
}
window.addEventListener('auth:state', e => {
  const user = e.detail.user;
  const anon = $('anon-view');
  const view = $('user-view');
  const wrap = $('avatar-wrap');

  if (user) {
    anon.hidden = true;
    view.hidden = false;
    wrap.hidden = false;
    const letter = initial(user.name || user.email);
    $('avatar-btn').textContent = letter;
    $('big-avatar').textContent = letter;
    $('who-name').textContent = user.name || user.email;
    $('who-email').textContent = user.email;
    $('who-uid').textContent = user.userId;
    $('who-stdb').textContent = e.detail.senderHex ?? '-';
    $('who-exp').textContent = fmtUnixSeconds(e.detail.sessionExpiresAt);
    $('email-unverified').hidden = !!user.emailVerified;
  } else {
    anon.hidden = false;
    view.hidden = true;
    wrap.hidden = true;
    $('avatar-menu').classList.remove('is-open');
  }
});

const avatarBtn = $('avatar-btn');
const avatarMenu = $('avatar-menu');
avatarBtn.addEventListener('click', e => {
  e.stopPropagation();
  const open = avatarMenu.classList.toggle('is-open');
  avatarBtn.setAttribute('aria-expanded', String(open));
  if (open) refreshSessionsList();
});
async function refreshSessionsList() {
  const box = $('sessions-list');
  try {
    const r = await window.auth.listMySessions();
    if (!r.sessions.length) {
      box.innerHTML = '<div class="id-row">No active sessions.</div>';
      return;
    }
    box.innerHTML = r.sessions
      .map(
        s => `
      <div class="id-row">
        <div class="lbl">${fmtTimestamp(s.createdAt.microsSinceUnixEpoch)}</div>
        <div>${escapeHtml(s.userAgent ?? 'unknown UA')}</div>
        <button class="btn tiny danger" data-revoke="${escapeHtml(s.sessionId)}" style="margin-top: 6px;">Revoke</button>
      </div>
    `
      )
      .join('');
    box.querySelectorAll('[data-revoke]').forEach(btn => {
      btn.addEventListener('click', async () => {
        if (!confirm('Revoke this session?')) return;
        try {
          await window.auth.revokeMySession(btn.dataset.revoke);
          refreshSessionsList();
        } catch (err) {
          showToast('err', err.message ?? String(err));
        }
      });
    });
  } catch (err) {
    box.innerHTML = `<div class="id-row" style="color: var(--color-orange);">${escapeHtml(err.message ?? String(err))}</div>`;
  }
}
$('resend-verify-btn').addEventListener('click', () =>
  tryCall('resend-verify-btn', async () => {
    await window.auth.requestEmailVerify();
    showToast('ok', 'verification email sent (check STDB log in dev)', 7000);
  })
);
document.addEventListener('click', e => {
  if (!avatarMenu.classList.contains('is-open')) return;
  if (!avatarMenu.contains(e.target) && e.target !== avatarBtn) {
    avatarMenu.classList.remove('is-open');
    avatarBtn.setAttribute('aria-expanded', 'false');
  }
});

const notesById = new Map();
window.addEventListener('auth:notes', e => {
  const notes = e.detail.notes;
  notesById.clear();
  notes.forEach(n => notesById.set(n.noteId, n));

  const list = $('notes-list');
  const empty = $('notes-empty');
  if (notes.length === 0) {
    list.hidden = true;
    empty.hidden = false;
    return;
  }
  empty.hidden = true;
  list.hidden = false;
  list.innerHTML = notes
    .map(
      n => `
    <div class="note-card" data-edit="${escapeHtml(n.noteId)}">
      <button class="nc-del" data-del="${escapeHtml(n.noteId)}" aria-label="delete">×</button>
      ${n.title ? `<div class="nc-title">${escapeHtml(n.title)}</div>` : ''}
      <div class="nc-body">${escapeHtml(n.body)}</div>
      <div class="nc-meta">${fmtTimestamp(n.createdAt.microsSinceUnixEpoch)}</div>
    </div>
  `
    )
    .join('');
  list.querySelectorAll('[data-del]').forEach(btn => {
    btn.addEventListener('click', async e => {
      e.stopPropagation();
      if (!confirm('Delete this note?')) return;
      try {
        await window.auth.deleteNote(btn.dataset.del);
      } catch (err) {
        showToast('err', err.message ?? String(err));
      }
    });
  });
  list.querySelectorAll('[data-edit]').forEach(card => {
    card.addEventListener('click', () => openEdit(card.dataset.edit));
  });

  // Refresh open modal if its note row changed underneath us.
  if (editingId && notesById.has(editingId)) {
    const updated = notesById.get(editingId);
    if (
      $('edit-title').value === editOriginal.title &&
      $('edit-body').value === editOriginal.body
    ) {
      // user hasn't typed; pull the new values in
      $('edit-title').value = updated.title;
      $('edit-body').value = updated.body;
      editOriginal = { title: updated.title, body: updated.body };
    }
  }
});

let editingId = null;
let editOriginal = { title: '', body: '' };
const editBackdrop = $('edit-backdrop');
const editTitle = $('edit-title');
const editBody = $('edit-body');

function openEdit(noteId) {
  const n = notesById.get(noteId);
  if (!n) return;
  editingId = noteId;
  editOriginal = { title: n.title, body: n.body };
  editTitle.value = n.title;
  editBody.value = n.body;
  editBackdrop.classList.add('is-open');
  setTimeout(() => editBody.focus(), 0);
}
function closeEdit() {
  editBackdrop.classList.remove('is-open');
  editingId = null;
}
async function saveEdit() {
  if (!editingId) return;
  const id = editingId;
  const title = editTitle.value;
  const body = editBody.value;
  if (title === editOriginal.title && body === editOriginal.body) {
    closeEdit();
    return;
  }
  try {
    await window.auth.updateNote({ noteId: id, title, body });
    closeEdit();
  } catch (err) {
    showToast('err', err.message ?? String(err));
  }
}
async function deleteFromEdit() {
  if (!editingId) return;
  if (!confirm('Delete this note?')) return;
  const id = editingId;
  try {
    await window.auth.deleteNote(id);
    closeEdit();
  } catch (err) {
    showToast('err', err.message ?? String(err));
  }
}
editBackdrop.addEventListener('click', e => {
  if (e.target === editBackdrop) saveEdit();
});
$('edit-cancel').addEventListener('click', closeEdit);
$('edit-save').addEventListener('click', saveEdit);
$('edit-del').addEventListener('click', deleteFromEdit);
document.addEventListener('keydown', e => {
  if (e.key === 'Escape' && editBackdrop.classList.contains('is-open'))
    closeEdit();
});

const compose = $('compose-card');
const ntBody = $('nt-body');
const ntTitle = $('nt-title');
function expandCompose() {
  compose.classList.remove('collapsed');
}
function collapseCompose() {
  compose.classList.add('collapsed');
  ntTitle.value = '';
  ntBody.value = '';
  ntBody.style.height = '';
}
ntBody.addEventListener('focus', expandCompose);
ntTitle.addEventListener('focus', expandCompose);
ntBody.addEventListener('input', () => {
  ntBody.style.height = 'auto';
  ntBody.style.height = ntBody.scrollHeight + 'px';
});
$('nt-cancel').addEventListener('click', collapseCompose);

async function tryCall(btnId, fn, okMsg) {
  const btn = $(btnId);
  btn.disabled = true;
  try {
    await fn();
    if (okMsg) showToast('ok', okMsg);
  } catch (err) {
    showToast('err', err.message ?? String(err));
  } finally {
    btn.disabled = false;
  }
}

$('logout-btn').addEventListener('click', () =>
  tryCall('logout-btn', () => window.auth.logout(), 'signed out')
);
$('whoami-btn').addEventListener('click', () =>
  tryCall('whoami-btn', async () => {
    const r = await window.auth.whoami();
    showToast(
      'ok',
      `userId=${r.userId ?? 'null'} sender=${r.senderIdentityHex.slice(0, 16)}…`,
      6000
    );
  })
);
$('nt-btn').addEventListener('click', () =>
  tryCall(
    'nt-btn',
    async () => {
      const title = ntTitle.value.trim();
      const body = ntBody.value;
      if (!title && !body.trim()) return;
      await window.auth.createNote({ title: title || '', body });
      collapseCompose();
    },
    'note saved'
  )
);
