import { DbConnection, tables, type ErrorContext } from './module_bindings/app';
import type { CronSchedule } from './module_bindings/app/types';

interface ServerConfig {
  stdbUri: string;
  appDatabase: string;
}

type ConnectionState = 'connecting' | 'connected' | 'disconnected' | 'error';
type StatusTone = 'neutral' | 'success' | 'error';

let connection: DbConnection | null = null;

function element<T extends keyof HTMLElementTagNameMap>(
  tag: T,
  className?: string,
  text?: string
): HTMLElementTagNameMap[T] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function byId<T extends HTMLElement>(id: string): T {
  const node = document.getElementById(id);
  if (!node) throw new Error(`Missing required element #${id}`);
  return node as T;
}

function setText(id: string, value: string | number | bigint): void {
  byId(id).textContent = String(value);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function formatTime(micros: bigint): string {
  return new Date(Number(micros / 1_000n)).toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'medium',
  });
}

function formatJobName(name: string): string {
  return name
    .split('_')
    .map(part => `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
    .join(' ');
}

function scheduleLabel(schedule: CronSchedule): string {
  if (schedule.tag === 'Cron') {
    return `${schedule.value.expression} · ${schedule.value.timezone}`;
  }
  return `Every ${schedule.value.seconds} seconds`;
}

function nextRunLabel(nextRunAt?: { microsSinceUnixEpoch: bigint }): string {
  return nextRunAt ? formatTime(nextRunAt.microsSinceUnixEpoch) : 'Not armed';
}

function setConnection(state: ConnectionState, label: string): void {
  byId('connection').dataset.state = state;
  setText('conn', label);
  const enabled = state === 'connected';
  byId<HTMLButtonElement>('open-scheduler').disabled = !enabled;
  byId<HTMLButtonElement>('btn-schedule').disabled = !enabled;
}

function setFormStatus(text: string, tone: StatusTone = 'neutral'): void {
  const status = byId<HTMLElement>('form-status');
  status.textContent = text;
  status.dataset.tone = tone;
}

function setCollectionVisibility(
  listId: string,
  emptyId: string,
  hasItems: boolean
): void {
  byId(listId).hidden = !hasItems;
  byId(emptyId).hidden = hasItems;
}

function metric(label: string, value: string, unhealthy = false): HTMLElement {
  const node = element('div', 'metric');
  const valueNode = element(
    'strong',
    unhealthy ? 'unhealthy' : undefined,
    value
  );
  node.append(element('span', undefined, label), valueNode);
  return node;
}

function renderJobs(): void {
  if (!connection) return;
  const jobs = [...connection.db.cronJobs.iter()].sort((left, right) =>
    left.name.localeCompare(right.name)
  );
  const list = byId<HTMLUListElement>('jobs');
  list.replaceChildren();

  let healthy = 0;
  let dispatches = 0n;

  for (const job of jobs) {
    if (job.enabled && job.consecutiveFailures === 0) healthy += 1;
    dispatches += job.fireCount;

    const item = element('li', 'job-card');
    item.dataset.enabled = String(job.enabled);

    const heading = element('div', 'job-card-top');
    const identity = element('div', 'job-identity');
    const name = element('div');
    name.append(
      element('strong', undefined, formatJobName(job.name)),
      element('code', undefined, job.name)
    );
    identity.append(
      element('span', 'job-icon', job.name.charAt(0).toUpperCase()),
      name
    );

    const statusText = job.enabled ? 'enabled' : 'disabled';
    const status = element('span', `status-badge ${statusText}`, statusText);
    if (job.disabledReason) status.title = job.disabledReason;
    heading.append(identity, status);

    const schedule = element('div', 'job-schedule');
    schedule.append(element('code', undefined, scheduleLabel(job.schedule)));

    const footer = element('div', 'job-footer');
    const metrics = element('div', 'job-metrics');
    metrics.append(
      metric('Next run', nextRunLabel(job.nextRunAt)),
      metric('Dispatches', String(job.fireCount)),
      metric(
        'Failures',
        String(job.consecutiveFailures),
        job.consecutiveFailures > 0
      )
    );
    footer.append(metrics);

    if (job.enabled) {
      const button = element(
        'button',
        'button button-secondary button-small',
        'Unschedule'
      );
      button.type = 'button';
      button.onclick = async () => {
        const current = connection;
        if (!current) return;
        button.disabled = true;
        setFormStatus(`Unscheduling ${formatJobName(job.name)}...`);
        try {
          await current.reducers.unscheduleJob({ name: job.name });
          setFormStatus(`${formatJobName(job.name)} unscheduled.`, 'success');
        } catch (error) {
          setFormStatus(errorMessage(error), 'error');
          button.disabled = false;
        }
      };
      footer.append(button);
    }

    item.append(heading, schedule, footer);
    list.append(item);
  }

  setText('job-count', jobs.length);
  setText('stat-jobs', jobs.length);
  setText('stat-healthy', healthy);
  setText('stat-fires', dispatches);
  setCollectionVisibility('jobs', 'jobs-empty', jobs.length > 0);
}

function renderRuns(): void {
  if (!connection) return;
  const runs = [...connection.db.cronRun.iter()]
    .sort((left, right) => {
      const leftTime = left.scheduledFor.microsSinceUnixEpoch;
      const rightTime = right.scheduledFor.microsSinceUnixEpoch;
      if (leftTime !== rightTime) return leftTime > rightTime ? -1 : 1;
      const byName = left.jobName.localeCompare(right.jobName);
      if (byName !== 0) return byName;
      return left.sequence > right.sequence
        ? -1
        : left.sequence < right.sequence
          ? 1
          : 0;
    })
    .slice(0, 20);
  const list = byId<HTMLUListElement>('runs');
  list.replaceChildren();

  for (const run of runs) {
    const failed = run.status.tag === 'Failed';
    const item = element('li', 'event-item');
    const marker = element('span', `event-marker ${failed ? 'failed' : 'ok'}`);
    marker.setAttribute('aria-hidden', 'true');
    const content = element('div', 'event-content');
    content.append(
      element(
        'span',
        'event-title',
        `${formatJobName(run.jobName)} · ${run.status.tag}`
      )
    );
    if (run.error) {
      content.append(element('span', 'event-copy', run.error));
    }
    content.append(
      element(
        'time',
        'event-time',
        formatTime(run.scheduledFor.microsSinceUnixEpoch)
      )
    );
    item.append(marker, content);
    list.append(item);
  }

  setText('run-count', runs.length);
  setCollectionVisibility('runs', 'runs-empty', runs.length > 0);
}

function renderActivity(): void {
  if (!connection) return;
  const entries = [...connection.db.activityLog.iter()]
    .sort((left, right) => {
      const leftTime = left.at.microsSinceUnixEpoch;
      const rightTime = right.at.microsSinceUnixEpoch;
      if (leftTime !== rightTime) return leftTime > rightTime ? -1 : 1;
      return left.id > right.id ? -1 : left.id < right.id ? 1 : 0;
    })
    .slice(0, 20);
  const list = byId<HTMLUListElement>('activity');
  list.replaceChildren();

  for (const entry of entries) {
    const item = element('li', 'event-item');
    const marker = element('span', 'event-marker');
    marker.setAttribute('aria-hidden', 'true');
    const content = element('div', 'event-content');
    content.append(
      element('span', 'event-title', formatJobName(entry.jobName)),
      element('span', 'event-copy', entry.message),
      element('time', 'event-time', formatTime(entry.at.microsSinceUnixEpoch))
    );
    item.append(marker, content);
    list.append(item);
  }

  setText('activity-count', entries.length);
  setCollectionVisibility('activity', 'activity-empty', entries.length > 0);
}

function renderAll(): void {
  renderJobs();
  renderRuns();
  renderActivity();
}

function updateScheduleFields(): void {
  const kind = byId<HTMLSelectElement>('spec-kind');
  const expression = byId<HTMLInputElement>('spec-expr');
  const isCron = kind.value === 'cron';
  setText('spec-expr-label', isCron ? 'Expression' : 'Seconds');
  setText(
    'spec-help',
    isCron
      ? 'Use five fields, or six fields to include seconds.'
      : 'Enter a whole number of seconds.'
  );
  expression.placeholder = isCron ? '*/10 * * * * *' : '10';
  byId('spec-tz-wrap').hidden = !isCron;
}

function applyPreset(preset: string): void {
  const kind = byId<HTMLSelectElement>('spec-kind');
  const expression = byId<HTMLInputElement>('spec-expr');
  const timezone = byId<HTMLInputElement>('spec-tz');

  if (preset === 'weekday') {
    kind.value = 'cron';
    expression.value = '0 9 * * 1-5';
    timezone.value = 'America/New_York';
  } else if (preset === 'five-minutes') {
    kind.value = 'every';
    expression.value = '300';
  } else {
    kind.value = 'cron';
    expression.value = '*/10 * * * * *';
    timezone.value = 'UTC';
  }

  updateScheduleFields();
  setFormStatus('Preset applied.');
  expression.focus();
}

function wireForm(): void {
  const form = byId<HTMLFormElement>('schedule-form');
  const kind = byId<HTMLSelectElement>('spec-kind');
  const expression = byId<HTMLInputElement>('spec-expr');
  const timezone = byId<HTMLInputElement>('spec-tz');
  const jobName = byId<HTMLSelectElement>('job-name');
  const cleanupKeep = byId<HTMLInputElement>('cleanup-keep');
  const submit = byId<HTMLButtonElement>('btn-schedule');

  form.onsubmit = async event => {
    event.preventDefault();
    const current = connection;
    if (!current) {
      setFormStatus('Connect to SpacetimeDB before scheduling a job.', 'error');
      return;
    }

    const name = jobName.value;
    const keep = Number(cleanupKeep.value);
    submit.disabled = true;
    setFormStatus('');
    try {
      if (
        name === 'cleanup' &&
        (!Number.isInteger(keep) || keep < 1 || keep > 1_000)
      ) {
        throw new Error('Rows to keep must be an integer from 1 through 1000.');
      }
      if (kind.value === 'cron') {
        const value = expression.value.trim();
        if (!value) throw new Error('Enter a cron expression.');
        setFormStatus(`Scheduling ${formatJobName(name)}...`);
        await current.reducers.scheduleCron({
          name,
          expression: value,
          timezone: timezone.value.trim() || 'UTC',
          keep,
        });
      } else {
        const seconds = Number(expression.value);
        if (!Number.isInteger(seconds) || seconds < 1) {
          throw new Error('Interval seconds must be a positive integer.');
        }
        setFormStatus(`Scheduling ${formatJobName(name)}...`);
        await current.reducers.scheduleEvery({ name, seconds, keep });
      }
      setFormStatus(`${formatJobName(name)} scheduled.`, 'success');
    } catch (error) {
      setFormStatus(errorMessage(error), 'error');
    } finally {
      submit.disabled = connection === null;
    }
  };

  kind.onchange = () => {
    const isCron = kind.value === 'cron';
    expression.value = isCron ? '*/10 * * * * *' : '10';
    updateScheduleFields();
    setFormStatus('');
  };

  jobName.onchange = () => {
    byId('cleanup-keep-wrap').hidden = jobName.value !== 'cleanup';
    setFormStatus('');
  };

  for (const preset of document.querySelectorAll<HTMLButtonElement>(
    '[data-preset]'
  )) {
    preset.onclick = () => applyPreset(preset.dataset.preset ?? '');
  }

  byId<HTMLButtonElement>('open-scheduler').onclick = () => {
    byId('scheduler').scrollIntoView({ behavior: 'smooth', block: 'center' });
    window.setTimeout(() => jobName.focus(), 250);
  };

  updateScheduleFields();
}

function watchTables(current: DbConnection): void {
  current.db.cronJobs.onInsert(renderJobs);
  current.db.cronJobs.onDelete(renderJobs);
  current.db.cronJobs.onUpdate(renderJobs);
  current.db.cronRun.onInsert(renderRuns);
  current.db.cronRun.onDelete(renderRuns);
  current.db.cronRun.onUpdate(renderRuns);
  current.db.activityLog.onInsert(renderActivity);
  current.db.activityLog.onDelete(renderActivity);
  current.db.activityLog.onUpdate(renderActivity);
}

async function main(): Promise<void> {
  wireForm();
  setConnection('connecting', 'Connecting');

  const response = await fetch('/api/config');
  if (!response.ok) {
    throw new Error(`Config request failed: ${response.status}`);
  }
  const config = (await response.json()) as ServerConfig;

  DbConnection.builder()
    .withUri(config.stdbUri)
    .withDatabaseName(config.appDatabase)
    .onConnect((current: DbConnection) => {
      connection = current;
      setConnection('connected', config.appDatabase);
      watchTables(current);
      current
        .subscriptionBuilder()
        .onApplied(renderAll)
        .subscribe([tables.cronJobs, tables.cronRun, tables.activityLog]);
    })
    .onConnectError((_ctx: ErrorContext, error: Error) => {
      setConnection('error', 'Connection failed');
      setFormStatus(error.message, 'error');
    })
    .onDisconnect((_ctx, error) => {
      connection = null;
      setConnection('disconnected', 'Disconnected');
      if (error) setFormStatus(error.message, 'error');
    })
    .build();
}

void main().catch(error => {
  setConnection('error', 'Configuration failed');
  setFormStatus(errorMessage(error), 'error');
});
