import { useState, useMemo } from 'react'
import { useParams, Link } from 'react-router-dom'
import { useData } from '../hooks/useData'
import PassChip from '../components/PassChip'
import type { TaskResult } from '../types'

const ACCENT = '#4cf490'
const OK = '#4cf490'
const BAD = '#ff4c4c'
const CARD_BG = '#141416'
const BORDER = '#202126'

function pctColor(pct: number) {
  if (pct >= 80) return OK
  if (pct >= 50) return '#fbdc8e'
  return BAD
}

function duration(started: string, finished: string): string {
  try {
    const ms = new Date(finished).getTime() - new Date(started).getTime()
    if (ms < 1000) return `${ms}ms`
    return `${(ms / 1000).toFixed(1)}s`
  } catch {
    return '—'
  }
}

function TaskRow({ task }: { task: TaskResult }) {
  const [expanded, setExpanded] = useState(false)
  const passed = task.passed_tests >= task.total_tests && task.total_tests > 0
  const allScorerPass = Object.values(task.scorer_details).every((s) => s.pass)

  return (
    <div
      className="border rounded-lg overflow-hidden"
      style={{ borderColor: BORDER, backgroundColor: '#0d0d0e' }}
    >
      {/* Header row */}
      <button
        className="w-full flex items-center gap-3 px-4 py-3 text-left hover:bg-[#1a1a1f] transition-colors"
        onClick={() => setExpanded((v) => !v)}
      >
        <PassChip passed={passed} size="sm" />
        <span className="flex-1 font-mono text-sm text-slate-300">{task.task}</span>
        <span className="text-xs text-slate-500 capitalize">{task.category}</span>
        <span className="text-xs text-slate-500 tabular-nums">
          {task.passed_tests}/{task.total_tests} tests
        </span>
        <span className="text-xs text-slate-500">
          {duration(task.started_at, task.finished_at)}
        </span>
        <svg
          className="w-4 h-4 text-slate-500 shrink-0 transition-transform"
          style={{ transform: expanded ? 'rotate(180deg)' : 'rotate(0deg)' }}
          fill="none" viewBox="0 0 24 24" stroke="currentColor"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {/* Expanded content */}
      {expanded && (
        <div className="border-t px-4 py-4 space-y-4" style={{ borderColor: BORDER }}>
          {/* Scorer details */}
          {Object.keys(task.scorer_details).length > 0 && (
            <div>
              <p className="text-xs font-semibold text-slate-400 mb-2 uppercase tracking-wide">Scorer Details</p>
              <div className="flex flex-wrap gap-2">
                {Object.entries(task.scorer_details).map(([key, detail]) => (
                  <div
                    key={key}
                    className="flex items-center gap-2 px-3 py-1.5 rounded"
                    style={{ backgroundColor: '#141416', border: `1px solid ${BORDER}` }}
                  >
                    <PassChip passed={detail.pass} size="sm" />
                    <span className="text-xs text-slate-300 font-mono">{key}</span>
                    {detail.partial != null && (
                      <span className="text-xs text-slate-500">{(detail.partial * 100).toFixed(0)}%</span>
                    )}
                  </div>
                ))}
              </div>
              {!allScorerPass && (
                <div className="mt-2 flex flex-wrap gap-1">
                  {Object.entries(task.scorer_details)
                    .filter(([, d]) => Object.keys(d.notes).length > 0)
                    .map(([key, d]) => (
                      <div key={key} className="text-xs text-slate-500 font-mono">
                        {key}: {JSON.stringify(d.notes)}
                      </div>
                    ))}
                </div>
              )}
            </div>
          )}

          {/* LLM Output */}
          <div>
            <p className="text-xs font-semibold text-slate-400 mb-2 uppercase tracking-wide">LLM Output</p>
            <pre
              className="text-xs text-slate-300 p-3 rounded overflow-x-auto whitespace-pre-wrap break-words font-mono leading-relaxed"
              style={{ backgroundColor: '#0a0a0b', border: `1px solid ${BORDER}`, maxHeight: 400 }}
            >
              {task.llm_output || <span className="text-slate-600 italic">No output</span>}
            </pre>
          </div>
        </div>
      )}
    </div>
  )
}

export default function ModelDetail() {
  const { name } = useParams<{ name: string }>()
  const decodedName = decodeURIComponent(name ?? '')
  const { details, summary, loading, error } = useData()

  const [filterCategory, setFilterCategory] = useState<string>('all')
  const [filterStatus, setFilterStatus] = useState<'all' | 'pass' | 'fail'>('all')

  // Find model data across all languages/modes
  const modelData = useMemo(() => {
    if (!details) return null
    for (const lang of details.languages) {
      for (const modeObj of lang.modes) {
        const model = modeObj.models.find((m) => m.name === decodedName)
        if (model) return { lang: lang.lang, mode: modeObj.mode, model }
      }
    }
    return null
  }, [details, decodedName])

  const summaryData = useMemo(() => {
    if (!summary || !modelData) return null
    return summary.by_language[modelData.lang]?.modes[modelData.mode]?.models[decodedName] ?? null
  }, [summary, modelData, decodedName])

  const tasks = useMemo((): TaskResult[] => {
    if (!modelData) return []
    return Object.values(modelData.model.tasks)
  }, [modelData])

  const categories = useMemo(() => {
    const cats = new Set<string>()
    tasks.forEach((t) => cats.add(t.category))
    return Array.from(cats).sort()
  }, [tasks])

  const filteredTasks = useMemo(() => {
    return tasks.filter((t) => {
      if (filterCategory !== 'all' && t.category !== filterCategory) return false
      if (filterStatus === 'pass' && t.passed_tests < t.total_tests) return false
      if (filterStatus === 'fail' && t.passed_tests >= t.total_tests) return false
      return true
    })
  }, [tasks, filterCategory, filterStatus])

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-slate-400 animate-pulse">Loading…</div>
      </div>
    )
  }

  if (error || !modelData) {
    return (
      <div className="m-6 p-4 rounded-lg" style={{ backgroundColor: 'rgba(255,107,107,0.1)', color: BAD }}>
        {error ?? `Model "${decodedName}" not found`}
        <div className="mt-3">
          <Link to="/" style={{ color: ACCENT }}>← Back to Leaderboard</Link>
        </div>
      </div>
    )
  }

  const totals = summaryData?.totals
  const taskPassPct = totals?.task_pass_pct ?? 0

  return (
    <div className="px-4 py-6 max-w-screen-xl mx-auto">
      {/* Breadcrumb */}
      <div className="mb-4">
        <Link to="/" className="text-sm hover:underline" style={{ color: ACCENT }}>
          ← Leaderboard
        </Link>
      </div>

      {/* Hero card */}
      <div
        className="rounded-xl border p-6 mb-6"
        style={{ backgroundColor: CARD_BG, borderColor: BORDER }}
      >
        <div className="flex flex-wrap items-start gap-6">
          <div className="flex-1 min-w-0">
            <h1 className="text-2xl font-bold text-white mb-1 truncate">{decodedName}</h1>
            <div className="flex items-center gap-3 text-sm text-slate-400">
              <span className="capitalize">{modelData.lang}</span>
              <span>·</span>
              <span>{modelData.mode}</span>
              <span>·</span>
              <span>{modelData.model.route_api_model}</span>
            </div>
          </div>

          {totals && (
            <div className="flex gap-6">
              <div className="text-center">
                <div className="text-3xl font-bold tabular-nums" style={{ color: pctColor(taskPassPct) }}>
                  {taskPassPct.toFixed(1)}%
                </div>
                <div className="text-xs text-slate-500 mt-0.5">Task Pass Rate</div>
              </div>
              <div className="text-center">
                <div className="text-3xl font-bold text-white tabular-nums">
                  {totals.passed_tests}
                  <span className="text-slate-500 text-xl">/{totals.total_tests}</span>
                </div>
                <div className="text-xs text-slate-500 mt-0.5">Tests Passed</div>
              </div>
              <div className="text-center">
                <div className="text-3xl font-bold text-white tabular-nums">
                  {totals.tasks}
                </div>
                <div className="text-xs text-slate-500 mt-0.5">Total Tasks</div>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Category breakdown */}
      {summaryData && (
        <div
          className="rounded-xl border mb-6 overflow-hidden"
          style={{ backgroundColor: CARD_BG, borderColor: BORDER }}
        >
          <div className="px-4 py-3 border-b" style={{ borderColor: BORDER }}>
            <h2 className="text-sm font-semibold text-slate-300">Category Breakdown</h2>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr style={{ borderBottom: `1px solid ${BORDER}` }}>
                  <th className="px-4 py-2 text-left text-slate-400 font-semibold">Category</th>
                  <th className="px-4 py-2 text-left text-slate-400 font-semibold">Task Pass%</th>
                  <th className="px-4 py-2 text-left text-slate-400 font-semibold">Test Pass%</th>
                  <th className="px-4 py-2 text-left text-slate-400 font-semibold">Tests</th>
                  <th className="px-4 py-2 text-left text-slate-400 font-semibold">Tasks</th>
                </tr>
              </thead>
              <tbody>
                {Object.entries(summaryData.categories)
                  .sort(([, a], [, b]) => b.task_pass_pct - a.task_pass_pct)
                  .map(([cat, cs]) => (
                    <tr key={cat} style={{ borderBottom: `1px solid ${BORDER}` }}>
                      <td className="px-4 py-2">
                        <Link
                          to={`/category/${encodeURIComponent(cat)}`}
                          className="capitalize hover:underline"
                          style={{ color: ACCENT }}
                        >
                          {cat}
                        </Link>
                      </td>
                      <td className="px-4 py-2">
                        <span className="font-semibold" style={{ color: pctColor(cs.task_pass_pct) }}>
                          {cs.task_pass_pct.toFixed(1)}%
                        </span>
                      </td>
                      <td className="px-4 py-2">
                        <span className="font-semibold" style={{ color: pctColor(cs.pass_pct) }}>
                          {cs.pass_pct.toFixed(1)}%
                        </span>
                      </td>
                      <td className="px-4 py-2 text-slate-400 tabular-nums">
                        {cs.passed_tests} / {cs.total_tests}
                      </td>
                      <td className="px-4 py-2 text-slate-400 tabular-nums">
                        {cs.tasks}
                      </td>
                    </tr>
                  ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Task list */}
      <div>
        <div className="flex flex-wrap items-center gap-3 mb-4">
          <h2 className="text-sm font-semibold text-slate-300">
            Tasks ({filteredTasks.length} shown)
          </h2>
          <div className="flex-1" />

          {/* Category filter */}
          <select
            value={filterCategory}
            onChange={(e) => setFilterCategory(e.target.value)}
            className="px-2 py-1 rounded text-xs outline-none cursor-pointer"
            style={{ backgroundColor: CARD_BG, color: '#94a3b8', border: `1px solid ${BORDER}` }}
          >
            <option value="all">All categories</option>
            {categories.map((c) => (
              <option key={c} value={c}>{c}</option>
            ))}
          </select>

          {/* Status filter */}
          <div className="flex gap-1">
            {(['all', 'pass', 'fail'] as const).map((s) => (
              <button
                key={s}
                onClick={() => setFilterStatus(s)}
                className="px-2 py-1 rounded text-xs font-medium transition-colors capitalize"
                style={
                  filterStatus === s
                    ? { backgroundColor: '#202126', color: ACCENT }
                    : { color: '#64748b' }
                }
              >
                {s}
              </button>
            ))}
          </div>
        </div>

        <div className="space-y-2">
          {filteredTasks
            .sort((a, b) => a.task.localeCompare(b.task))
            .map((task) => (
              <TaskRow key={task.task} task={task} />
            ))}
          {filteredTasks.length === 0 && (
            <div className="py-8 text-center text-slate-500 text-sm">No tasks match the current filter.</div>
          )}
        </div>
      </div>
    </div>
  )
}
