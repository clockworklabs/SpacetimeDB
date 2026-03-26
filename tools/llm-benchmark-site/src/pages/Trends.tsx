import { useMemo, useState } from 'react'

function modeLabel(mode: string): string {
  if (mode === 'no_context') return 'No Context'
  if (mode === 'docs') return 'With Docs'
  if (mode === 'search') return 'Web Search'
  return mode
}
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
  ResponsiveContainer,
} from 'recharts'
import { useHistoryFiles } from '../hooks/useData'

const ACCENT = '#4cf490'
const ALLOWED_MODES = ['docs', 'no_context', 'search']
const CARD_BG = '#141416'
const BORDER = '#202126'

// Palette for lines — enough for many models
const LINE_COLORS = [
  '#4cf490', '#02befa', '#fbdc8e', '#ff4c4c', '#a880ff',
  '#fb923c', '#34d399', '#60a5fa', '#f472b6', '#a3e635',
  '#fbbf24', '#4ade80', '#e879f9', '#38bdf8', '#f87171',
]

interface ChartPoint {
  date: string
  [modelName: string]: number | string
}

function formatDate(iso: string): string {
  try {
    const d = new Date(iso)
    return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
  } catch {
    return iso.slice(0, 10)
  }
}

export default function Trends() {
  const { snapshots, loading } = useHistoryFiles()
  const [activeLang, setActiveLang] = useState<string>('')
  const [activeMode, setActiveMode] = useState<string>('')

  const { languages, modes } = useMemo(() => {
    const langSet = new Set<string>()
    const modeSet = new Set<string>()
    for (const snap of snapshots) {
      for (const [lang, langData] of Object.entries(snap.data.by_language)) {
        langSet.add(lang)
        for (const mode of Object.keys(langData.modes)) {
          modeSet.add(mode)
        }
      }
    }
    return {
      languages: Array.from(langSet).sort(),
      modes: Array.from(modeSet).filter((m) => ALLOWED_MODES.includes(m)).sort(),
    }
  }, [snapshots])

  const lang = activeLang || languages[0] || ''
  const mode = activeMode || (modes.includes('docs') ? 'docs' : modes[0]) || ''

  // Build chart data: one point per snapshot
  const { chartData, modelNames } = useMemo(() => {
    const modelSet = new Set<string>()
    const chartData: ChartPoint[] = []

    for (const snap of snapshots) {
      const modeData = snap.data.by_language[lang]?.modes[mode]
      if (!modeData) continue

      const point: ChartPoint = { date: formatDate(snap.data.generated_at) }
      for (const [modelName, modelSummary] of Object.entries(modeData.models)) {
        modelSet.add(modelName)
        point[modelName] = Math.round(modelSummary.totals.task_pass_pct * 10) / 10
      }
      chartData.push(point)
    }

    return { chartData, modelNames: Array.from(modelSet).sort() }
  }, [snapshots, lang, mode])

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-slate-400 animate-pulse">Loading history…</div>
      </div>
    )
  }

  return (
    <div className="px-4 py-6 max-w-screen-xl mx-auto">
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-white mb-1">Trends</h1>
        <p className="text-slate-400 text-sm">
          Task pass rate over time, per model. Run the periodic CI workflow to populate history.
        </p>
      </div>

      {snapshots.length === 0 ? (
        // ── No history ──────────────────────────────────────────────────────
        <div
          className="rounded-xl border p-12 text-center"
          style={{ backgroundColor: CARD_BG, borderColor: BORDER }}
        >
          <div className="text-5xl mb-4">📈</div>
          <h2 className="text-lg font-semibold text-slate-200 mb-2">No history data yet</h2>
          <p className="text-slate-400 max-w-md mx-auto text-sm leading-relaxed">
            Run the periodic CI workflow to start tracking trends. Each run will save a timestamped
            snapshot to <code className="text-xs bg-[#202126] px-1 py-0.5 rounded">docs/llms/history/</code> and
            update the <code className="text-xs bg-[#202126] px-1 py-0.5 rounded">manifest.json</code> file.
          </p>
        </div>
      ) : (
        // ── Chart ────────────────────────────────────────────────────────────
        <>
          {/* Controls */}
          <div className="flex flex-wrap items-center gap-4 mb-6">
            {/* Language tabs */}
            <div className="flex items-center gap-1 p-1 rounded-lg" style={{ backgroundColor: CARD_BG }}>
              {languages.map((l) => (
                <button
                  key={l}
                  onClick={() => { setActiveLang(l); setActiveMode('') }}
                  className="px-3 py-1.5 rounded text-sm font-medium transition-colors capitalize"
                  style={
                    lang === l
                      ? { backgroundColor: '#202126', color: ACCENT }
                      : { color: '#64748b' }
                  }
                >
                  {l}
                </button>
              ))}
            </div>

            {modes.length > 1 && (
              <select
                value={mode}
                onChange={(e) => setActiveMode(e.target.value)}
                className="px-3 py-1.5 rounded text-sm font-medium outline-none cursor-pointer"
                style={{ backgroundColor: CARD_BG, color: '#94a3b8', border: `1px solid ${BORDER}` }}
              >
                {modes.map((m) => (
                  <option key={m} value={m}>{modeLabel(m)}</option>
                ))}
              </select>
            )}

            <span className="text-xs text-slate-500 ml-auto">
              {snapshots.length} snapshot{snapshots.length !== 1 ? 's' : ''} loaded
            </span>
          </div>

          {/* Chart card */}
          <div
            className="rounded-xl border p-6"
            style={{ backgroundColor: CARD_BG, borderColor: BORDER }}
          >
            <h2 className="text-sm font-semibold text-slate-400 mb-4 uppercase tracking-wide">
              Task Pass Rate Over Time
            </h2>

            {chartData.length === 0 ? (
              <div className="text-center text-slate-500 py-12 text-sm">
                No data for {lang} / {mode}
              </div>
            ) : (
              <ResponsiveContainer width="100%" height={400}>
                <LineChart data={chartData} margin={{ top: 5, right: 30, left: 0, bottom: 5 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke={BORDER} />
                  <XAxis
                    dataKey="date"
                    tick={{ fill: '#64748b', fontSize: 12 }}
                    axisLine={{ stroke: BORDER }}
                    tickLine={false}
                  />
                  <YAxis
                    domain={[0, 100]}
                    tick={{ fill: '#64748b', fontSize: 12 }}
                    axisLine={false}
                    tickLine={false}
                    tickFormatter={(v: number) => `${v}%`}
                  />
                  <Tooltip
                    contentStyle={{
                      backgroundColor: '#0d0d0e',
                      border: `1px solid ${BORDER}`,
                      borderRadius: 8,
                      color: '#e2e8f0',
                      fontSize: 12,
                    }}
                    formatter={(value: number, name: string) => [`${value}%`, name]}
                    labelStyle={{ color: '#94a3b8', marginBottom: 4 }}
                  />
                  <Legend
                    wrapperStyle={{ color: '#94a3b8', fontSize: 12, paddingTop: 16 }}
                  />
                  {modelNames.map((modelName, idx) => (
                    <Line
                      key={modelName}
                      type="monotone"
                      dataKey={modelName}
                      stroke={LINE_COLORS[idx % LINE_COLORS.length]}
                      strokeWidth={2}
                      dot={{ r: 3, fill: LINE_COLORS[idx % LINE_COLORS.length] }}
                      activeDot={{ r: 5 }}
                      connectNulls
                    />
                  ))}
                </LineChart>
              </ResponsiveContainer>
            )}
          </div>

          {/* Snapshot list */}
          <div
            className="mt-6 rounded-xl border overflow-hidden"
            style={{ backgroundColor: CARD_BG, borderColor: BORDER }}
          >
            <div className="px-4 py-3 border-b" style={{ borderColor: BORDER }}>
              <h2 className="text-sm font-semibold text-slate-400">Snapshots</h2>
            </div>
            <div className="divide-y" style={{ borderColor: BORDER }}>
              {[...snapshots].reverse().map((snap) => (
                <div key={snap.filename} className="px-4 py-2 flex items-center gap-4 text-sm">
                  <span className="font-mono text-xs text-slate-500">{snap.filename}</span>
                  <span className="text-slate-400">
                    {new Date(snap.data.generated_at).toLocaleString()}
                  </span>
                </div>
              ))}
            </div>
          </div>
        </>
      )}
    </div>
  )
}
