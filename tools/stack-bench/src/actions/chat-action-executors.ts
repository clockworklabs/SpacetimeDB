import { actionImplementation } from './action-contract.js';
import {
  actorFor,
  browserFor,
  pad,
} from './actor-action-runtime.js';
import type { ActorActionArguments, BrowserActorCapabilities } from './actor-action-runtime.js';
import { browserApplicationBoundary } from './browser-action-executors.js';

type ChatArguments<Input extends { readonly actor: string }> =
  ActorActionArguments<Input, BrowserActorCapabilities>;

interface AccountInput {
  readonly actor: string;
  readonly exact?: boolean;
  readonly expectFailure?: boolean;
  readonly name: string;
  readonly password?: string;
  readonly readyTestid?: string;
  readonly settleMs?: number;
}

interface RoomInput {
  readonly actor: string;
  readonly private?: boolean;
  readonly room: string;
}

interface MessageInput {
  readonly actor: string;
  readonly text: string;
}

interface ManyMessagesInput {
  readonly actor: string;
  readonly count: number;
  readonly delayMs?: number;
  readonly prefix: string;
}

async function signUp({ input, capabilities, signal }: ChatArguments<AccountInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  const user = input.exact ? input.name : browser.scopedUser(input.name);
  const password = input.password ?? `pw-${user}`;
  const username = actor.page.locator(browser.testId('signup-username')).first();
  if (!(await username.isVisible())) {
    const toggle = actor.loc('signin-toggle');
    await username.or(toggle).first().waitFor({ state: 'visible', timeout: browser.defaultWithin });
    if (!(await username.isVisible())) {
      await toggle.click({ timeout: browser.defaultWithin });
      await username.waitFor({ state: 'visible', timeout: browser.defaultWithin });
    }
  }
  await username.fill(user);
  await actor.page.locator(browser.testId('signup-password')).first().fill(password);
  await actor.page.locator(browser.testId('signup-submit')).first().click();
  if (input.expectFailure) {
    await browser.sleep(input.settleMs ?? 2000, signal);
    return { user, expectedFailure: true };
  }
  await actor.page.locator(browser.testId('current-user')).first()
    .waitFor({ state: 'visible', timeout: browser.defaultWithin * 2 });
  return { user, signedUp: true };
}

async function signIn({ input, capabilities, signal }: ChatArguments<AccountInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  const user = input.exact ? input.name : browser.scopedUser(input.name);
  const password = input.password ?? `pw-${user}`;
  const username = actor.page.locator(browser.testId('signin-username')).first();
  const toggle = actor.loc('signin-toggle');
  if (!(await username.isVisible())) {
    await username.or(toggle).first().waitFor({ state: 'visible', timeout: browser.defaultWithin });
    if (!(await username.isVisible())) {
      await toggle.click({ timeout: browser.defaultWithin });
      await username.waitFor({ state: 'visible', timeout: browser.defaultWithin });
    }
  }
  await username.fill(user);
  await actor.page.locator(browser.testId('signin-password')).first().fill(password);
  await actor.page.locator(browser.testId('signin-submit')).first().click();
  if (input.expectFailure) {
    await browser.sleep(input.settleMs ?? 2000, signal);
    return { user, expectedFailure: true };
  }
  await actor.page.locator(browser.testId('current-user')).first()
    .waitFor({ state: 'visible', timeout: browser.defaultWithin * 2 });
  return { user, signedIn: true };
}

async function createRoom({ input, capabilities }: ChatArguments<RoomInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  const room = browser.roomName(input.room);
  const nameInput = actor.page.locator(browser.testId('room-name-input')).first();
  if (!(await nameInput.isVisible())) {
    await actor.page.locator(browser.testId('room-create')).first().click();
  }
  await nameInput.fill(room);
  if (input.private) await actor.page.locator(browser.testId('room-private-toggle')).first().click();
  await actor.page.locator(browser.testId('room-name-submit')).first().click();
  await actor.page.locator(browser.testId('room-item'), { hasText: room }).first()
    .waitFor({ state: 'visible', timeout: browser.defaultWithin });
  return { room, private: input.private === true };
}

async function enterRoom({ input, capabilities, signal }: ChatArguments<RoomInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  const room = browser.roomName(input.room);
  const item = actor.page.locator(browser.testId('room-item'), { hasText: room }).first();
  await item.waitFor({ state: 'visible', timeout: browser.defaultWithin });
  await item.click();
  const message = actor.loc('message-input');
  if (!(await message.isVisible())) {
    await browser.sleep(750, signal);
    if (!(await message.isVisible())) await item.click();
  }
  await message.waitFor({ state: 'visible', timeout: browser.defaultWithin });
  return { room, entered: true };
}

async function send({ input, capabilities }: ChatArguments<MessageInput>) {
  const message = actorFor(capabilities, input.actor).loc('message-input');
  await message.fill(input.text);
  await message.press('Enter');
  return { sent: input.text };
}

async function sendMany({ input, capabilities, signal }: ChatArguments<ManyMessagesInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  const message = actor.loc('message-input');
  for (let index = 1; index <= input.count; index++) {
    await message.fill(`${input.prefix}-${pad(index, input.count)}`);
    await message.press('Enter');
    if (input.delayMs) await browser.sleep(input.delayMs, signal);
  }
  return { sent: input.count, prefix: input.prefix };
}

async function ensureSignedIn({ input, capabilities, signal }: ChatArguments<AccountInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  const user = input.exact ? input.name : browser.scopedUser(input.name);
  const currentUser = actor.page.locator(browser.testId('current-user')).first();
  if (await currentUser.isVisible()) {
    const signedInAs = await currentUser.innerText();
    if (!signedInAs.includes(user)) {
      throw new Error(`${actor.name} is already signed in as a different account`);
    }
    return { restored: false, user };
  }
  await signIn({ input, capabilities, signal });
  await actor.page.locator(browser.testId(input.readyTestid ?? 'room-list')).first()
    .waitFor({ state: 'attached', timeout: browser.defaultWithin });
  await browser.sleep(input.settleMs ?? 1500, signal);
  return { restored: true, user };
}

export const CHAT_ACTION_IMPLEMENTATIONS = Object.freeze({
  createRoom: actionImplementation(browserApplicationBoundary(createRoom)),
  ensureSignedIn: actionImplementation(browserApplicationBoundary(ensureSignedIn)),
  enterRoom: actionImplementation(browserApplicationBoundary(enterRoom)),
  send: actionImplementation(browserApplicationBoundary(send)),
  sendMany: actionImplementation(browserApplicationBoundary(sendMany)),
  signIn: actionImplementation(browserApplicationBoundary(signIn)),
  signUp: actionImplementation(browserApplicationBoundary(signUp)),
});
