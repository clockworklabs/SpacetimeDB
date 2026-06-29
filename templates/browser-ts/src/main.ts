import { DbConnection, tables, type ErrorContext } from './module_bindings';
import { type Identity } from 'spacetimedb';

const HOST = import.meta.env.VITE_SPACETIMEDB_HOST ?? 'ws://localhost:3000';
const DB_NAME = import.meta.env.VITE_SPACETIMEDB_DB_NAME ?? 'browser-ts';

const statusEl = document.getElementById('status')!;
const nameInput = document.getElementById('name-input') as HTMLInputElement;
const addBtn = document.getElementById('add-btn') as HTMLButtonElement;
const addForm = document.getElementById('add-form')!;
const peopleList = document.getElementById('people-list')!;
const countEl = document.getElementById('count')!;

function renderPeople(conn: DbConnection) {
  const people = Array.from(conn.db.person.iter());
  countEl.textContent = String(people.length);
  if (people.length === 0) {
    peopleList.innerHTML =
      '<li style="color: #888;">No people yet. Add someone above!</li>';
    return;
  }
  peopleList.innerHTML = people
    .map((p) => '<li>' + escapeHtml(p.name || '') + '</li>')
    .join('');
}

function escapeHtml(text: string) {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

const conn = DbConnection.builder()
  .withUri(HOST)
  .withDatabaseName(DB_NAME)
  .withToken(localStorage.getItem('auth_token') || undefined)
  .onConnect((conn: DbConnection, identity: Identity, token: string) => {
    localStorage.setItem('auth_token', token);
    console.log('Connected with identity:', identity.toHexString());

    statusEl.textContent = 'Connected';
    statusEl.style.color = 'green';
    nameInput.disabled = false;
    addBtn.disabled = false;

    conn
      .subscriptionBuilder()
      .onApplied(() => renderPeople(conn))
      .subscribe(tables.person);

    conn.db.person.onInsert(() => renderPeople(conn));
    conn.db.person.onDelete(() => renderPeople(conn));
  })
  .onDisconnect(() => {
    console.log('Disconnected from SpacetimeDB');
    statusEl.textContent = 'Disconnected';
    statusEl.style.color = 'red';
    nameInput.disabled = true;
    addBtn.disabled = true;
  })
  .onConnectError((_ctx: ErrorContext, error: Error) => {
    console.error('Connection error:', error);
    statusEl.textContent = 'Error: ' + error.message;
    statusEl.style.color = 'red';
  })
  .build();

addForm.addEventListener('submit', (e) => {
  e.preventDefault();
  const name = nameInput.value.trim();
  if (name) {
    conn.reducers.add({ name });
    nameInput.value = '';
  }
});
