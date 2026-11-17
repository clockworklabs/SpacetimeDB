const state = {
  conn: 'connecting',
  reactor: null,
  events: [],
  players: [],
  statuses: [],
  shop: [],
  activeTab: 'shop',
  currentIdentityHex: null,
  colorPaletteOpen: false,
};

const $ = id => document.getElementById(id);
const fmt = new Intl.NumberFormat();
const COOLANT_UNLOCK_LEVEL = 2;
const SURGE_UNLOCK_LEVEL = 2;
const PLAYER_COLORS = [
  '#22c7b8',
  '#ffce5c',
  '#52df8f',
  '#ff6a66',
  '#aee8ff',
  '#d28cff',
  '#ff9f6e',
  '#8ddf65',
];

// Upgrade lanes reuse the colors the reactor already assigns to each system:
// amber = power/surge, green = cooling/coolant, cyan = tap charges.
const UPGRADE_LANES = {
  power: {
    color: 'var(--accent-2)',
    count: 'powerUpgradeCount',
    icon: '<svg viewBox="0 0 40 40" fill="currentColor" aria-hidden="true"><path d="M22.8 8.8 12.6 22.1h7l-2.3 9.1 10.2-13.4h-7l2.3-9z"/></svg>',
  },
  cooling: {
    color: 'var(--ok)',
    count: 'coolingUpgradeCount',
    icon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"><path d="M4 7h16M4 12h16M4 17h16"/><path d="M9.5 7v10M14.5 7v10" opacity="0.4"/></svg>',
  },
  capacity: {
    color: '#aee8ff',
    count: 'capacityUpgradeCount',
    icon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 20h18"/><path d="M5 20V9M9.5 20V5M14.5 20V5M19 20V9"/></svg>',
  },
  charges: {
    color: '#5ed7ff',
    count: 'chargeUpgradeCount',
    icon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="7" width="16" height="10" rx="2"/><path d="M21 10v4"/><path d="M7.5 10v4M11.5 10v4" stroke-linecap="round"/></svg>',
  },
  bay: {
    color: 'var(--muted)',
    count: 'bayUpgradeCount',
    icon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 2h6"/><circle cx="12" cy="13" r="7"/><path d="M12 13V9.5"/></svg>',
  },
};

function upgradeLaneColor(id) {
  return UPGRADE_LANES[id]?.color ?? 'var(--accent)';
}

function upgradeLevel(id) {
  const lane = UPGRADE_LANES[id];
  if (!lane) return 0;
  return Number(state.reactor?.[lane.count] ?? 0);
}

function costPercent(energy, cost) {
  if (cost <= 0n) return 100;
  if (energy >= cost) return 100;
  return Number((energy * 100n) / cost);
}

function microsToMs(ts) {
  if (!ts || ts.microsSinceUnixEpoch == null) return 0;
  return Number(ts.microsSinceUnixEpoch / 1000n);
}

function eventTime(ts) {
  const ms = microsToMs(ts);
  return ms
    ? new Date(ms).toLocaleTimeString([], {
        hour: 'numeric',
        minute: '2-digit',
        second: '2-digit',
      })
    : '';
}

function identityHex(identity) {
  if (!identity) return '';
  if (typeof identity.toHexString === 'function') return identity.toHexString();
  return String(identity);
}

function secondsUntil(ts) {
  const ms = microsToMs(ts);
  if (!ms) return 0;
  return Math.max(0, Math.ceil((ms - Date.now()) / 1000));
}

function secondsUntilPrecise(ts) {
  const ms = microsToMs(ts);
  if (!ms) return 0;
  return Math.max(0, (ms - Date.now()) / 1000);
}

function formatSeconds(seconds) {
  if (seconds <= 0) return 'ready';
  return `${seconds.toFixed(1)}s`;
}

function effectiveReactor(reactor) {
  if (!reactor) return null;
  const elapsed = Math.max(
    0,
    (Date.now() - microsToMs(reactor.updatedAt)) / 1000
  );
  const cooling = Number(reactor.coolingPerSecond ?? 4);
  const capacity = Number(reactor.heatCapacity ?? 100);
  const heat = Math.max(0, Number(reactor.heat ?? 0) - elapsed * cooling);
  return {
    ...reactor,
    heat,
    heatCapacity: capacity,
    coolingPerSecond: cooling,
    overheated: Boolean(reactor.overheated) && heat > capacity * 0.45,
  };
}

function coolingText(reactor) {
  if (!reactor || reactor.heat <= 0) return 'stable';
  if (reactor.overheated) {
    const recoverAt = reactor.heatCapacity * 0.45;
    const seconds =
      Math.max(0, reactor.heat - recoverAt) / reactor.coolingPerSecond;
    return seconds > 0 ? `Cooling ${formatSeconds(seconds)}` : 'safe';
  }
  return `cooling ${formatSeconds(reactor.heat / reactor.coolingPerSecond)}`;
}

function statusByScope(scope) {
  return state.statuses.find(row => row.scope === scope);
}

function effectiveStatus(scope) {
  const row = statusByScope(scope);
  if (!row) return null;
  const seconds = secondsUntilPrecise(row.resetAt);
  if (seconds <= 0) {
    return {
      ...row,
      used: 0,
      remaining: row.limit,
    };
  }
  return row;
}

function isCooling(scope) {
  const row = effectiveStatus(scope);
  if (!row || row.remaining > 0) return false;
  return secondsUntil(row.resetAt) > 0;
}

function cooldownText(scope) {
  const row = effectiveStatus(scope);
  if (!row) return '';
  const seconds = secondsUntilPrecise(row.resetAt);
  if (row.remaining <= 0 && seconds > 0) {
    return scope === 'reactor.tap'
      ? `Resets in ${formatSeconds(seconds)}`
      : `Ready in ${formatSeconds(seconds)}`;
  }
  return `${row.remaining}/${row.limit} ready`;
}

function cooldownButtonText(scope) {
  const row = effectiveStatus(scope);
  const seconds = secondsUntilPrecise(row?.resetAt);
  return seconds > 0 ? formatSeconds(seconds) : 'Wait';
}

function tapChargeInfo(scope) {
  const row = effectiveStatus(scope);
  if (!row) return 'waiting';
  const seconds = secondsUntilPrecise(row.resetAt);
  const reset =
    seconds > 0
      ? `resets in ${formatSeconds(seconds)}`
      : `${row.windowSeconds}s window`;
  return `${row.remaining} / ${row.limit} · ${reset}`;
}

function renderChargeSegments(scope) {
  const row = effectiveStatus(scope);
  const node = $('tapChargeSegments');
  if (!node) return;
  const limit = row?.limit ?? 8;
  const remaining = row?.remaining ?? 0;
  node.style.setProperty('--charge-limit', String(Math.max(1, limit)));
  node.innerHTML = Array.from(
    { length: limit },
    (_value, index) =>
      `<i class="charge-segment ${index < remaining ? 'active' : ''}" aria-hidden="true"></i>`
  ).join('');
}

function hasCoolantFlush(reactor) {
  return Number(reactor?.coolingUpgradeCount ?? 0) >= COOLANT_UNLOCK_LEVEL;
}

function hasSurgeBurst(reactor) {
  return Number(reactor?.powerUpgradeCount ?? 0) >= SURGE_UNLOCK_LEVEL;
}

function romanLevel(value) {
  const numerals = [
    [10, 'X'],
    [9, 'IX'],
    [5, 'V'],
    [4, 'IV'],
    [1, 'I'],
  ];
  let n = Math.max(1, Math.min(20, Math.trunc(Number(value) || 1)));
  let out = '';
  for (const [amount, label] of numerals) {
    while (n >= amount) {
      out += label;
      n -= amount;
    }
  }
  return out;
}

function surgeLevel(reactor) {
  return romanLevel(reactor?.reactorLevel ?? 1);
}

function coolantLevel(reactor) {
  return romanLevel(Math.max(1, Number(reactor?.coolingUpgradeCount ?? 0)));
}

function surgePreview(reactor) {
  const level = reactor?.reactorLevel ?? 1;
  const heat = Math.max(28, Math.floor((reactor?.heatCapacity ?? 100) * 0.25));
  return `+${fmt.format(level * 10)} energy<br>+${heat} heat`;
}

function coolantAmount(reactor) {
  return Math.max(65, Math.floor((reactor?.heatCapacity ?? 100) * 0.55));
}

function coolantPreview(reactor) {
  return `Vent -${coolantAmount(reactor)} heat`;
}

function spawnReactorPop(text, kind = '', color = '') {
  const layer = $('popLayer');
  const pop = document.createElement('div');
  pop.className = `energy-pop ${kind}`;
  pop.textContent = text;
  pop.style.setProperty(
    '--pop-x',
    `${Math.round((Math.random() - 0.5) * 90)}px`
  );
  if (color) pop.style.setProperty('--pop-color', color);
  layer.appendChild(pop);
  window.setTimeout(() => pop.remove(), 850);
}

function spawnEventPop(event) {
  if (!event.allowed) {
    if (event.scope === 'reactor.tap')
      spawnReactorPop('WAIT', 'blocked', event.actorColor);
    return;
  }
  if (event.energyDelta > 0n) {
    const prefix =
      event.kind === 'overcharge' || event.kind === 'overcharge_overheated'
        ? 'SURGE +'
        : '+';
    const kind =
      event.kind === 'overcharge' || event.kind === 'overcharge_overheated'
        ? 'surge-pop'
        : '';
    spawnReactorPop(
      `${prefix}${fmt.format(event.energyDelta)}`,
      kind,
      event.actorColor
    );
    return;
  }
  if (event.kind === 'repair') {
    spawnReactorPop('VENT', 'cooling', event.actorColor);
  }
}

function setBusy(button, busy) {
  button.dataset.busy = busy ? 'true' : 'false';
}

function flashButton(button, allowed) {
  button.classList.remove('fired');
  void button.offsetWidth;
  if (allowed) button.classList.add('fired');
  window.setTimeout(() => button.classList.remove('fired'), 460);
}

async function callAction(button, fn) {
  if (button.dataset.busy === 'true') return;
  if (!window.reactor) throw new Error('reactor.not_ready');
  setBusy(button, true);
  try {
    const result = await fn();
    flashButton(button, result.allowed);
    render();
  } catch (err) {
    console.error('reactor action failed', err);
    render();
  } finally {
    setBusy(button, false);
  }
}

let energyShown = 0;
let energyTarget = 0;
let energyReady = false;
let energyTween = null;
let energyGainTimer = null;

function pulseEnergyGain() {
  const banner = document.querySelector('.energy-banner');
  if (!banner) return;
  banner.classList.remove('gain');
  void banner.offsetWidth;
  banner.classList.add('gain');
  window.clearTimeout(energyGainTimer);
  energyGainTimer = window.setTimeout(
    () => banner.classList.remove('gain'),
    460
  );
}

function writeEnergy() {
  $('energy').textContent = fmt.format(Math.round(energyShown));
}

function startEnergyTween() {
  if (energyTween != null) return;
  energyTween = window.setInterval(() => {
    const diff = energyTarget - energyShown;
    if (Math.abs(diff) < 0.5) {
      energyShown = energyTarget;
      writeEnergy();
      window.clearInterval(energyTween);
      energyTween = null;
      return;
    }
    energyShown += diff * 0.25;
    writeEnergy();
  }, 16);
}

function setEnergy(value) {
  const next = Number(value);
  if (!energyReady) {
    energyReady = true;
    energyShown = next;
    energyTarget = next;
    writeEnergy();
    return;
  }
  if (next > energyTarget) pulseEnergyGain();
  energyTarget = next;
  if (energyShown !== energyTarget) startEnergyTween();
}

function renderStats() {
  const reactor = effectiveReactor(state.reactor);
  const energy = reactor?.energy ?? 0n;
  const level = reactor?.reactorLevel ?? 1;
  const capacity = reactor?.heatCapacity ?? 100;
  const heat = reactor?.heat ?? 0;
  const heatPct = capacity <= 0 ? 0 : (heat / capacity) * 100;
  const overheated = Boolean(reactor?.overheated);
  const tapCooling = isCooling('reactor.tap');

  if (reactor) setEnergy(energy);
  $('heatText').textContent = `${heat.toFixed(1)} / ${capacity}`;
  $('coolText').textContent = coolingText(reactor);
  $('heatFill').style.width = `${Math.max(0, Math.min(100, heatPct))}%`;
  $('tapChargeInfo').textContent = tapChargeInfo('reactor.tap');
  renderChargeSegments('reactor.tap');

  $('reactorWrap').classList.toggle('warm', heatPct >= 35 && heatPct < 70);
  $('reactorWrap').classList.toggle('critical', overheated || heatPct >= 70);
  $('tapBtn').classList.toggle(
    'hot',
    overheated || heatPct >= 75 || tapCooling
  );
  $('tapBtn').disabled = state.conn !== 'connected' || overheated || tapCooling;
  $('reactorLabel').textContent = overheated
    ? 'Overheated'
    : tapCooling
      ? 'No Charges'
      : 'Tap';
  $('reactorHint').textContent = overheated
    ? coolingText(reactor)
    : tapCooling
      ? cooldownText('reactor.tap')
      : `+${level} energy · +${reactor?.tapHeatGain ?? 13} heat`;
}

function paneEmpty(title, sub) {
  if (state.conn !== 'connected') {
    const connecting =
      state.conn === 'error'
        ? ['Reconnecting', 'Lost the link to SpacetimeDB.']
        : ['Connecting', 'Linking up with SpacetimeDB.'];
    return `<div class="pane-empty"><i class="pane-spinner" aria-hidden="true"></i><b>${connecting[0]}</b><span>${connecting[1]}</span></div>`;
  }
  return `<div class="pane-empty"><b>${title}</b><span>${sub}</span></div>`;
}

function renderShop() {
  const reactor = effectiveReactor(state.reactor);
  const energy = reactor?.energy ?? 0n;
  const upgradeCooling = isCooling('reactor.upgrade');
  const rows = state.shop.filter(item => item.available);
  $('shopList').innerHTML =
    rows.length === 0
      ? paneEmpty('Loading upgrades', 'Fetching the reactor shop.')
      : rows
          .map(item => {
            const affordable = energy >= item.cost;
            const disabled =
              state.conn !== 'connected' || upgradeCooling || !affordable;
            const buttonText = upgradeCooling
              ? cooldownButtonText('reactor.upgrade')
              : 'Install';
            const lane = UPGRADE_LANES[item.id];
            const level = upgradeLevel(item.id);
            return `
      <div class="shop-item ${affordable ? 'affordable' : ''}" data-lane="${item.id}" style="--lane: ${upgradeLaneColor(item.id)}">
        <div class="shop-icon" aria-hidden="true">${lane?.icon ?? ''}</div>
        <div class="shop-head">
          <div class="shop-title"><b>${item.name}</b>${level > 0 ? `<i class="shop-level">Lv ${level}</i>` : ''}</div>
          <span class="shop-effect">${item.effect}</span>
          <p class="shop-desc">${item.description}</p>
          <div class="shop-cost">
            <div class="shop-cost-track"><div class="shop-cost-fill" style="width: ${costPercent(energy, item.cost)}%"></div></div>
            <span class="shop-cost-text">${fmt.format(energy)} / ${fmt.format(item.cost)}</span>
          </div>
        </div>
        <div class="shop-buy-row">
          <button class="primary upgrade-btn" type="button" data-upgrade-id="${item.id}" ${disabled ? 'disabled' : ''}>${buttonText}</button>
        </div>
      </div>
    `;
          })
          .join('');
}

function renderPlayers() {
  const rows = state.players ?? [];
  $('playerCount').textContent = rows.length.toString();
  $('playerList').innerHTML =
    rows.length === 0
      ? paneEmpty('No crew yet', 'Open another tab to bring a pilot online.')
      : rows
          .map(player => {
            const isCurrent =
              identityHex(player.identity) === state.currentIdentityHex;
            const colorControl = isCurrent
              ? `<div class="player-color-wrap">
          <button id="crewColorDot" class="color-dot" type="button" aria-label="Change your crew color" aria-expanded="${state.colorPaletteOpen ? 'true' : 'false'}"></button>
          <div id="crewColorPalette" class="color-palette player-palette ${state.colorPaletteOpen ? 'open' : ''}" aria-label="Crew colors">
            ${PLAYER_COLORS.map(
              swatch =>
                `<button class="color-swatch" type="button" data-color="${swatch}" style="--swatch: ${swatch}" aria-label="Use color ${swatch}"></button>`
            ).join('')}
          </div>
        </div>`
              : '<i class="player-color" aria-hidden="true"></i>';
            return `
    <div class="player-row ${isCurrent ? 'current' : ''}" style="--player-color: ${player.color}">
      ${colorControl}
      <div>
        <div class="player-meta"><b>${player.displayName}</b>${isCurrent ? '<i class="self-pill">You</i>' : ''}</div>
        <span>${player.taps} taps · ${player.surges} surges · ${player.coolantUses} vents</span>
      </div>
      <div class="player-score">${fmt.format(player.contributedEnergy)}</div>
    </div>
  `;
          })
          .join('');
}

function activityChip(event) {
  if (!event.allowed) return { className: 'blocked', label: 'Wait' };
  if (event.kind === 'tap' || event.kind === 'tap_overheated')
    return { className: 'tap', label: 'Tap' };
  if (event.kind === 'overcharge' || event.kind === 'overcharge_overheated')
    return { className: 'surge', label: 'Surge' };
  if (event.kind === 'repair') return { className: 'repair', label: 'Vent' };
  if (event.kind === 'upgrade') return { className: 'upgrade', label: 'Shop' };
  return { className: '', label: 'Room' };
}

function renderActivity() {
  const rows = (state.events ?? []).slice(0, 18);
  $('activityCount').textContent = rows.length.toString();
  $('activityList').innerHTML =
    rows.length === 0
      ? paneEmpty('No activity yet', 'Tap the core to start the feed.')
      : rows
          .map(event => {
            const delta =
              event.energyDelta === 0n
                ? ''
                : ` ${event.energyDelta > 0n ? '+' : ''}${event.energyDelta.toString()} energy`;
            const chip = activityChip(event);
            return `
      <div class="activity-row" style="--player-color: ${event.actorColor}">
        <span class="event-chip ${chip.className}">${chip.label}</span>
        <b>${event.actorName}${delta}</b>
        <time>${eventTime(event.createdAt)}</time>
      </div>
    `;
          })
          .join('');
}

function renderTabs() {
  $('shopCount').textContent = state.shop.length.toString();
  for (const tab of document.querySelectorAll('[data-tab]')) {
    const active = tab.dataset.tab === state.activeTab;
    tab.setAttribute('aria-selected', active ? 'true' : 'false');
  }
  for (const pane of document.querySelectorAll('[data-pane]')) {
    pane.classList.toggle('active', pane.dataset.pane === state.activeTab);
  }
}

function renderSystems() {
  const reactor = effectiveReactor(state.reactor);
  const heat = reactor?.heat ?? 0;
  const overheated = Boolean(reactor?.overheated);
  const systems = [];
  const coolantUnlocked = hasCoolantFlush(reactor);
  const surgeUnlocked = hasSurgeBurst(reactor);

  const repairCooling = isCooling('reactor.repair');
  const useful = overheated || heat >= 35;
  if (coolantUnlocked) {
    systems.push(`
    <div class="system-control">
      <button id="repairBtn" class="coolant" type="button" ${state.conn !== 'connected' || repairCooling || !useful ? 'disabled' : ''}>
        <span>Coolant ${coolantLevel(reactor)}</span>
        <span id="repairHint" class="button-sub">${repairCooling ? cooldownText('reactor.repair') : coolantPreview(reactor)}</span>
      </button>
      <div id="repairCooldown" class="cooldown ${repairCooling ? 'blocked' : ''}">${repairCooling ? cooldownText('reactor.repair') : ''}</div>
    </div>
  `);
  }

  const surgeCooling = isCooling('reactor.overcharge');
  if (surgeUnlocked) {
    systems.push(`
    <div class="system-control">
      <button id="overchargeBtn" class="surge" type="button" ${state.conn !== 'connected' || overheated || surgeCooling ? 'disabled' : ''}>
        <span>Surge ${surgeLevel(reactor)}</span>
        <span id="overchargeHint" class="button-sub">${overheated ? 'Core cooling' : surgeCooling ? cooldownText('reactor.overcharge') : surgePreview(reactor)}</span>
      </button>
      <div id="overchargeCooldown" class="cooldown ${surgeCooling ? 'blocked' : ''}">${surgeCooling ? cooldownText('reactor.overcharge') : ''}</div>
    </div>
  `);
  }

  $('systemDock').innerHTML = systems.join('');
}

function updateShopTimers() {
  const reactor = effectiveReactor(state.reactor);
  const energy = reactor?.energy ?? 0n;
  const upgradeCooling = isCooling('reactor.upgrade');

  for (const item of state.shop.filter(row => row.available)) {
    const button = document.querySelector(
      `.upgrade-btn[data-upgrade-id="${item.id}"]`
    );
    if (button instanceof HTMLButtonElement) {
      const affordable = energy >= item.cost;
      button.disabled =
        state.conn !== 'connected' || upgradeCooling || !affordable;
      button.textContent = upgradeCooling
        ? cooldownButtonText('reactor.upgrade')
        : 'Install';

      const row = button.closest('.shop-item');
      if (row instanceof HTMLElement) {
        row.classList.toggle('affordable', affordable);
        const fill = row.querySelector('.shop-cost-fill');
        if (fill instanceof HTMLElement)
          fill.style.width = `${costPercent(energy, item.cost)}%`;
        const costText = row.querySelector('.shop-cost-text');
        if (costText)
          costText.textContent = `${fmt.format(energy)} / ${fmt.format(item.cost)}`;
      }
    }
  }
}

function updateSystemTimers() {
  const reactor = effectiveReactor(state.reactor);
  const heat = reactor?.heat ?? 0;
  const overheated = Boolean(reactor?.overheated);

  const repairBtn = $('repairBtn');
  if (repairBtn) {
    const cooling = isCooling('reactor.repair');
    const useful = overheated || heat >= 35;
    repairBtn.disabled = state.conn !== 'connected' || cooling || !useful;
    const hint = $('repairHint');
    if (hint)
      hint.textContent = cooling
        ? cooldownText('reactor.repair')
        : coolantPreview(reactor);
    const cooldown = $('repairCooldown');
    if (cooldown) {
      cooldown.textContent = cooling ? cooldownText('reactor.repair') : '';
      cooldown.classList.toggle('blocked', cooling);
    }
  }

  const surgeBtn = $('overchargeBtn');
  if (surgeBtn) {
    const cooling = isCooling('reactor.overcharge');
    surgeBtn.disabled = state.conn !== 'connected' || overheated || cooling;
    const hint = $('overchargeHint');
    if (hint)
      hint.innerHTML = overheated
        ? 'Core cooling'
        : cooling
          ? cooldownText('reactor.overcharge')
          : surgePreview(reactor);
    const cooldown = $('overchargeCooldown');
    if (cooldown) {
      cooldown.textContent = cooling ? cooldownText('reactor.overcharge') : '';
      cooldown.classList.toggle('blocked', cooling);
    }
  }
}

function render() {
  renderStats();
  renderTabs();
  renderShop();
  renderSystems();
  renderPlayers();
  renderActivity();
}

function renderLive() {
  renderStats();
  updateShopTimers();
  updateSystemTimers();
}

function animationLoop() {
  renderLive();
  requestAnimationFrame(animationLoop);
}

window.addEventListener('reactor:connState', ev => {
  state.conn = ev.detail.state;
  if (ev.detail.detail) {
    console.warn('reactor connection state', ev.detail);
  }
  render();
});

window.addEventListener('reactor:data', ev => {
  state.reactor = ev.detail.state;
  state.events = ev.detail.events ?? [];
  state.players = ev.detail.players ?? [];
  state.statuses = ev.detail.statuses ?? [];
  state.shop = ev.detail.shop ?? [];
  state.currentIdentityHex = ev.detail.currentIdentityHex ?? null;
  render();
});

window.addEventListener('reactor:eventInserted', ev => {
  spawnEventPop(ev.detail.event);
});

window.addEventListener('reactor:ready', () => {
  render();
});

$('tapBtn').addEventListener('click', () =>
  callAction($('tapBtn'), () => window.reactor.tap())
);
$('playerList').addEventListener('click', ev => {
  const target = ev.target instanceof Element ? ev.target : null;
  if (!target) return;

  if (target.closest('#crewColorDot')) {
    state.colorPaletteOpen = !state.colorPaletteOpen;
    renderPlayers();
    return;
  }

  const button = target.closest('[data-color]');
  if (!(button instanceof HTMLButtonElement)) return;
  const color = button.dataset.color;
  if (!color || !window.reactor) return;
  state.colorPaletteOpen = false;
  window.reactor
    .setPlayerColor(color)
    .catch(err => console.error('setPlayerColor failed', err));
  renderPlayers();
});
document.addEventListener('pointerdown', ev => {
  if (!state.colorPaletteOpen) return;
  const target = ev.target instanceof Element ? ev.target : null;
  if (target?.closest('#crewColorDot, #crewColorPalette')) return;
  state.colorPaletteOpen = false;
  renderPlayers();
});
document.addEventListener('keydown', ev => {
  if (ev.key !== 'Escape' || !state.colorPaletteOpen) return;
  state.colorPaletteOpen = false;
  renderPlayers();
});
$('shopList').addEventListener('click', ev => {
  const button =
    ev.target instanceof Element ? ev.target.closest('.upgrade-btn') : null;
  if (button instanceof HTMLButtonElement) {
    const upgradeId = button.dataset.upgradeId;
    if (upgradeId)
      callAction(button, () => window.reactor.buyUpgrade(upgradeId));
  }
});
document.querySelector('.tabs').addEventListener('click', ev => {
  const button =
    ev.target instanceof Element ? ev.target.closest('[data-tab]') : null;
  if (!(button instanceof HTMLButtonElement)) return;
  state.activeTab = button.dataset.tab;
  renderTabs();
});
$('systemDock').addEventListener('click', ev => {
  const button =
    ev.target instanceof Element ? ev.target.closest('button') : null;
  if (!(button instanceof HTMLButtonElement)) return;
  if (button.id === 'repairBtn')
    callAction(button, () => window.reactor.repair());
  if (button.id === 'overchargeBtn')
    callAction(button, () => window.reactor.overcharge());
});
render();
requestAnimationFrame(animationLoop);
