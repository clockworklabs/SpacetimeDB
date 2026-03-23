import { useState, useMemo } from 'react'
import { Link } from 'react-router-dom'
import { useData } from '../hooks/useData'
import type { LeaderboardRow } from '../types'

const ACCENT = '#69b3ff'
const OK = '#38d39f'
const BAD = '#ff6b6b'
const CARD_BG = '#111824'
const BORDER = '#1e2a38'

function pctColor(pct: number): string {
  if (pct >= 80) return OK
  if (pct >= 50) return '#f0a500'
  return BAD
}

function PctBar({ pct }: { pct: number }) {
  return (
    <div className="flex items-center gap-2">
      <div
        className="h-1.5 rounded-full flex-1 overflow-hidden"
        style={{ backgroundColor: '#1e2a38', maxWidth: 80 }}
      >
        <div
          className="h-full rounded-full transition-all"
          style={{ width: `${pct}%`, backgroundColor: pctColor(pct) }}
        />
      </div>
      <span className="text-sm font-semibold tabular-nums" style={{ color: pctColor(pct) }}>
        {pct.toFixed(1)}%
      </span>
    </div>
  )
}

function RankBadge({ rank }: { rank: number }) {
  const colors: Record<number, { bg: string; text: string }> = {
    1: { bg: 'rgba(255,215,0,0.15)', text: '#ffd700' },
    2: { bg: 'rgba(192,192,192,0.15)', text: '#c0c0c0' },
    3: { bg: 'rgba(205,127,50,0.15)', text: '#cd7f32' },
  }
  const style = colors[rank] ?? { bg: 'rgba(30,42,56,0.8)', text: '#64748b' }
  return (
    <span
      className="inline-flex items-center justify-center w-7 h-7 rounded-full text-sm font-bold"
      style={{ backgroundColor: style.bg, color: style.text }}
    >
      {rank}
    </span>
  )
}

export default function Leaderboard() {
  const { details, summary, loading, error } = useData()

  const [lang, setLang] = useState<string>('')
  const [mode, setMode] = useState<string>('')

  // Derive available languages & modes
  const languages = useMemo(() => {
    if (summary) return Object.keys(summary.by_language).sort()
    if (details) return details.languages.map((l) => l.lang).sort()
    return []
  }, [summary, details])

  const modes = useMemo(() => {
    const l = lang || languages[0]
    if (!l) return []
    if (summary) return Object.keys(summary.by_language[l]?.modes ?? {}).sort()
    if (details) {
      const langData = details.languages.find((x) => x.lang === l)
      return langData ? langData.modes.map((m) => m.mode).sort() : []
    }
    return []
  }, [lang, languages, summary, details])

  const activeLang = lang || languages[0] || ''
  const activeMode = mode || modes[0] || ''

  // Build leaderboard rows from summary
  const rows: LeaderboardRow[] = useMemo(() => {
    if (!summary || !activeLang || !activeMode) return []
    const modeData = summary.by_language[activeLang]?.modes[activeMode]
    if (!modeData) return []

    const result: LeaderboardRow[] = Object.entries(modeData.models).map(([modelName, modelSummary]) => {
      const cats: LeaderboardRow['categories'] = {}
      for (const [cat, cs] of Object.entries(modelSummary.categories)) {
        cats[cat] = {
          passed: cs.passed_tests,
          total: cs.total_tests,
          taskPassPct: cs.task_pass_pct,
        }
      }
      return {
        rank: 0,
        modelName,
        taskPassPct: modelSummary.totals.task_pass_pct,
        passedTests: modelSummary.totals.passed_tests,
        totalTests: modelSummary.totals.total_tests,
        tasksPassed: modelSummary.totals.tasks,
        totalTasks: modelSummary.totals.tasks,
        categories: cats,
      }
    })

    result.sort((a, b) => b.taskPassPct - a.taskPassPct)
    result.forEach((r, i) => { r.rank = i + 1 })
    return result
  }, [summary, activeLang, activeMode])

  // All category names for column headers
  const allCategories = useMemo(() => {
    const cats = new Set<string>()
    rows.forEach((r) => Object.keys(r.categories).forEach((c) => cats.add(c)))
    return Array.from(cats).sort()
  }, [rows])

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-slate-400 animate-pulse">Loading benchmark data…</div>
      </div>
    )
  }

  if (error) {
    return (
      <div
        className="m-6 p-4 rounded-lg border"
        style={{ backgroundColor: 'rgba(255,107,107,0.1)', borderColor: 'rgba(255,107,107,0.3)', color: BAD }}
      >
        <p className="font-semibold mb-1">Failed to load benchmark data</p>
        <p className="text-sm opacity-80">{error}</p>
        <p className="text-sm mt-2 text-slate-400">
          Make sure the JSON files exist at <code className="text-xs">../../docs/llms/</code> relative to the dev server.
        </p>
      </div>
    )
  }

  return (
    <div className="px-4 py-6 max-w-screen-2xl mx-auto">
      {/* Header */}
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-white mb-1">Leaderboard</h1>
        <p className="text-slate-400 text-sm">
          Models ranked by task pass rate — how many tasks they fully solve.
        </p>
      </div>

      {/* Controls */}
      <div className="flex flex-wrap items-center gap-4 mb-6">
        {/* Language tabs */}
        <div className="flex items-center gap-1 p-1 rounded-lg" style={{ backgroundColor: CARD_BG }}>
          {languages.map((l) => (
            <button
              key={l}
              onClick={() => { setLang(l); setMode('') }}
              className="px-3 py-1.5 rounded text-sm font-medium transition-colors capitalize"
              style={
                activeLang === l
                  ? { backgroundColor: '#1e2a38', color: ACCENT }
                  : { color: '#64748b' }
              }
            >
              {l}
            </button>
          ))}
        </div>

        {/* Mode selector */}
        {modes.length > 1 && (
          <select
            value={activeMode}
            onChange={(e) => setMode(e.target.value)}
            className="px-3 py-1.5 rounded text-sm font-medium outline-none cursor-pointer"
            style={{ backgroundColor: CARD_BG, color: '#94a3b8', border: `1px solid ${BORDER}` }}
          >
            {modes.map((m) => (
              <option key={m} value={m}>{m}</option>
            ))}
          </select>
        )}

        <div className="flex-1" />

        {summary && (
          <span className="text-xs text-slate-500">
            Generated {new Date(summary.generated_at).toLocaleString()}
          </span>
        )}
      </div>

      {/* Table */}
      <div
        className="rounded-xl border overflow-x-auto"
        style={{ backgroundColor: CARD_BG, borderColor: BORDER }}
      >
        <table className="w-full text-sm border-collapse">
          <thead>
            <tr style={{ borderBottom: `1px solid ${BORDER}` }}>
              <th className="px-4 py-3 text-left font-semibold text-slate-400 whitespace-nowrap">Rank</th>
              <th className="px-4 py-3 text-left font-semibold text-slate-400 whitespace-nowrap">Model</th>
              <th className="px-4 py-3 text-left font-semibold text-slate-400 whitespace-nowrap">Task Pass%</th>
              <th className="px-4 py-3 text-left font-semibold text-slate-400 whitespace-nowrap">Tests</th>
              {allCategories.map((cat) => (
                <th key={cat} className="px-4 py-3 text-left font-semibold whitespace-nowrap" style={{ color: '#64748b' }}>
                  <Link
                    to={`/category/${encodeURIComponent(cat)}`}
                    className="hover:underline capitalize"
                    style={{ color: '#64748b' }}
                    onClick={(e) => e.stopPropagation()}
                  >
                    {cat}
                  </Link>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, idx) => (
              <tr
                key={row.modelName}
                className="transition-colors hover:cursor-pointer"
                style={{ borderBottom: idx < rows.length - 1 ? `1px solid ${BORDER}` : undefined }}
                onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = '#151f2e')}
                onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = '')}
              >
                <td className="px-4 py-3">
                  <RankBadge rank={row.rank} />
                </td>
                <td className="px-4 py-3">
                  <Link
                    to={`/model/${encodeURIComponent(row.modelName)}`}
                    className="font-medium hover:underline"
                    style={{ color: ACCENT }}
                  >
                    {row.modelName}
                  </Link>
                </td>
                <td className="px-4 py-3">
                  <PctBar pct={row.taskPassPct} />
                </td>
                <td className="px-4 py-3 text-slate-400 tabular-nums whitespace-nowrap">
                  {row.passedTests} / {row.totalTests}
                </td>
                {allCategories.map((cat) => {
                  const c = row.categories[cat]
                  if (!c) {
                    return (
                      <td key={cat} className="px-4 py-3 text-center">
                        <span className="text-slate-600 text-xs">—</span>
                      </td>
                    )
                  }
                  return (
                    <td key={cat} className="px-4 py-3">
                      <div className="flex flex-col gap-0.5">
                        <span
                          className="text-xs font-semibold tabular-nums"
                          style={{ color: pctColor(c.taskPassPct) }}
                        >
                          {c.taskPassPct.toFixed(0)}%
                        </span>
                        <span className="text-xs text-slate-500 tabular-nums">
                          {c.passed}/{c.total}
                        </span>
                      </div>
                    </td>
                  )
                })}
              </tr>
            ))}
            {rows.length === 0 && (
              <tr>
                <td colSpan={4 + allCategories.length} className="px-4 py-12 text-center text-slate-500">
                  No data for {activeLang} / {activeMode}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      {/* Legend */}
      <div className="flex items-center gap-4 mt-4 text-xs text-slate-500">
        <span>Task Pass% = fraction of tasks fully solved (stricter than raw test pass%)</span>
        <span className="flex items-center gap-1">
          <span className="w-2 h-2 rounded-full inline-block" style={{ backgroundColor: OK }} />
          ≥80%
        </span>
        <span className="flex items-center gap-1">
          <span className="w-2 h-2 rounded-full inline-block" style={{ backgroundColor: '#f0a500' }} />
          ≥50%
        </span>
        <span className="flex items-center gap-1">
          <span className="w-2 h-2 rounded-full inline-block" style={{ backgroundColor: BAD }} />
          &lt;50%
        </span>
      </div>
    </div>
  )
}
