import { renderMarkdown } from './markdown.js';

const $ = id => document.getElementById(id);
let activeThreadId = null;
function selectThread(newId) {
  activeThreadId = newId;
  window.stdb?.setActiveThread(newId);
}
let allThreads = [];
let allMessages = [];
let allAttachments = {}; // messageId -> attachment metadata
let pendingAttachments = []; // {mimeType, filename, bytes} not yet sent
let lockedThreads = new Map(); // threadId -> cancelRequested
let overrides = new Map(); // agentName -> AgentOverride row

let inFlightSend = new Set();
let configState = { kind: 'unknown' };
let connState = 'connecting';

let confirmResolver = null;
function confirmDialog({
  title = 'Confirm',
  body = '',
  confirmText = 'OK',
  danger = false,
} = {}) {
  $('confirm-title').textContent = title;
  $('confirm-body').textContent = body;
  const ok = $('confirm-ok');
  ok.textContent = confirmText;
  ok.className = 'btn ' + (danger ? 'danger' : 'primary');
  $('confirm-backdrop').classList.add('open');
  setTimeout(() => ok.focus(), 50);
  return new Promise(resolve => {
    confirmResolver = resolve;
  });
}
function closeConfirm(result) {
  $('confirm-backdrop').classList.remove('open');
  if (confirmResolver) {
    confirmResolver(result);
    confirmResolver = null;
  }
}
$('confirm-ok').addEventListener('click', () => closeConfirm(true));
$('confirm-cancel').addEventListener('click', () => closeConfirm(false));
document.addEventListener('keydown', e => {
  if (!$('confirm-backdrop').classList.contains('open')) return;
  if (e.key === 'Escape') {
    e.preventDefault();
    closeConfirm(false);
  } else if (e.key === 'Enter') {
    e.preventDefault();
    closeConfirm(true);
  }
});
$('confirm-backdrop').addEventListener('click', e => {
  if (e.target === $('confirm-backdrop')) closeConfirm(false);
});

function openImageViewer(src, title) {
  $('image-full').src = src;
  $('image-full').alt = title;
  $('image-title').textContent = title;
  $('image-open').href = src;
  $('image-backdrop').classList.add('open');
  $('image-dialog').focus({ preventScroll: true });
}
function closeImageViewer() {
  $('image-backdrop').classList.remove('open');
  $('image-full').removeAttribute('src');
  $('image-open').href = '#';
}
$('image-close').addEventListener('click', closeImageViewer);
$('image-backdrop').addEventListener('click', e => {
  if (e.target === $('image-backdrop')) closeImageViewer();
});
document.addEventListener('keydown', e => {
  if (!$('image-backdrop').classList.contains('open')) return;
  if (e.key === 'Escape') {
    e.preventDefault();
    closeImageViewer();
  }
});

let toastTimer = null;
function toast(kind, text) {
  const el = $('toast');
  el.className = 'toast ' + kind;
  el.textContent = text;
  void el.offsetWidth;
  el.classList.add('show');
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove('show'), 2400);
}

window.addEventListener('stdb:connState', e => {
  const { state, detail } = e.detail;
  connState = state;
  const text = $('user-status');
  if (text) {
    if (state === 'connected') {
      text.className = 'user-status ok';
      text.textContent = 'online';
    } else if (state === 'connecting') {
      text.className = 'user-status warn';
      text.textContent = 'connecting…';
    } else if (state === 'idle') {
      text.className = 'user-status';
      text.textContent = 'idle';
    } else {
      text.className = 'user-status err';
      text.textContent = detail ? `error: ${detail}` : 'error';
    }
  }
  updateButtons();
});

window.addEventListener('stdb:ready', () => {
  $('btn-settings').disabled = false;
  updateButtons();
});

$('btn-toggle-sidebar').addEventListener('click', () => {
  const sb = $('sidebar');
  sb.classList.toggle('collapsed');
  $('btn-toggle-sidebar').textContent = sb.classList.contains('collapsed')
    ? '»'
    : '«';
});

$('btn-new-thread-hero').addEventListener('click', () =>
  $('btn-new-thread').click()
);

window.addEventListener('stdb:config', e => {
  configState = e.detail.state;
  if (configState.kind === 'unconfigured') {
    openSetup();
  } else if (configState.kind === 'configured') {
    closeSetup();
  }
  updateButtons();
});

function openSetup() {
  const isFirst = configState.kind !== 'configured';
  $('setup-cancel').style.display = isFirst ? 'none' : '';
  if (configState.kind === 'configured') {
    $('cfg-stalelock').value = String(
      configState.status.staleLockThresholdSecs
    );
    $('cfg-rl-tokens').value =
      configState.status.rateLimitTokensPerWindow != null
        ? String(configState.status.rateLimitTokensPerWindow)
        : '';
    $('cfg-rl-window').value =
      configState.status.rateLimitWindowSecs != null
        ? String(configState.status.rateLimitWindowSecs)
        : '';
    const providers = configState.status.configuredProviders;
    $('cfg-configured').textContent =
      providers.length === 0 ? 'none' : providers.join(', ');
  } else {
    $('cfg-configured').textContent = 'none';
  }
  $('setup-backdrop').classList.add('open');
  setTimeout(() => $('cfg-apikey').focus(), 50);
}
function closeSetup() {
  $('setup-backdrop').classList.remove('open');
}

$('setup-cancel').addEventListener('click', closeSetup);
$('setup-backdrop').addEventListener('click', e => {
  if (e.target === $('setup-backdrop')) closeSetup();
});

$('setup-form').addEventListener('submit', async e => {
  e.preventDefault();
  if (!window.stdb) return toast('err', 'STDB not ready');

  const provider = $('cfg-provider').value;
  const apiKey = $('cfg-apikey').value.trim();
  const staleLockThresholdSecs = Number.parseInt($('cfg-stalelock').value, 10);
  const rlTokensRaw = $('cfg-rl-tokens').value.trim();
  const rlWindowRaw = $('cfg-rl-window').value.trim();
  const rateLimitTokensPerWindow =
    rlTokensRaw === '' ? undefined : Number.parseInt(rlTokensRaw, 10);
  const rateLimitWindowSecs =
    rlWindowRaw === '' ? undefined : Number.parseInt(rlWindowRaw, 10);

  if (!Number.isFinite(staleLockThresholdSecs) || staleLockThresholdSecs < 1) {
    return toast('err', 'Stale-lock threshold must be ≥ 1');
  }
  const haveTokens = rateLimitTokensPerWindow !== undefined;
  const haveWindow = rateLimitWindowSecs !== undefined;
  if (haveTokens !== haveWindow) {
    return toast('err', 'Set both rate-limit fields, or leave both blank');
  }
  if (
    haveTokens &&
    (!Number.isFinite(rateLimitTokensPerWindow) || rateLimitTokensPerWindow < 1)
  ) {
    return toast('err', 'Rate-limit token cap must be ≥ 1');
  }
  if (
    haveWindow &&
    (!Number.isFinite(rateLimitWindowSecs) || rateLimitWindowSecs < 1)
  ) {
    return toast('err', 'Rate-limit window must be ≥ 1');
  }

  const btn = $('setup-save');
  btn.disabled = true;
  btn.textContent = 'Saving…';
  try {
    await window.stdb.setAgentSecret({
      staleLockThresholdSecs,
      rateLimitTokensPerWindow,
      rateLimitWindowSecs,
    });
    if (apiKey) {
      await window.stdb.setApiKey(provider, apiKey);
      $('cfg-apikey').value = '';
    }
    toast('ok', 'Saved');
  } catch (err) {
    toast('err', err.message ?? String(err));
  } finally {
    btn.disabled = false;
    btn.textContent = 'Save';
  }
});

$('btn-settings').addEventListener('click', openSetup);

window.addEventListener('stdb:overrides', e => {
  overrides = new Map((e.detail.overrides ?? []).map(o => [o.agentName, o]));
  renderMessages();
});

window.addEventListener('stdb:locks', e => {
  lockedThreads = new Map(e.detail.locks);
  renderThreads();
  renderMessages();
  updateButtons();
});

window.addEventListener('stdb:threads', e => {
  allThreads = e.detail.threads;
  renderThreads();
  if (activeThreadId === null && allThreads.length > 0) {
    selectThread(allThreads[0].id);
    renderThreads();
  }
  if (
    activeThreadId !== null &&
    !allThreads.some(t => t.id === activeThreadId)
  ) {
    selectThread(allThreads[0]?.id ?? null);
  }
  if (activeThreadId !== null) renderMessages();
  updateButtons();
});

function renderThreads() {
  const list = $('thread-list');
  if (allThreads.length === 0) {
    list.innerHTML = '<div class="empty">no threads yet</div>';
    return;
  }
  list.innerHTML = '';
  for (const t of allThreads) {
    const item = document.createElement('div');
    item.className = 'thread-item' + (t.id === activeThreadId ? ' active' : '');
    item.dataset.threadId = String(t.id);
    const titleEl = document.createElement('span');
    titleEl.className = 'title';
    titleEl.textContent = t.title ?? `Thread #${t.id}`;
    if (lockedThreads.has(t.id)) {
      const dot = document.createElement('span');
      dot.className = 'busy-dot';
      dot.title = lockedThreads.get(t.id) ? 'stopping' : 'thinking';
      item.appendChild(dot);
    }
    item.appendChild(titleEl);

    const menuBtn = document.createElement('button');
    menuBtn.className = 'row-menu-btn';
    menuBtn.title = 'More';
    menuBtn.setAttribute('aria-label', 'More actions');
    menuBtn.innerHTML =
      '<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5" r="1.5"/><circle cx="12" cy="12" r="1.5"/><circle cx="12" cy="19" r="1.5"/></svg>';
    menuBtn.addEventListener('click', e => {
      e.stopPropagation();
      openRowMenu(item, t.id);
    });
    item.appendChild(menuBtn);

    item.addEventListener('click', () => {
      selectThread(t.id);
      renderThreads();
      renderMessages();
      updateButtons();
    });
    list.appendChild(item);
  }
}

const DEFAULT_AGENT_PREFERENCE = ['chat'];
function pickDefaultAgent() {
  const agents =
    configState.kind === 'configured' ? configState.status.agents : [];
  const names = agents.map(a => a.name);
  for (const pref of DEFAULT_AGENT_PREFERENCE) {
    if (names.includes(pref)) return pref;
  }
  return names[0];
}

// thread → effective model (thread.modelOverride ?? agent override ?? agent code default)
function effectiveModelFor(thread) {
  if (!thread) return '';
  if (thread.modelOverride) return thread.modelOverride;
  const ov = overrides.get(thread.agentName);
  if (ov?.model != null) return ov.model;
  if (configState.kind !== 'configured') return '';
  const ai = configState.status.agents.find(a => a.name === thread.agentName);
  return ai?.defaultModel ?? '';
}
$('btn-new-thread').addEventListener('click', async () => {
  if (configState.kind !== 'configured' || !window.stdb) return;
  const agentName = pickDefaultAgent();
  if (!agentName) return toast('err', 'no agents registered');
  try {
    const id = await window.stdb.startThread({
      agentName,
      title: undefined,
      systemPromptOverride: undefined,
      metadata: undefined,
    });
    selectThread(id);
    renderThreads();
    renderMessages();
    updateButtons();
    $('composer-input').focus();
  } catch (err) {
    toast('err', err.message ?? String(err));
  }
});

let openMenuEl = null;
function closeRowMenu() {
  if (openMenuEl) {
    openMenuEl.parentElement?.classList.remove('menu-open');
    openMenuEl.remove();
    openMenuEl = null;
  }
}
function openRowMenu(itemEl, threadId) {
  if (openMenuEl && openMenuEl.dataset.threadId === String(threadId)) {
    closeRowMenu();
    return;
  }
  closeRowMenu();
  const menu = document.createElement('div');
  menu.className = 'row-menu';
  menu.dataset.threadId = String(threadId);
  const renameBtn = document.createElement('button');
  renameBtn.innerHTML =
    '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg><span>Rename</span>';
  renameBtn.addEventListener('click', e => {
    e.stopPropagation();
    closeRowMenu();
    openRenameFor(threadId);
  });
  const deleteBtn = document.createElement('button');
  deleteBtn.className = 'danger';
  deleteBtn.innerHTML =
    '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/><path d="M10 11v6M14 11v6"/></svg><span>Delete</span>';
  deleteBtn.addEventListener('click', e => {
    e.stopPropagation();
    closeRowMenu();
    deleteThreadConfirmed(threadId);
  });
  menu.appendChild(renameBtn);
  menu.appendChild(deleteBtn);
  itemEl.appendChild(menu);
  itemEl.classList.add('menu-open');
  openMenuEl = menu;
}
document.addEventListener('click', () => closeRowMenu());
document.addEventListener('keydown', e => {
  if (e.key === 'Escape') closeRowMenu();
});

let renameTargetId = null;
function openRenameFor(threadId) {
  const t = allThreads.find(x => x.id === threadId);
  if (!t) return;
  renameTargetId = threadId;
  $('rename-title').value = t.title ?? '';
  $('rename-prompt').value = t.systemPromptOverride ?? '';
  $('rename-backdrop').classList.add('open');
  setTimeout(() => $('rename-title').focus(), 50);
}
async function deleteThreadConfirmed(threadId) {
  if (!window.stdb) return;
  const t = allThreads.find(x => x.id === threadId);
  const label = t?.title ?? `Thread #${threadId}`;
  const ok = await confirmDialog({
    title: 'Delete chat',
    body: `Delete "${label}" and all its messages? This can't be undone.`,
    confirmText: 'Delete',
    danger: true,
  });
  if (!ok) return;
  try {
    await window.stdb.deleteThread(threadId);
    if (activeThreadId === threadId) {
      selectThread(null);
      renderMessages();
    }
    renderThreads();
    updateButtons();
  } catch (err) {
    toast('err', err.message ?? String(err));
  }
}
$('rename-cancel').addEventListener('click', () =>
  $('rename-backdrop').classList.remove('open')
);
$('rename-backdrop').addEventListener('click', e => {
  if (e.target === $('rename-backdrop'))
    $('rename-backdrop').classList.remove('open');
});
$('rename-form').addEventListener('submit', async e => {
  e.preventDefault();
  if (!window.stdb || renameTargetId === null) return;
  const titleRaw = $('rename-title').value.trim();
  const promptRaw = $('rename-prompt').value.trim();
  try {
    await window.stdb.updateThread({
      threadId: renameTargetId,
      title: titleRaw ? titleRaw : undefined,
      systemPromptOverride: promptRaw ? promptRaw : undefined,
      modelOverride: undefined,
      metadata: undefined,
      clearTitle: !titleRaw,
      clearSystemPromptOverride: !promptRaw,
      clearModelOverride: false,
      clearMetadata: false,
    });
    $('rename-backdrop').classList.remove('open');
    renameTargetId = null;
  } catch (err) {
    toast('err', err.message ?? String(err));
  }
});

window.addEventListener('stdb:messages', e => {
  allMessages = e.detail.messages;
  allAttachments = e.detail.attachments ?? {};
  renderMessages();
});

function renderMessages() {
  const wrap = $('msg-list');
  const head = $('chat-head');
  const headLabel = $('chat-agent-label');
  const composer = $('composer');
  const hero = $('empty-hero');

  if (activeThreadId === null) {
    head.hidden = true;
    hero.style.display = 'flex';
    wrap.style.display = 'none';
    composer.hidden = true;
    return;
  }
  hero.style.display = 'none';
  wrap.style.display = 'flex';
  composer.hidden = false;

  const t = allThreads.find(x => x.id === activeThreadId);
  head.hidden = false;
  const titleStr = t?.title ?? `Thread #${activeThreadId}`;
  headLabel.textContent = titleStr;
  if (t) {
    const model = effectiveModelFor(t) || t.agentName || 'Unavailable';
    $('composer-model-label').textContent = model;
  }

  const msgs = allMessages.filter(
    m => m.id !== undefined && m.threadId === activeThreadId
  );
  if (msgs.length === 0 && !lockedThreads.has(activeThreadId)) {
    wrap.innerHTML = '<div class="empty">no messages yet - say hello</div>';
    return;
  }
  wrap.innerHTML = '';
  const wasNearBottom =
    wrap.scrollHeight - wrap.scrollTop - wrap.clientHeight < 100;
  let lastUserMessage = null;
  let lastAssistantIdx = -1;
  for (let i = msgs.length - 1; i >= 0; i--) {
    if (msgs[i].role === 'assistant') {
      lastAssistantIdx = i;
      break;
    }
  }
  for (let i = 0; i < msgs.length; i++) {
    const m = msgs[i];
    if (m.role === 'user') lastUserMessage = m.content;

    const node = document.createElement('div');
    const errCls = m.isError ? ' error' : '';
    node.className = `msg ${m.role}${errCls}`;
    const who = document.createElement('div');
    who.className = 'who';
    who.textContent = m.role + (m.isError ? ' · error' : '');
    const body = document.createElement('div');
    body.className = 'body';
    if (m.role === 'assistant' && !m.isError) {
      body.innerHTML = renderMarkdown(m.content) || '<em>(empty)</em>';
      body.querySelectorAll('pre').forEach(pre => {
        const wrap = document.createElement('div');
        wrap.className = 'code-block';
        pre.parentNode.insertBefore(wrap, pre);
        wrap.appendChild(pre);
        const btn = document.createElement('button');
        btn.className = 'copy';
        btn.textContent = 'copy';
        btn.addEventListener('click', () => {
          const code = pre.textContent ?? '';
          navigator.clipboard.writeText(code).then(
            () => {
              btn.textContent = 'copied';
              setTimeout(() => (btn.textContent = 'copy'), 1200);
            },
            () => {}
          );
        });
        wrap.appendChild(btn);
      });
    } else {
      body.textContent = m.content;
    }
    node.appendChild(who);
    node.appendChild(body);

    const msgAtts = allAttachments[String(m.id)] ?? allAttachments[m.id] ?? [];
    for (const att of msgAtts) {
      const fileUrl = `/files?id=${encodeURIComponent(att.fileId.toString())}&v=${encodeURIComponent(att.sha256Hex ?? '')}`;
      const filename = att.filename ?? 'attachment';
      if (String(att.mimeType ?? '').startsWith('image/')) {
        const img = document.createElement('img');
        img.className = 'msg-image';
        img.src = fileUrl;
        img.alt = filename;
        img.loading = 'lazy';
        img.title = filename;
        img.addEventListener('click', () => openImageViewer(fileUrl, filename));
        img.addEventListener(
          'error',
          () => {
            const link = document.createElement('a');
            link.className = 'msg-file-link';
            link.href = fileUrl;
            link.target = '_blank';
            link.rel = 'noopener';
            link.textContent = filename;
            img.replaceWith(link);
          },
          { once: true }
        );
        node.appendChild(img);
      } else {
        const link = document.createElement('a');
        link.className = 'msg-file-link';
        link.href = fileUrl;
        link.target = '_blank';
        link.rel = 'noopener';
        link.textContent = filename;
        node.appendChild(link);
      }
    }

    if (m.toolCallsJson) {
      const tc = document.createElement('details');
      tc.className = 'toolcalls';
      const summary = document.createElement('summary');
      let callList;
      try {
        callList = JSON.parse(m.toolCallsJson);
      } catch {
        callList = [];
      }
      summary.textContent = `▸ ${callList.length} tool call${callList.length === 1 ? '' : 's'}`;
      const pre = document.createElement('pre');
      pre.textContent = formatToolCalls(callList);
      tc.appendChild(summary);
      tc.appendChild(pre);
      node.appendChild(tc);
    }

    if (m.role === 'assistant') {
      const footer = document.createElement('div');
      footer.className = 'msg-footer';

      if (m.content) {
        const copyBtn = document.createElement('button');
        copyBtn.title = 'Copy';
        copyBtn.setAttribute('aria-label', 'Copy');
        copyBtn.innerHTML =
          '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>';
        copyBtn.addEventListener('click', () => {
          navigator.clipboard.writeText(m.content).then(
            () => {
              copyBtn.title = 'Copied';
              setTimeout(() => (copyBtn.title = 'Copy'), 1200);
            },
            () => toast('err', 'clipboard write failed')
          );
        });
        footer.appendChild(copyBtn);
      }

      if (i === lastAssistantIdx && !lockedThreads.has(activeThreadId)) {
        const regen = document.createElement('button');
        regen.title = 'Regenerate';
        regen.setAttribute('aria-label', 'Regenerate');
        regen.innerHTML =
          '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg>';
        regen.addEventListener('click', async () => {
          if (!window.stdb || activeThreadId === null) return;
          regen.disabled = true;
          try {
            await window.stdb.regenerateResponse(activeThreadId);
          } catch (err) {
            toast('err', err.message ?? String(err));
          } finally {
            regen.disabled = false;
          }
        });
        footer.appendChild(regen);
      }

      if (m.isError && lastUserMessage !== null) {
        const retry = document.createElement('button');
        retry.title = 'Retry';
        retry.setAttribute('aria-label', 'Retry');
        retry.innerHTML =
          '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="1 4 1 10 7 10"/><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"/></svg>';
        const userMsg = lastUserMessage;
        retry.addEventListener('click', async () => {
          if (!window.stdb || activeThreadId === null) return;
          retry.disabled = true;
          try {
            await window.stdb.sendMessage(activeThreadId, userMsg);
          } catch (err) {
            toast('err', err.message ?? String(err));
          } finally {
            retry.disabled = false;
          }
        });
        footer.appendChild(retry);
      }

      const spacer = document.createElement('span');
      spacer.className = 'spacer';
      footer.appendChild(spacer);

      if (
        !m.isError &&
        (m.promptTokens !== undefined || m.completionTokens !== undefined)
      ) {
        const u = document.createElement('span');
        u.className = 'usage';
        const pt = m.promptTokens ?? '?';
        const ct = m.completionTokens ?? '?';
        u.textContent = `${pt} in · ${ct} out`;
        footer.appendChild(u);
      }

      node.appendChild(footer);
    }

    wrap.appendChild(node);
  }

  if (lockedThreads.has(activeThreadId)) {
    const t = document.createElement('div');
    t.className = 'typing';
    t.innerHTML =
      '<span>agent is thinking</span><span class="dots"><span></span><span></span><span></span></span>';
    wrap.appendChild(t);
  }

  if (wasNearBottom) wrap.scrollTop = wrap.scrollHeight;
}

function formatToolCalls(arr) {
  return arr
    .map(c => {
      const args = c.function?.arguments ?? '';
      return `→ ${c.function?.name ?? '?'}(${args})`;
    })
    .join('\n');
}

const MAX_ATTACH_BYTES = 4_000_000;
const MAX_ATTACH_COUNT = 4;
const MAX_ATTACH_TOTAL_BYTES = 12_000_000;
function renderPendingAttachments() {
  const wrap = $('pending-attachments');
  if (pendingAttachments.length === 0) {
    wrap.style.display = 'none';
    wrap.innerHTML = '';
    return;
  }
  wrap.style.display = 'flex';
  wrap.innerHTML = '';
  pendingAttachments.forEach((a, idx) => {
    const t = document.createElement('div');
    t.className = 'pending-thumb';
    const blob = new Blob([a.bytes], { type: a.mimeType });
    const url = URL.createObjectURL(blob);
    t.innerHTML = `<img src="${url}"><button type="button" class="x" aria-label="remove">×</button>`;
    t.querySelector('.x').addEventListener('click', () => {
      pendingAttachments.splice(idx, 1);
      renderPendingAttachments();
    });
    wrap.appendChild(t);
  });
}

// Model list comes from OpenRouter's /api/v1/models so it's always
// current. Cached per page load.
let modelListCache = null;
let modelListPromise = null;
async function getModelList() {
  if (modelListCache) return modelListCache;
  if (modelListPromise) return modelListPromise;
  modelListPromise = (async () => {
    const res = await fetch('https://openrouter.ai/api/v1/models');
    if (!res.ok) throw new Error(`openrouter /models -> ${res.status}`);
    const body = await res.json();
    const ids = (body?.data ?? [])
      .map(m => m?.id)
      .filter(id => typeof id === 'string')
      .sort();
    modelListCache = ids;
    return ids;
  })();
  try {
    return await modelListPromise;
  } finally {
    modelListPromise = null;
  }
}

let modelPopover = null;
function closeModelPopover() {
  if (modelPopover) {
    modelPopover.remove();
    modelPopover = null;
  }
}
async function pickModel(model) {
  closeModelPopover();
  if (!window.stdb || activeThreadId === null) return;
  try {
    await window.stdb.updateThread({
      threadId: activeThreadId,
      title: undefined,
      systemPromptOverride: undefined,
      modelOverride: model,
      metadata: undefined,
      clearTitle: false,
      clearSystemPromptOverride: false,
      clearModelOverride: false,
      clearMetadata: false,
    });
  } catch (err) {
    toast('err', err.message ?? String(err));
  }
}
function renderModelList(listEl, ids, current, filter) {
  listEl.innerHTML = '';
  const q = filter.trim().toLowerCase();
  const filtered = q ? ids.filter(id => id.toLowerCase().includes(q)) : ids;
  if (filtered.length === 0) {
    const empty = document.createElement('div');
    empty.className = 'empty';
    empty.textContent = 'no matches';
    listEl.appendChild(empty);
    return;
  }
  // Keep the active model at the top if it's in the filtered set.
  const ordered = filtered.includes(current)
    ? [current, ...filtered.filter(id => id !== current)]
    : filtered;
  // Cap the rendered count for performance; search to find the rest.
  const SHOW_LIMIT = 200;
  for (const id of ordered.slice(0, SHOW_LIMIT)) {
    const btn = document.createElement('button');
    btn.textContent = id;
    btn.title = id;
    if (id === current) btn.classList.add('active');
    btn.addEventListener('click', () => pickModel(id));
    listEl.appendChild(btn);
  }
  if (ordered.length > SHOW_LIMIT) {
    const more = document.createElement('div');
    more.className = 'empty';
    more.textContent = `…and ${ordered.length - SHOW_LIMIT} more - refine the search`;
    listEl.appendChild(more);
  }
}
async function openModelPopover(anchor) {
  closeModelPopover();
  const t = allThreads.find(x => x.id === activeThreadId);
  if (!t) return;
  const current = effectiveModelFor(t);
  modelPopover = document.createElement('div');
  modelPopover.className = 'model-popover';
  const search = document.createElement('input');
  search.type = 'text';
  search.className = 'search';
  search.placeholder = 'filter models…';
  const list = document.createElement('div');
  list.className = 'list';
  const loading = document.createElement('div');
  loading.className = 'empty';
  loading.textContent = 'loading models…';
  list.appendChild(loading);
  modelPopover.appendChild(search);
  modelPopover.appendChild(list);
  document.body.appendChild(modelPopover);
  // Anchor by bottom edge so the popover grows upward.
  const r = anchor.getBoundingClientRect();
  modelPopover.style.left = `${r.left}px`;
  modelPopover.style.bottom = `${window.innerHeight - r.top + 4}px`;
  search.focus();

  let ids;
  try {
    ids = await getModelList();
  } catch (err) {
    loading.textContent = `couldn't load: ${err.message ?? err}`;
    return;
  }
  // Popover may have been closed during the await.
  if (!modelPopover) return;
  renderModelList(list, ids, current, '');
  search.addEventListener('input', () =>
    renderModelList(list, ids, current, search.value)
  );
}
$('composer-model-label').addEventListener('click', e => {
  e.stopPropagation();
  if (modelPopover) closeModelPopover();
  else openModelPopover(e.currentTarget);
});
document.addEventListener('click', e => {
  if (modelPopover && !modelPopover.contains(e.target)) closeModelPopover();
});
document.addEventListener('keydown', e => {
  if (e.key === 'Escape') closeModelPopover();
});

$('btn-attach').addEventListener('click', () => $('file-input').click());
$('file-input').addEventListener('change', async e => {
  const file = e.target.files?.[0];
  e.target.value = '';
  if (!file) return;
  if (
    !['image/png', 'image/jpeg', 'image/webp', 'image/gif'].includes(file.type)
  ) {
    return toast('err', `unsupported image type: ${file.type}`);
  }
  if (pendingAttachments.length >= MAX_ATTACH_COUNT) {
    return toast('err', `at most ${MAX_ATTACH_COUNT} attachments are allowed`);
  }
  if (file.size > MAX_ATTACH_BYTES) {
    return toast(
      'err',
      `attachment too large (${file.size} > ${MAX_ATTACH_BYTES})`
    );
  }
  const totalBytes =
    pendingAttachments.reduce((sum, item) => sum + item.bytes.length, 0) +
    file.size;
  if (totalBytes > MAX_ATTACH_TOTAL_BYTES) {
    return toast(
      'err',
      `attachments exceed the ${MAX_ATTACH_TOTAL_BYTES / 1_000_000} MB total limit`
    );
  }
  const bytes = new Uint8Array(await file.arrayBuffer());
  pendingAttachments.push({
    mimeType: file.type,
    filename: file.name,
    bytes,
  });
  renderPendingAttachments();
});

const composer = $('composer-input');
composer.addEventListener('input', () => {
  composer.style.height = 'auto';
  composer.style.height = Math.min(composer.scrollHeight, 160) + 'px';
});
composer.addEventListener('keydown', e => {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    $('btn-send').click();
  }
});

$('btn-stop').addEventListener('click', async () => {
  if (!window.stdb || activeThreadId === null) return;
  const tid = activeThreadId;
  if (!lockedThreads.has(tid) || lockedThreads.get(tid) === true) return;
  try {
    await window.stdb.requestCancel(tid);
  } catch (err) {
    toast('err', err.message ?? String(err));
  }
});

$('btn-send').addEventListener('click', async () => {
  if (!window.stdb || activeThreadId === null) return;
  const tid = activeThreadId;
  if (lockedThreads.has(tid) || inFlightSend.has(tid)) return;

  const text = composer.value.trim();
  const atts = pendingAttachments.slice();
  if (!text && atts.length === 0) return;

  inFlightSend.add(tid);
  composer.value = '';
  composer.style.height = 'auto';
  pendingAttachments = [];
  renderPendingAttachments();
  updateButtons();

  const threadRow = allThreads.find(x => x.id === tid);
  const noTitleYet =
    threadRow && (threadRow.title == null || threadRow.title === '');
  const noPriorUser = !allMessages.some(
    m => m.threadId === tid && m.role === 'user'
  );

  try {
    await window.stdb.sendMessage(tid, text, atts);
  } catch (err) {
    toast('err', err.message ?? String(err));
  } finally {
    inFlightSend.delete(tid);
    updateButtons();
    if (activeThreadId === tid) composer.focus();
  }

  // After the first message lands, ask the summarizer to title the
  // thread. Idempotent server-side; failures are silently ignored.
  if (noTitleYet && noPriorUser && window.stdb) {
    window.stdb.generateThreadTitle(tid).catch(() => {});
  }
});

function updateButtons() {
  const ready =
    connState === 'connected' &&
    configState.kind === 'configured' &&
    !!window.stdb;
  $('btn-new-thread').disabled = !ready;
  $('btn-new-thread-hero').disabled = !ready;
  const tid = activeThreadId;
  const locked = tid !== null && lockedThreads.has(tid);
  const busy = tid !== null && (locked || inFlightSend.has(tid));
  const canSend = ready && tid !== null && !busy;
  $('composer-input').disabled = !canSend;
  $('btn-send').disabled = !canSend;
  $('btn-attach').disabled = !canSend;
  $('btn-send').textContent = busy ? 'thinking…' : 'Send';
  $('btn-send').hidden = locked;
  $('btn-stop').hidden = !locked;
  $('btn-stop').disabled = !locked || lockedThreads.get(tid) === true;
  $('btn-stop').textContent = lockedThreads.get(tid) ? 'stopping…' : 'Stop';
}

let currentUserState = null;
let authMode = 'login'; // 'login' | 'signup' | 'forgot'

function setAuthMode(mode) {
  authMode = mode;
  const title = $('auth-title');
  const sub = $('auth-sub');
  const submit = $('auth-submit');
  const togglePrompt = $('toggle-prompt');
  const toggleLink = $('toggle-link');
  const forgotFoot = $('forgot-link').parentElement;
  const passField = $('auth-pass').closest('.auth-field');
  const nameField = $('auth-name-field');
  if (mode === 'signup') {
    title.textContent = 'Create an account';
    sub.textContent = 'Sign up to start chatting.';
    submit.textContent = 'Create account';
    togglePrompt.textContent = 'Already have an account?';
    toggleLink.textContent = 'Sign in';
    forgotFoot.hidden = true;
    nameField.hidden = false;
    $('auth-pass').autocomplete = 'new-password';
    passField.hidden = false;
  } else if (mode === 'forgot') {
    title.textContent = 'Reset password';
    sub.textContent = "Enter your email and we'll send you a reset link.";
    submit.textContent = 'Send reset link';
    togglePrompt.textContent = 'Remembered it?';
    toggleLink.textContent = 'Sign in';
    forgotFoot.hidden = true;
    nameField.hidden = true;
    passField.hidden = true;
  } else {
    title.textContent = 'Welcome to Agents';
    sub.textContent = 'Sign in to continue.';
    submit.textContent = 'Sign in';
    togglePrompt.textContent = "Don't have an account?";
    toggleLink.textContent = 'Sign up';
    forgotFoot.hidden = false;
    nameField.hidden = true;
    $('auth-pass').autocomplete = 'current-password';
    passField.hidden = false;
  }
}

$('toggle-link').addEventListener('click', () => {
  setAuthMode(authMode === 'login' ? 'signup' : 'login');
});
$('forgot-link').addEventListener('click', () => setAuthMode('forgot'));

$('auth-form').addEventListener('submit', async e => {
  e.preventDefault();
  if (!window.auth) return;
  const email = $('auth-email').value.trim();
  const password = $('auth-pass').value;
  const submit = $('auth-submit');
  submit.disabled = true;
  try {
    if (authMode === 'signup') {
      const name = $('auth-name').value.trim() || undefined;
      await window.auth.signup({ email, password, name });
    } else if (authMode === 'forgot') {
      await window.auth.forgotPassword(email);
      toast('ok', 'Reset link sent. Check the STDB module log (dev mailer).');
      setAuthMode('login');
    } else {
      await window.auth.login({ email, password });
    }
  } catch (err) {
    toast('err', err.message ?? String(err));
  } finally {
    submit.disabled = false;
  }
});

$('oauth-google').addEventListener('click', () =>
  window.auth?.oauthStart('google')
);
$('oauth-github').addEventListener('click', () =>
  window.auth?.oauthStart('github')
);

$('btn-logout').addEventListener('click', async () => {
  if (!window.auth) return;
  try {
    await window.auth.logout();
  } catch (err) {
    toast('err', err.message ?? String(err));
  }
});

function renderUserPanel() {
  const u = currentUserState;
  const av = $('user-avatar');
  const setAvatarInitial = () => {
    av.classList.remove('has-image');
    const initial = u
      ? (u.name?.trim() || u.email || '?').slice(0, 1).toUpperCase()
      : '?';
    av.replaceChildren(document.createTextNode(initial));
  };
  if (!u) {
    setAvatarInitial();
    $('user-name').textContent = 'Unavailable';
    return;
  }
  const imageUrl = typeof u.image === 'string' ? u.image.trim() : '';
  if (imageUrl) {
    av.classList.add('has-image');
    const img = document.createElement('img');
    img.alt = '';
    img.referrerPolicy = 'no-referrer';
    img.addEventListener('error', setAvatarInitial, { once: true });
    img.src = imageUrl;
    av.replaceChildren(img);
  } else {
    setAvatarInitial();
  }
  $('user-name').textContent = u.name?.trim() || u.email;
}

function applyConnStateToAvatar(state) {
  const av = $('user-avatar');
  av.classList.toggle('online', state === 'connected');
  av.classList.toggle('warn', state === 'connecting' || state === 'idle');
  $('user-status').textContent =
    state === 'connected'
      ? 'online'
      : state === 'connecting'
        ? 'connecting…'
        : state === 'idle'
          ? 'idle'
          : 'offline';
}

function showAuthView() {
  $('auth-shell').hidden = false;
  $('shell').hidden = true;
}
function showChatView() {
  $('auth-shell').hidden = true;
  $('shell').hidden = false;
}

function dismissBootSplash() {
  const splash = document.getElementById('bootSplash');
  if (!splash) return;
  splash.classList.add('fading');
  setTimeout(() => splash.remove(), 250);
}
window.addEventListener('auth:ready', () => {
  if (!currentUserState) showAuthView();
  dismissBootSplash();
});
setTimeout(dismissBootSplash, 4000);
window.addEventListener('auth:state', e => {
  currentUserState = e.detail.user;
  if (currentUserState) {
    showChatView();
    renderUserPanel();
  } else {
    showAuthView();
  }
});

window.addEventListener('stdb:connState', e => {
  applyConnStateToAvatar(e.detail.state);
});

// Initial view: assume auth-shell until auth:state proves otherwise.
showAuthView();
