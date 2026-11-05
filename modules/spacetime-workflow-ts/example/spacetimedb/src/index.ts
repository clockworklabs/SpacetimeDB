import {
  schema,
  SenderError,
  t,
  table,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
} from 'spacetimedb/server';
import { callChat, openRouterProvider } from '@spacetimedb/agents';
import { client } from '@spacetimedb/workflow';

const MODEL = 'openai/gpt-4o-mini';
const MAX_QUESTION_LENGTH = 300;
const MAX_TOPICS = 4;
const MAX_TOPIC_LENGTH = 80;
const SOURCES_PER_TOPIC = 3;

const workflows = client({
  workflows: {
    research: {
      args: t.object('ResearchArgs', {
        dossierId: t.u64(),
        question: t.string(),
      }),
      retry: { maxAttempts: 3, backoffSeconds: 5 },
      // Longer than the slowest model call.
      stepTimeoutSeconds: 30,
    },
  },
});

const dossier = table(
  { name: 'dossier', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    owner: t.identity().index(),
    runId: t.u64(),
    question: t.string(),
    topics: t.array(t.string()),
    planTokens: t.u32(),
    report: t.option(t.string()),
    reportTokens: t.u32(),
    createdAt: t.timestamp(),
  }
);

const source = t.object('Source', { title: t.string(), url: t.string() });

const topicNote = table(
  { name: 'topic_note', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    key: t.string().unique(),
    dossierId: t.u64().index(),
    owner: t.identity().index(),
    topic: t.string(),
    summary: t.option(t.string()),
    sources: t.array(source),
    tokens: t.u32(),
    savedAt: t.timestamp(),
  }
);

const spacetimedb = schema(
  { ...workflows.tables, dossier, topicNote },
  { env: { OPENROUTER_API_KEY: t.string().optional() } }
);
export default spacetimedb;

type Schema = InferSchema<typeof spacetimedb>;
type Ctx = ProcedureCtx<Schema>;

interface Source {
  title: string;
  url: string;
  extract: string;
}

interface ModelReply {
  text: string;
  tokens: number;
}

function callModel(ctx: Ctx, system: string, prompt: string): ModelReply {
  const key = ctx.env.OPENROUTER_API_KEY;
  if (!key) {
    throw new SenderError('Set OPENROUTER_API_KEY in .env and restart the app');
  }
  const result = callChat(ctx.http, openRouterProvider, {
    apiKey: key,
    model: MODEL,
    system,
    messages: [{ role: 'user', content: prompt }],
    maxTokens: 800,
  });
  if (!result.ok) throw new Error(`Model request failed: ${result.error.kind}`);
  return {
    text: result.response.text ?? '',
    tokens: result.response.usage.totalTokens,
  };
}

function planTopics(ctx: Ctx, question: string) {
  const reply = callModel(
    ctx,
    `Plan research for a question. Choose 2 to ${MAX_TOPICS} distinct angles that do not overlap. Reply with only a JSON array of short Wikipedia search phrases, one per angle.`,
    question
  );
  const topics = JSON.parse(reply.text.replace(/```(json)?/g, '')) as string[];
  return {
    topics: topics.map(String).slice(0, MAX_TOPICS),
    tokens: reply.tokens,
  };
}

function searchWikipedia(ctx: Ctx, topic: string): Source[] {
  const url =
    'https://en.wikipedia.org/w/api.php?action=query&format=json&formatversion=2' +
    `&generator=search&gsrlimit=${SOURCES_PER_TOPIC}&gsrsearch=${encodeURIComponent(topic)}` +
    `&prop=extracts%7Cinfo&exintro=1&explaintext=1&exlimit=${SOURCES_PER_TOPIC}` +
    '&exchars=1500&inprop=url';
  const response = ctx.http.fetch(url, {
    headers: { 'User-Agent': 'spacetimedb-dossier-example' },
  });
  if (response.status !== 200) {
    throw new Error(`Wikipedia returned ${response.status}`);
  }
  const { query } = JSON.parse(response.text()) as {
    query?: {
      pages: { title: string; fullurl: string; extract?: string }[];
    };
  };
  return (query?.pages ?? []).map(page => ({
    title: page.title,
    url: page.fullurl,
    extract: page.extract ?? '',
  }));
}

function summarize(
  ctx: Ctx,
  question: string,
  topic: string,
  otherTopics: string[],
  sources: Source[]
): ModelReply {
  return callModel(
    ctx,
    `Summarize what these sources say about this topic that helps answer the question, in under 100 words. Use only the sources. Leave out background, which other notes cover: ${otherTopics.join(', ')}.`,
    `Question: ${question}\nTopic: ${topic}\n\n${sources
      .map(item => `${item.title}: ${item.extract}`)
      .join('\n\n')}`
  );
}

function writeReport(
  ctx: Ctx,
  question: string,
  notes: { topic: string; summary: string }[]
): ModelReply {
  return callModel(
    ctx,
    'Answer the question in a short plain-text report of up to four paragraphs, using only these notes. Say what the notes could not answer. Do not use markdown.',
    `Question: ${question}\n\n${notes
      .map(note => `${note.topic}: ${note.summary}`)
      .join('\n\n')}`
  );
}

export const workflowRunAttempt = workflows.register(spacetimedb, {
  research(step, { dossierId, question }) {
    const plan = step.call('Plan the research', ctx =>
      planTopics(ctx, question)
    );
    step.tx('Save the plan', ctx => {
      const row = ctx.db.dossier.id.find(dossierId)!;
      ctx.db.dossier.id.update({
        ...row,
        topics: plan.topics,
        planTokens: plan.tokens,
      });
    });

    step.waitFor('plan-approved');
    // The plan may have been edited before it was approved.
    const topics = step.tx(
      'Read the approved plan',
      ctx => ctx.db.dossier.id.find(dossierId)!.topics
    );

    // Every topic is researched at the same time, each in its own procedure.
    const research = step.parallel('Research', topics, (ctx, topic, run) => {
      const sources = searchWikipedia(ctx, topic);
      const note =
        sources.length === 0
          ? undefined
          : summarize(
              ctx,
              question,
              topic,
              topics.filter(other => other !== topic),
              sources
            );
      // Saved now, so the page shows it while other topics are still running.
      // The key makes a retry replace its own note instead of adding one.
      ctx.withTx(tx => {
        const { owner } = tx.db.dossier.id.find(dossierId)!;
        const row = {
          id: 0n,
          key: run.idempotencyKey,
          dossierId,
          owner,
          topic,
          summary: note?.text,
          sources: sources.map(({ title, url }) => ({ title, url })),
          tokens: note?.tokens ?? 0,
          savedAt: tx.timestamp,
        };
        const saved = tx.db.topicNote.key.find(run.idempotencyKey);
        if (saved) tx.db.topicNote.id.update({ ...row, id: saved.id });
        else tx.db.topicNote.insert(row);
      });
      return note?.text;
    });
    // A topic that keeps failing is left out. The rest of the report goes on.
    const notes = topics.flatMap((topic, i) => {
      const result = research[i];
      return result.ok && result.value
        ? [{ topic, summary: result.value }]
        : [];
    });

    if (notes.length === 0) {
      throw new Error('Could not find any sources for this question');
    }

    const report = step.call('Write the report', ctx =>
      writeReport(ctx, question, notes)
    );
    step.tx('Publish', ctx => {
      const row = ctx.db.dossier.id.find(dossierId)!;
      ctx.db.dossier.id.update({
        ...row,
        report: report.text,
        reportTokens: report.tokens,
      });
    });
  },
});

export const init = spacetimedb.init(ctx => {
  workflows.install(ctx);
});

export const startDossier = spacetimedb.reducer(
  { question: t.string() },
  (ctx, { question }) => {
    const trimmed = question.trim();
    if (trimmed.length === 0 || trimmed.length > MAX_QUESTION_LENGTH) {
      throw new SenderError(
        `Ask a question of up to ${MAX_QUESTION_LENGTH} characters`
      );
    }
    const row = ctx.db.dossier.insert({
      id: 0n,
      owner: ctx.sender,
      runId: 0n,
      question: trimmed,
      topics: [],
      planTokens: 0,
      report: undefined,
      reportTokens: 0,
      createdAt: ctx.timestamp,
    });
    const runId = workflows.start(ctx, 'research', {
      dossierId: row.id,
      question: trimmed,
    });
    ctx.db.dossier.id.update({ ...row, runId });
  }
);

function requireOwnedDossier(ctx: ReducerCtx<Schema>, id: bigint) {
  const row = ctx.db.dossier.id.find(id);
  if (!row || !ctx.sender.isEqual(row.owner)) {
    throw new SenderError('Dossier not found');
  }
  return row;
}

export const approvePlan = spacetimedb.reducer(
  { dossierId: t.u64(), topics: t.array(t.string()) },
  (ctx, { dossierId, topics }) => {
    const row = requireOwnedDossier(ctx, dossierId);
    if (ctx.db.workflowRun.id.find(row.runId)?.waitingFor !== 'plan-approved') {
      throw new SenderError('This plan is no longer waiting for approval');
    }
    const plan = topics.map(topic => topic.trim()).filter(Boolean);
    if (
      plan.length === 0 ||
      plan.length > MAX_TOPICS ||
      plan.some(topic => topic.length > MAX_TOPIC_LENGTH)
    ) {
      throw new SenderError(
        `A plan needs 1 to ${MAX_TOPICS} topics of up to ${MAX_TOPIC_LENGTH} characters`
      );
    }
    ctx.db.dossier.id.update({ ...row, topics: plan });
    workflows.signal(ctx, row.runId, 'plan-approved');
  }
);

export const cancelDossier = spacetimedb.reducer(
  { dossierId: t.u64() },
  (ctx, { dossierId }) => {
    const row = requireOwnedDossier(ctx, dossierId);
    workflows.cancel(ctx, row.runId);
  }
);

export const myDossiers = spacetimedb.view(
  { name: 'my_dossiers', public: true },
  t.array(dossier.rowType),
  ctx => [...ctx.db.dossier.owner.filter(ctx.sender)]
);

export const myNotes = spacetimedb.view(
  { name: 'my_notes', public: true },
  t.array(topicNote.rowType),
  ctx => [...ctx.db.topicNote.owner.filter(ctx.sender)]
);

export const myRuns = spacetimedb.view(
  { name: 'my_runs', public: true },
  t.array(workflows.tables.workflowRun.rowType),
  ctx => [...ctx.db.workflowRun.startedBy.filter(ctx.sender)]
);

export const mySteps = spacetimedb.view(
  { name: 'my_steps', public: true },
  t.array(workflows.tables.workflowHistory.rowType),
  ctx => {
    const runs = new Set(
      [...ctx.db.workflowRun.startedBy.filter(ctx.sender)].map(run => run.id)
    );
    return [...ctx.db.workflowHistory.iter()].filter(row =>
      runs.has(row.runId)
    );
  }
);
