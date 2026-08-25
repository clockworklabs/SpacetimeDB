import { DbConnection, type ErrorContext } from './codegen';

import {
  TOKEN_KEY_PREFIX,
  MATCH_FALLBACK_MS,
  shipClasses,
  maneuverSlots,
  shipColors,
  maneuverSlotMeta,
  maneuverFx,
  selectActiveRoom,
  selectHighlightedManeuver,
  selectLatestDuel,
  selectLatestTicket,
  selectLobbyScreen,
  type ServerConfig,
  type TableEvents,
  type ShipClass,
  type ManeuverSlot,
  type Pilot,
  type LobbyTicket,
  type LobbyRoom,
  type LobbySeat,
  type Duel,
  type Combatant,
  type ShipCatalogRow,
  type ManeuverCatalogRow,
  type DuelManeuver,
  type RoundLog,
  type QueueSummary,
  type RatingRow,
} from './model';

let conn: DbConnection | null = null;
let me = '';
let selectedShip: ShipClass = 'Interceptor';
let screenOverride: 'setup' | null = null;
let fallbackTimer: ReturnType<typeof setTimeout> | null = null;
let autoJoinedRoom: string | null = null;
let playedRoomId: string | null = null;
let arenaSignature = '';
let animatedRound = -1;
const prevVitals = new Map<string, { hull: number; shields: number }>();

// Maneuver cards appear on hover or focus for elements with [data-maneuver-id].
function setupTooltip(): void {
  const tip = $('tooltip');
  let current: Element | null = null;
  let hideTimer: ReturnType<typeof setTimeout> | null = null;
  let showTimer: ReturnType<typeof setTimeout> | null = null;

  const place = (el: Element) => {
    const r = el.getBoundingClientRect();
    const tr = tip.getBoundingClientRect();
    const left = Math.max(
      8,
      Math.min(
        r.left + r.width / 2 - tr.width / 2,
        window.innerWidth - tr.width - 8
      )
    );
    const above = r.top - tr.height - 8;
    tip.style.left = `${Math.round(left)}px`;
    tip.style.top = `${Math.round(above < 8 ? r.bottom + 8 : above)}px`;
  };

  const show = (el: Element) => {
    const id = el.getAttribute('data-maneuver-id');
    const move = id
      ? rows(maneuverCatalogTable()).find(m => m.maneuverId === id)
      : undefined;
    if (!move) return;
    const meta = maneuverSlotMeta[move.slot.tag];
    const fx = maneuverFx(move);
    tip.style.setProperty('--slot', meta.color);
    tip.innerHTML = `
      <div class="tip-head"><span class="tip-slot">${meta.label}</span><span class="tip-name">${escapeHtml(move.name)}</span></div>
      ${fx ? `<div class="tip-fx">${fx}</div>` : ''}
      <div class="tip-desc">${escapeHtml(move.description)}</div>
    `;
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }
    if (showTimer) {
      clearTimeout(showTimer);
      showTimer = null;
    }
    tip.hidden = false;
    place(el);
    showTimer = setTimeout(() => tip.classList.add('show'), 10);
  };

  const hide = () => {
    if (showTimer) {
      clearTimeout(showTimer);
      showTimer = null;
    }
    tip.classList.remove('show');
    hideTimer = setTimeout(() => {
      tip.hidden = true;
    }, 130);
  };

  const enter = (target: EventTarget | null) => {
    const el =
      target instanceof Element ? target.closest('[data-maneuver-id]') : null;
    if (!el || el === current) return;
    current = el;
    show(el);
  };
  const leave = (related: EventTarget | null) => {
    if (!current) return;
    if (related instanceof Node && current.contains(related)) return;
    current = null;
    hide();
  };

  document.addEventListener('pointerover', ev => enter(ev.target));
  document.addEventListener('pointerout', ev => leave(ev.relatedTarget));
  document.addEventListener('focusin', ev => enter(ev.target));
  document.addEventListener('focusout', () => {
    if (current) {
      current = null;
      hide();
    }
  });
  window.addEventListener(
    'scroll',
    () => {
      if (current) {
        current = null;
        hide();
      }
    },
    true
  );
  window.addEventListener('resize', () => {
    if (current) {
      current = null;
      hide();
    }
  });
}

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el;
}

function input(id: string): HTMLInputElement {
  return $(id) as HTMLInputElement;
}

function dialog(id: string): HTMLDialogElement {
  return $(id) as HTMLDialogElement;
}

function setText(id: string, value: string): void {
  $(id).textContent = value;
}

function defaultDisplayName(): string {
  return me ? `Pilot ${me.slice(0, 6).toUpperCase()}` : 'Pilot';
}

function clearFallbackTimer(): void {
  if (fallbackTimer == null) return;
  clearTimeout(fallbackTimer);
  fallbackTimer = null;
}

function scheduleAiFallback(): void {
  clearFallbackTimer();
  fallbackTimer = setTimeout(async () => {
    fallbackTimer = null;
    const ticket = latestTicket();
    if (!ticket || ticket.status.tag !== 'Queued') return;
    try {
      showToast('No rival found. Launching vs Arena AI.');
      requireConn().reducers.fallbackToAi({});
    } catch (err) {
      showToast(err instanceof Error ? err.message : String(err), 'error');
    }
  }, MATCH_FALLBACK_MS);
}

function setScreen(id: 'setupScreen' | 'waitingScreen' | 'duelScreen'): void {
  for (const screen of ['setupScreen', 'waitingScreen', 'duelScreen']) {
    $(screen).classList.toggle('active', screen === id);
  }
}

function showToast(message: string, kind: 'ok' | 'error' = 'ok'): void {
  const el = $('toast');
  el.textContent = message;
  el.className = `toast ${kind}`;
}

function errorMessage(err: unknown): string {
  const message = err instanceof Error ? err.message : String(err);
  if (message.includes('duel.invalid_display_name'))
    return 'Enter a name to start.';
  if (message.includes('duel.invalid_ship_class')) return 'Choose a ship.';
  if (message.includes('duel.room_not_found'))
    return 'That duel is unavailable.';
  if (message.includes('duel.not_in_room'))
    return 'Join the duel before advancing.';
  if (message.includes('duel.not_active')) return 'This duel is not active.';
  return message;
}

// Suppress expected leave and teardown errors after a duel or room has ended.
function isBenignDuelError(err: unknown): boolean {
  const message = err instanceof Error ? err.message : String(err);
  return /room_closed|not_in_room|room_not_found|duel_abandoned|duel_not_ready|not_active/i.test(
    message
  );
}

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

function pct(current: number, max: number): number {
  if (max <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((current / max) * 100)));
}

function requireConn(): DbConnection {
  if (!conn) throw new Error('stdb.disconnected');
  return conn;
}

function rows<T>(source: TableEvents<T>): T[] {
  return [...source.iter()];
}

function profileTable(): TableEvents<Pilot> {
  return requireConn().db.myProfile;
}
function playersTable(): TableEvents<Pilot> {
  return requireConn().db.players;
}

function displayNameFor(subject: string): string {
  const player = rows(playersTable()).find(row => row.subject === subject);
  return player?.displayName ?? `Pilot ${subject.slice(0, 6).toUpperCase()}`;
}
function ticketsTable(): TableEvents<LobbyTicket> {
  return requireConn().db.myLobbyTickets;
}
function roomsTable(): TableEvents<LobbyRoom> {
  return requireConn().db.myLobbyRooms;
}
function seatsTable(): TableEvents<LobbySeat> {
  return requireConn().db.myLobbyRoomSeats;
}
function summaryTable(): TableEvents<QueueSummary> {
  return requireConn().db.lobbyQueueSummary;
}
function ratingsTable(): TableEvents<RatingRow> {
  return requireConn().db.myLobbyRatings;
}
function leaderboardTable(): TableEvents<RatingRow> {
  return requireConn().db.lobbyRankedLeaderboard;
}
function shipCatalogTable(): TableEvents<ShipCatalogRow> {
  return requireConn().db.shipCatalog;
}
function maneuverCatalogTable(): TableEvents<ManeuverCatalogRow> {
  return requireConn().db.maneuverCatalog;
}
function duelsTable(): TableEvents<Duel> {
  return requireConn().db.myDuels;
}
function combatantsTable(): TableEvents<Combatant> {
  return requireConn().db.myDuelCombatants;
}
function logsTable(): TableEvents<RoundLog> {
  return requireConn().db.myDuelRoundLogs;
}
function maneuversTable(): TableEvents<DuelManeuver> {
  return requireConn().db.myDuelManeuvers;
}

function tokenKey(config: ServerConfig): string {
  return `${TOKEN_KEY_PREFIX}:${config.stdbUri}:${config.database}`;
}

function loadToken(config: ServerConfig): string | undefined {
  try {
    return sessionStorage.getItem(tokenKey(config)) ?? undefined;
  } catch {
    return undefined;
  }
}

function saveToken(config: ServerConfig, token: string): void {
  try {
    sessionStorage.setItem(tokenKey(config), token);
  } catch {
    /* Storage can be unavailable. */
  }
}

function clearToken(config: ServerConfig): void {
  try {
    sessionStorage.removeItem(tokenKey(config));
  } catch {
    /* Storage can be unavailable. */
  }
}

function isStaleTokenError(err: unknown): boolean {
  const message = err instanceof Error ? err.message : String(err);
  return (
    message.includes('Failed to verify token') ||
    message.includes('Unauthorized')
  );
}

async function loadConfig(): Promise<ServerConfig> {
  const r = await fetch('/api/config');
  if (!r.ok) throw new Error(`/api/config returned ${r.status}`);
  return (await r.json()) as ServerConfig;
}

function connectOnce(
  config: ServerConfig,
  token?: string
): Promise<DbConnection> {
  return new Promise((resolve, reject) => {
    let builder = DbConnection.builder()
      .withUri(config.stdbUri)
      .withDatabaseName(config.database);
    if (token) builder = builder.withToken(token);
    builder
      .onConnect((c, identity, token) => {
        conn = c;
        me = identity.toHexString();
        if (token) saveToken(config, token);
        resolve(c);
      })
      .onDisconnect((_ctx, err) => {
        showToast(err?.message ?? 'Disconnected.', 'error');
      })
      .onConnectError((_ctx, err) => reject(err))
      .build();
  });
}

async function connect(config: ServerConfig): Promise<DbConnection> {
  const token = loadToken(config);
  try {
    return await connectOnce(config, token);
  } catch (err) {
    if (!token || !isStaleTokenError(err)) throw err;
    clearToken(config);
    showToast('Session expired. Reconnecting.', 'error');
    return connectOnce(config);
  }
}

function currentProfile(): Pilot | undefined {
  return rows(profileTable())[0];
}

function latestTicket(): LobbyTicket | undefined {
  return selectLatestTicket(rows(ticketsTable()));
}

function latestDuel(): Duel | undefined {
  return selectLatestDuel(rows(duelsTable()), latestTicket());
}

function activeRoom(): LobbyRoom | undefined {
  return selectActiveRoom(rows(roomsTable()), latestDuel(), latestTicket());
}

function selectedShipCatalog(): ShipCatalogRow | undefined {
  return rows(shipCatalogTable()).find(
    row => row.shipClass.tag === selectedShip
  );
}

function statRows(ship: ShipCatalogRow): Array<[string, number, string]> {
  return [
    ['Hull', pct(ship.hull, 160), String(ship.hull)],
    ['Shields', pct(ship.shields, 80), String(ship.shields)],
    ['Attack', pct(ship.attack, 38), String(ship.attack)],
    ['Speed', pct(ship.speed, 8), String(ship.speed)],
    ['Crit', pct(ship.critBps, 1400), `${Math.round(ship.critBps / 100)}%`],
    ['Dodge', pct(ship.dodgeBps, 2200), `${Math.round(ship.dodgeBps / 100)}%`],
  ];
}

function renderShipCarousel(): void {
  const details = selectedShipCatalog();
  const card = document.querySelector('.launch-card');
  if (card instanceof HTMLElement)
    card.style.setProperty('--ship', shipColors[selectedShip]);
  setText('selectedShipRole', details?.role ?? 'Loading');
  setText('selectedShipName', selectedShip);
  setText(
    'selectedShipDescription',
    details?.description ?? 'Loading ship catalog from SpacetimeDB.'
  );
  $('selectedShipVisual').className =
    `ship-preview ${selectedShip.toLowerCase()}`;
  $('shipRoster').innerHTML = shipClasses
    .map(
      cls => `
    <button type="button" role="tab" class="ship-chip ${cls.toLowerCase()} ${cls === selectedShip ? 'active' : ''}" data-ship="${cls}" style="--chip: ${shipColors[cls]}" aria-selected="${cls === selectedShip}">
      <i class="chip-ship" aria-hidden="true"></i>
      <span>${cls}</span>
    </button>
  `
    )
    .join('');
  $('selectedShipStats').innerHTML = details
    ? statRows(details)
        .map(
          ([label, pctValue, value]) => `
    <div class="ship-stat">
      <div class="ship-stat-label"><span>${label}</span><strong>${value}</strong></div>
      <div class="ship-stat-track"><i style="width:${pctValue}%"></i></div>
    </div>
  `
        )
        .join('')
    : '<div class="empty wide">Loading ship catalog.</div>';

  const moves = rows(maneuverCatalogTable())
    .filter(move => move.shipClass.tag === selectedShip)
    .sort(
      (a, b) =>
        maneuverSlots.indexOf(a.slot.tag) - maneuverSlots.indexOf(b.slot.tag)
    );
  $('shipAbilities').innerHTML =
    moves.length === 0
      ? ''
      : `
    <span class="eyebrow">Maneuvers</span>
    <div class="ability-cards compact">
      ${moves
        .map(move => {
          const meta = maneuverSlotMeta[move.slot.tag];
          return `
        <div class="ability-card" style="--slot: ${meta.color}" data-maneuver-id="${move.maneuverId}" tabindex="0">
          <div class="maneuver-top"><span class="maneuver-icon">${meta.icon}</span><span class="maneuver-slot">${meta.label}</span></div>
          <strong>${escapeHtml(move.name)}</strong>
        </div>
      `;
        })
        .join('')}
    </div>
  `;
}

async function chooseShip(ship: ShipClass): Promise<void> {
  selectedShip = ship;
  renderShipCarousel();
  requireConn().reducers.selectShip({ shipClass: { tag: ship } });
}

function renderProfile(): void {
  const profile = currentProfile();
  selectedShip = profile?.shipClass.tag ?? selectedShip;
  const nameInput = input('displayName');
  const nextName = profile?.displayName ?? defaultDisplayName();
  if (document.activeElement !== nameInput || !nameInput.value.trim()) {
    nameInput.value = nextName;
  }
  renderShipCarousel();
}

function renderRanked(): void {
  const rating = rows(ratingsTable()).find(
    row => row.pool === 'spaceship_duel'
  );
  setText('myRating', String(rating?.rating ?? 1000));
  setText(
    'myRecord',
    rating
      ? `${rating.wins}W ${rating.losses}L${rating.draws ? ` ${rating.draws}D` : ''}`
      : '0W 0L'
  );
  const leaderboard = rows(leaderboardTable())
    .filter(
      row => row.pool === 'spaceship_duel' && !row.subject.startsWith('ai:')
    )
    .sort(
      (a, b) =>
        b.rating - a.rating ||
        b.wins - a.wins ||
        a.subject.localeCompare(b.subject)
    )
    .slice(0, 10);
  $('leaderboard').innerHTML =
    leaderboard.length === 0
      ? '<p class="leaderboard-empty">No ranked pilots yet. Win a duel to get on the board.</p>'
      : leaderboard
          .map(
            (row, index) => `
      <div class="leaderboard-row ${row.subject === me ? 'me' : ''}">
        <span class="lb-rank rank-${index + 1}">${index + 1}</span>
        <strong>${escapeHtml(displayNameFor(row.subject))}${row.subject === me ? ' <span class="lb-you">(you)</span>' : ''}</strong>
        <em>${row.rating}</em>
      </div>
    `
          )
          .join('');
}

function maybeAutoJoin(): void {
  if (screenOverride === 'setup') return;
  const room = activeRoom();
  if (!room) return;
  const mySeat = rows(seatsTable()).find(
    seat => seat.roomId === room.roomId && seat.subject === me
  );
  if (!mySeat || mySeat.status.tag === 'Joined') return;
  const key = room.roomId.toString();
  if (autoJoinedRoom === key) return;
  autoJoinedRoom = key;
  try {
    requireConn().reducers.joinDuelRoom({ roomId: room.roomId });
  } catch (err) {
    autoJoinedRoom = null;
    if (!isBenignDuelError(err)) showToast(errorMessage(err), 'error');
  }
}

function renderLobby(): void {
  const room = activeRoom();
  $('forfeitDuel').toggleAttribute('disabled', !room);
  maybeAutoJoin();
}

function closeForfeitDialog(): void {
  const modal = dialog('forfeitDialog');
  if (modal.open) modal.close();
}

async function forfeitActiveDuel(): Promise<void> {
  const room = activeRoom();
  autoJoinedRoom = null;
  screenOverride = 'setup';
  closeForfeitDialog();
  render();
  if (!room) return;
  try {
    requireConn().reducers.leaveDuel({ roomId: room.roomId });
  } catch (err) {
    // Surface only actionable errors while leaving a room that may already be closed.
    if (!isBenignDuelError(err)) showToast(errorMessage(err), 'error');
  }
}

function desiredScreen(): 'setupScreen' | 'waitingScreen' | 'duelScreen' {
  const duel = latestDuel();
  const ticket = latestTicket();
  return selectLobbyScreen({
    screenOverride,
    ticket,
    duel,
    room: activeRoom(),
    playedRoomId,
  });
}

function combatantCardHtml(row: Combatant): string {
  return `
    <article class="combatant ${row.subject === me ? 'mine' : ''}" data-subject="${row.subject}">
      <div class="ship-visual ${row.shipClass.tag.toLowerCase()}"></div>
      <div class="pops"></div>
      <div>
        <span class="eyebrow">${row.subject === me ? 'Your ship' : 'Opponent'}</span>
        <h3>${escapeHtml(row.displayName)}</h3>
        <p>${row.shipClass.tag}</p>
      </div>
      <div class="ship-abilities"></div>
      <div class="bar-block">
        <div class="bar-label"><span>Hull</span><strong class="hull-text"></strong></div>
        <div class="bar"><i class="hull-fill"></i></div>
        <div class="bar-label"><span>Shields</span><strong class="shield-text"></strong></div>
        <div class="bar shield"><i class="shield-fill"></i></div>
      </div>
    </article>
  `;
}

function highlightedManeuverFor(
  duel: Duel | undefined,
  row: Combatant
): DuelManeuver | undefined {
  return selectHighlightedManeuver(duel, row, rows(maneuversTable()));
}

function abilitiesHtml(duel: Duel | undefined, row: Combatant): string {
  const abilities = rows(maneuverCatalogTable())
    .filter(move => move.shipClass.tag === row.shipClass.tag)
    .sort(
      (a, b) =>
        maneuverSlots.indexOf(a.slot.tag) - maneuverSlots.indexOf(b.slot.tag)
    );
  if (abilities.length === 0) return '';
  const highlighted = highlightedManeuverFor(duel, row);
  return `
    <span class="eyebrow">Abilities</span>
    <div class="ability-cards compact">
      ${abilities
        .map(move => {
          const meta = maneuverSlotMeta[move.slot.tag];
          return `
        <div class="ability-card ${highlighted?.maneuverId === move.maneuverId ? 'active' : ''}" style="--slot: ${meta.color}" data-maneuver-id="${move.maneuverId}" tabindex="0">
          <div class="maneuver-top"><span class="maneuver-icon">${meta.icon}</span><span class="maneuver-slot">${meta.label}</span></div>
          <strong>${escapeHtml(move.name)}</strong>
        </div>
      `;
        })
        .join('')}
    </div>
  `;
}

function flashCard(card: HTMLElement, kind: string): void {
  const cls = `flash-${kind}`;
  card.classList.remove('flash-hit', 'flash-crit', 'flash-shield');
  void card.offsetWidth;
  card.classList.add(cls);
  setTimeout(() => card.classList.remove(cls), 420);
}

function shakeCard(card: HTMLElement, hard: boolean): void {
  const cls = hard ? 'shake-hard' : 'shake';
  card.classList.remove('shake', 'shake-hard');
  void card.offsetWidth;
  card.classList.add(cls);
  setTimeout(() => card.classList.remove(cls), 480);
}

function lungeCard(card: HTMLElement): void {
  card.classList.add('attacking');
  setTimeout(() => card.classList.remove('attacking'), 200);
}

function popNumber(card: HTMLElement, text: string, kind: string): void {
  const layer = card.querySelector('.pops');
  if (!layer) return;
  const pop = document.createElement('div');
  pop.className = `pop ${kind}`;
  pop.textContent = text;
  pop.style.setProperty(
    '--pop-x',
    `${Math.round((Math.random() - 0.5) * 44)}px`
  );
  layer.appendChild(pop);
  setTimeout(() => pop.remove(), 900);
}

function animateRound(
  host: HTMLElement,
  duel: Duel,
  combatants: Combatant[],
  round: number
): void {
  const cards = new Map<string, HTMLElement>();
  for (const el of host.querySelectorAll('.combatant')) {
    cards.set((el as HTMLElement).dataset.subject ?? '', el as HTMLElement);
  }
  const roundLogs = rows(logsTable()).filter(
    row => row.roomId === duel.roomId && row.round === round
  );
  const critRound = roundLogs.some(row => /critical/i.test(row.message));
  const hadEvade = roundLogs.some(row => /evade|miss/i.test(row.message));

  const damage = combatants.map(c => {
    const prev = prevVitals.get(c.subject);
    const prevHull = prev?.hull ?? c.hull;
    const prevShields = prev?.shields ?? c.shields;
    const dmg = Math.max(0, prevHull + prevShields - (c.hull + c.shields));
    const shieldOnly = prevHull === c.hull && prevShields > c.shields;
    return { subject: c.subject, dmg, shieldOnly };
  });
  const maxDmg = Math.max(0, ...damage.map(d => d.dmg));

  for (const d of damage) {
    const card = cards.get(d.subject);
    if (!card) continue;
    if (d.dmg > 0) {
      const isCrit = critRound && !d.shieldOnly && d.dmg === maxDmg;
      const kind = d.shieldOnly ? 'shield' : isCrit ? 'crit' : 'hit';
      popNumber(card, `-${d.dmg}`, kind);
      flashCard(card, kind);
      shakeCard(card, isCrit);
      for (const other of cards) {
        if (other[0] !== d.subject) lungeCard(other[1]);
      }
    } else if (hadEvade) {
      popNumber(card, 'EVADE', 'evade');
    }
  }
}

function renderManeuvers(
  duel: Duel | undefined,
  combatants: Combatant[],
  complete: boolean
): void {
  const host = $('maneuverActions');
  if (!duel || complete || duel.status.tag !== 'Active') {
    host.innerHTML = '';
    host.hidden = true;
    return;
  }
  const mine = combatants.find(row => row.subject === me);
  if (!mine) {
    host.innerHTML = '';
    host.hidden = true;
    return;
  }
  const round = duel.round + 1;
  const chosen = rows(maneuversTable()).find(
    row =>
      row.roomId === duel.roomId && row.round === round && row.subject === me
  );
  const catalog = rows(maneuverCatalogTable())
    .filter(row => row.shipClass.tag === mine.shipClass.tag)
    .sort(
      (a, b) =>
        maneuverSlots.indexOf(a.slot.tag) - maneuverSlots.indexOf(b.slot.tag)
    );
  host.hidden = false;
  host.innerHTML = `
    <div class="maneuver-head">
      <span class="eyebrow ${chosen ? 'locked' : ''}">${chosen ? '&#10003; Locked in | waiting for opponent' : `Choose your maneuver | Round ${round}`}</span>
    </div>
    <div class="maneuver-grid">
      ${catalog
        .map(row => {
          const meta = maneuverSlotMeta[row.slot.tag];
          const isChosen = chosen?.slot.tag === row.slot.tag;
          return `
        <button
          type="button"
          class="maneuver-button ${isChosen ? 'active' : ''} ${chosen && !isChosen ? 'dimmed' : ''}"
          data-slot="${row.slot.tag}"
          style="--slot: ${meta.color}"
          title="${escapeHtml(row.description)}"
          ${chosen ? 'disabled' : ''}
        >
          <div class="maneuver-top">
            <span class="maneuver-icon">${meta.icon}</span>
            <span class="maneuver-slot">${meta.label}</span>
            ${isChosen ? '<span class="maneuver-lock">&#10003;</span>' : ''}
          </div>
          <strong>${escapeHtml(row.name)}</strong>
          <div class="maneuver-fx">${maneuverFx(row)}</div>
        </button>
      `;
        })
        .join('')}
    </div>
  `;
}

function renderArena(): void {
  const duel = latestDuel();
  const combatants = rows(combatantsTable()).filter(
    row => !duel || row.roomId === duel.roomId
  );

  const host = $('combatants');
  if (combatants.length === 0) {
    host.innerHTML =
      '<div class="empty wide">Queue from two tabs to create the duel.</div>';
    arenaSignature = '';
    animatedRound = -1;
    prevVitals.clear();
    $('duelStatus').hidden = true;
    renderManeuvers(undefined, [], false);
    $('forfeitDuel').hidden = false;
    $('newDuel').hidden = true;
    $('home').hidden = true;
    return;
  }

  // Rebuild cards only when the combatant set changes, so HP bars can animate in place.
  const round = duel?.round ?? 0;
  const signature = `${duel?.roomId ?? ''}:${combatants.map(c => c.subject).join('|')}`;
  if (signature !== arenaSignature) {
    host.innerHTML = combatants.map(combatantCardHtml).join('');
    arenaSignature = signature;
    animatedRound = round;
    prevVitals.clear();
  }

  const cards = new Map<string, HTMLElement>();
  for (const el of host.querySelectorAll('.combatant')) {
    cards.set((el as HTMLElement).dataset.subject ?? '', el as HTMLElement);
  }
  const complete =
    !!duel &&
    (duel.status.tag === 'Complete' || duel.status.tag === 'Abandoned');

  // Animate a freshly resolved round before applying the new bar values.
  if (duel && round > animatedRound) {
    animateRound(host, duel, combatants, round);
    animatedRound = round;
  }

  for (const row of combatants) {
    const card = cards.get(row.subject);
    if (!card) continue;
    (card.querySelector('.hull-text') as HTMLElement).textContent =
      `${row.hull}/${row.maxHull}`;
    (card.querySelector('.shield-text') as HTMLElement).textContent =
      `${row.shields}/${row.maxShields}`;
    (card.querySelector('.hull-fill') as HTMLElement).style.width =
      `${pct(row.hull, row.maxHull)}%`;
    (card.querySelector('.shield-fill') as HTMLElement).style.width =
      `${pct(row.shields, row.maxShields)}%`;
    card.classList.toggle(
      'low',
      row.hull > 0 && pct(row.hull, row.maxHull) <= 30
    );
    card.classList.toggle('dead', row.hull <= 0);
    card.classList.toggle(
      'winner',
      complete && !!duel!.winnerSubject && row.subject === duel!.winnerSubject
    );
    card.classList.toggle(
      'loser',
      complete && !!duel!.winnerSubject && row.subject !== duel!.winnerSubject
    );
    const abilities = card.querySelector('.ship-abilities') as HTMLElement;
    abilities.innerHTML = abilitiesHtml(duel, row);
  }

  prevVitals.clear();
  for (const c of combatants)
    prevVitals.set(c.subject, { hull: c.hull, shields: c.shields });

  const statusEl = $('duelStatus');
  if (complete) {
    const abandoned = duel!.status.tag === 'Abandoned';
    const youWon = duel!.winnerSubject === me;
    statusEl.textContent = abandoned
      ? 'Duel Abandoned'
      : youWon
        ? 'Victory'
        : 'Defeat';
    statusEl.className = `${abandoned ? 'abandoned' : youWon ? 'win' : 'lose'} show`;
    statusEl.hidden = false;
  } else if (duel) {
    statusEl.textContent = round >= 1 ? `Round ${round}` : 'Ready';
    statusEl.className = '';
    statusEl.hidden = false;
  } else {
    statusEl.hidden = true;
  }

  renderManeuvers(duel, combatants, complete);
  $('forfeitDuel').hidden = complete;
  $('newDuel').hidden = !complete;
  $('home').hidden = !complete;
}

function render(): void {
  if (!conn) return;
  renderProfile();
  renderRanked();
  renderLobby();
  renderArena();
  const ticket = latestTicket();
  const duel = latestDuel();
  if (
    duel &&
    (duel.status.tag === 'Active' || duel.status.tag === 'Configuring')
  ) {
    playedRoomId = duel.roomId.toString();
  }
  if (ticket?.status.tag !== 'Queued' || duel || activeRoom())
    clearFallbackTimer();
  setScreen(desiredScreen());
}

function wireTables(): void {
  const rerender = () => render();
  const sources = [
    profileTable(),
    playersTable(),
    ticketsTable(),
    roomsTable(),
    seatsTable(),
    summaryTable(),
    ratingsTable(),
    leaderboardTable(),
    shipCatalogTable(),
    maneuverCatalogTable(),
    duelsTable(),
    combatantsTable(),
    logsTable(),
    maneuversTable(),
  ];
  for (const source of sources) {
    source.onInsert(rerender);
    source.onUpdate(rerender);
    source.onDelete(rerender);
  }
}

function wireActions(): void {
  const moveShip = async (direction: -1 | 1) => {
    const index = shipClasses.indexOf(selectedShip);
    const next =
      shipClasses[
        (index + direction + shipClasses.length) % shipClasses.length
      ];
    try {
      await chooseShip(next);
    } catch (err) {
      showToast(errorMessage(err), 'error');
    }
  };
  $('shipRoster').addEventListener('click', ev => {
    const btn =
      ev.target instanceof Element ? ev.target.closest('[data-ship]') : null;
    if (!(btn instanceof HTMLElement) || !btn.dataset.ship) return;
    const ship = btn.dataset.ship as ShipClass;
    if (ship !== selectedShip)
      void chooseShip(ship).catch(err => showToast(errorMessage(err), 'error'));
  });
  document.addEventListener('keydown', ev => {
    if (!$('setupScreen').classList.contains('active')) return;
    if (document.activeElement instanceof HTMLInputElement) return;
    if (ev.key === 'ArrowLeft') void moveShip(-1);
    else if (ev.key === 'ArrowRight') void moveShip(1);
  });
  const saveDisplayName = async () => {
    const value = input('displayName').value.trim();
    if (!value || value === currentProfile()?.displayName) return;
    try {
      requireConn().reducers.setDisplayName({ displayName: value });
    } catch (err) {
      showToast(errorMessage(err), 'error');
    }
  };
  input('displayName').addEventListener('change', () => void saveDisplayName());
  input('displayName').addEventListener('keydown', ev => {
    if (ev.key === 'Enter') {
      ev.preventDefault();
      input('displayName').blur();
    }
  });
  $('findDuel').addEventListener('click', async () => {
    try {
      screenOverride = null;
      autoJoinedRoom = null;
      requireConn().reducers.setDisplayName({
        displayName: input('displayName').value,
      });
      requireConn().reducers.selectShip({ shipClass: { tag: selectedShip } });
      requireConn().reducers.findDuel({});
      showToast('Looking for match.');
      scheduleAiFallback();
    } catch (err) {
      showToast(errorMessage(err), 'error');
    }
  });
  $('maneuverActions').addEventListener('click', async ev => {
    const btn =
      ev.target instanceof Element ? ev.target.closest('[data-slot]') : null;
    if (!(btn instanceof HTMLElement) || !btn.dataset.slot) return;
    const duel = latestDuel();
    if (!duel) return;
    try {
      requireConn().reducers.chooseManeuver({
        roomId: duel.roomId,
        slot: { tag: btn.dataset.slot as ManeuverSlot },
      });
    } catch (err) {
      showToast(errorMessage(err), 'error');
    }
  });
  $('forfeitDuel').addEventListener('click', () => {
    if (!activeRoom()) return;
    dialog('forfeitDialog').showModal();
  });
  $('cancelForfeit').addEventListener('click', closeForfeitDialog);
  $('confirmForfeit').addEventListener('click', () => void forfeitActiveDuel());
  dialog('forfeitDialog').addEventListener('click', ev => {
    if (ev.target === dialog('forfeitDialog')) closeForfeitDialog();
  });
  $('newDuel').addEventListener('click', async () => {
    const duel = latestDuel();
    try {
      screenOverride = null;
      autoJoinedRoom = null;
      requireConn().reducers.queueAgain({ roomId: duel?.roomId });
      showToast('Looking for match.');
      scheduleAiFallback();
    } catch (err) {
      showToast(errorMessage(err), 'error');
    }
  });
  $('home').addEventListener('click', () => {
    screenOverride = 'setup';
    render();
  });
  $('cancelSearch').addEventListener('click', async () => {
    clearFallbackTimer();
    screenOverride = 'setup';
    render();
    for (const ticket of rows(ticketsTable()).filter(
      t => t.status.tag === 'Queued'
    )) {
      try {
        requireConn().reducers['lobby.cancelTicket']({
          ticketId: ticket.ticketId,
        });
      } catch (err) {
        if (!isBenignDuelError(err)) showToast(errorMessage(err), 'error');
      }
    }
  });
}

async function run(): Promise<void> {
  const config = await loadConfig();
  const c = await connect(config);
  c.subscriptionBuilder()
    .onApplied(() => render())
    .onError((ctx: ErrorContext) =>
      console.error('subscription error', ctx.event)
    )
    .subscribe([
      'SELECT * FROM my_profile',
      'SELECT * FROM players',
      'SELECT * FROM my_lobby_tickets',
      'SELECT * FROM my_lobby_rooms',
      'SELECT * FROM my_lobby_room_seats',
      'SELECT * FROM lobby_queue_summary',
      'SELECT * FROM my_lobby_ratings',
      'SELECT * FROM lobby_ranked_leaderboard',
      'SELECT * FROM ship_catalog',
      'SELECT * FROM maneuver_catalog',
      'SELECT * FROM my_duels',
      'SELECT * FROM my_duel_combatants',
      'SELECT * FROM my_duel_round_logs',
      'SELECT * FROM my_duel_maneuvers',
    ]);
  wireTables();
  wireActions();
  setupTooltip();
  showToast('Connected.');
  render();
}

run().catch(err => {
  showToast(errorMessage(err), 'error');
});
