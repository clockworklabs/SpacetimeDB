import { existsSync, readdirSync, statSync, watch } from 'node:fs';
import type { FSWatcher } from 'node:fs';
import { join } from 'node:path';

import { ARTIFACT_FILE } from '../src/evidence/artifacts.js';
import { CAMPAIGN_FILE } from '../src/campaigns/campaign-path.js';

const DEBOUNCE_MS = 500;
const POLL_MS = 5000;
const LOG_FILE = 'process.stdout.log';
const CAMPAIGN_FILES = [CAMPAIGN_FILE.plan, CAMPAIGN_FILE.state] as const;
const EXECUTION_FILES = [ARTIFACT_FILE.run, ARTIFACT_FILE.progressionState] as const;

export interface CampaignChange {
  type: 'campaign' | 'log';
  key: string;
  attemptId?: string;
}

export type WatchMode = 'watch' | 'poll';

export interface CampaignWatcher {
  close(): void;
}

interface CampaignFingerprint {
  campaign: string;
  logs: Map<string, number>;
}

function stamp(path: string): string | null {
  if (!existsSync(path)) return null;
  const stat = statSync(path);
  return `${stat.size}:${stat.mtimeMs}`;
}

function directories(root: string): string[] {
  if (!existsSync(root)) return [];
  return readdirSync(root, { withFileTypes: true })
    .filter(entry => entry.isDirectory()).map(entry => entry.name);
}

// The evidence a view reads, in one pass: the campaign files and each
// execution's result, and the log sizes that tell a follower there are new
// bytes to fetch.
function fingerprintCampaign(directory: string): CampaignFingerprint {
  const parts = CAMPAIGN_FILES.map(file => `${file}:${stamp(join(directory, file)) ?? 'missing'}`);
  const logs = new Map<string, number>();
  const attemptsRoot = join(directory, 'attempts');
  for (const attempt of directories(attemptsRoot)) {
    const attemptDirectory = join(attemptsRoot, attempt);
    let bytes = 0;
    for (const execution of directories(attemptDirectory)) {
      const executionDirectory = join(attemptDirectory, execution);
      for (const file of EXECUTION_FILES) {
        const value = stamp(join(executionDirectory, file));
        if (value) parts.push(`${attempt}/${execution}/${file}:${value}`);
      }
      const log = join(executionDirectory, LOG_FILE);
      if (existsSync(log)) bytes += statSync(log).size;
    }
    logs.set(attempt, bytes);
  }
  return { campaign: parts.sort().join('|'), logs };
}

// One watcher for the whole server: a recursive watch per campaign directory
// where the platform supports it (Windows, macOS, and Linux on Node 20 and
// later), and a poll of the same fingerprints where it does not.
export function watchCampaigns(campaignsRoot: string,
  emit: (change: CampaignChange) => void,
  onMode: (mode: WatchMode) => void = () => {}): CampaignWatcher {
  const fingerprints = new Map<string, CampaignFingerprint>();
  const watchers = new Map<string, FSWatcher>();
  const timers = new Map<string, NodeJS.Timeout>();
  let rootWatcher: FSWatcher | null = null;
  let poll: NodeJS.Timeout | null = null;
  let closed = false;

  const check = (key: string): void => {
    timers.delete(key);
    const directory = join(campaignsRoot, key);
    if (!existsSync(directory)) {
      fingerprints.delete(key);
      watchers.get(key)?.close();
      watchers.delete(key);
      return;
    }
    const next = fingerprintCampaign(directory);
    const previous = fingerprints.get(key);
    fingerprints.set(key, next);
    if (!previous) return;
    if (previous.campaign !== next.campaign) emit({ type: 'campaign', key });
    for (const [attemptId, bytes] of next.logs) {
      if ((previous.logs.get(attemptId) ?? 0) !== bytes) emit({ type: 'log', key, attemptId });
    }
  };

  const schedule = (key: string): void => {
    if (closed || timers.has(key)) return;
    timers.set(key, setTimeout(() => check(key), DEBOUNCE_MS).unref());
  };

  const startPoll = (): void => {
    if (closed || poll) return;
    onMode('poll');
    poll = setInterval(() => {
      attach();
      for (const key of directories(campaignsRoot)) check(key);
    }, POLL_MS).unref();
  };

  const attach = (): void => {
    if (closed) return;
    if (!rootWatcher && existsSync(campaignsRoot)) {
      try {
        rootWatcher = watch(campaignsRoot, { persistent: false }, (_event, name) => {
          const key = String(name ?? '').split(/[/\\]/)[0];
          if (key) schedule(key);
          attach();
        });
        rootWatcher.once('error', () => { rootWatcher = null; startPoll(); });
      } catch { startPoll(); }
    }
    for (const key of directories(campaignsRoot)) {
      // A campaign that appeared since the last pass has everything to report.
      if (!fingerprints.has(key)) {
        fingerprints.set(key, { campaign: '', logs: new Map() });
        schedule(key);
      }
      if (watchers.has(key) || poll) continue;
      try {
        const watcher = watch(join(campaignsRoot, key), { persistent: false, recursive: true },
          () => schedule(key));
        watcher.once('error', () => { watchers.delete(key); startPoll(); });
        watchers.set(key, watcher);
      } catch {
        // No recursive watch on this platform: the poll reads the same files.
        startPoll();
        return;
      }
    }
  };

  for (const key of directories(campaignsRoot)) {
    fingerprints.set(key, fingerprintCampaign(join(campaignsRoot, key)));
  }
  attach();
  if (!rootWatcher) startPoll();
  else if (!poll) onMode('watch');
  return {
    close() {
      closed = true;
      for (const timer of timers.values()) clearTimeout(timer);
      timers.clear();
      if (poll) clearInterval(poll);
      poll = null;
      rootWatcher?.close();
      rootWatcher = null;
      for (const watcher of watchers.values()) watcher.close();
      watchers.clear();
    },
  };
}
