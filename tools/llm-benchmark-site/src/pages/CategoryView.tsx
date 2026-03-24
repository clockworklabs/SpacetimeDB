import { useState, useMemo } from 'react'
import { useParams, Link } from 'react-router-dom'
import { useData } from '../hooks/useData'
import PassChip from '../components/PassChip'
import type { TaskResult } from '../types'

const ACCENT = '#4cf490'
const BAD = '#ff4c4c'
const CARD_BG = '#141416'
const BORDER = '#202126'

interface CellInfo {
  task: TaskResult
  modelName: string
}

interface ModalProps {
  cell: CellInfo | null
  onClose: () => void
}

function OutputModal({ cell, onClose }: ModalProps) {
  if (!cell) return null

  const passed = cell.task.passed_tests >= cell.task.total_tests

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ backgroundColor: 'rgba(0,0,0,0.7)' }}
      onClick={onClose}
    >
      <div
        className="rounded-xl border w-full max-w-3xl max-h-[80vh] flex flex-col"
        style={{ backgroundColor: '#0d0d0e', borderColor: BORDER }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Modal header */}
        <div className="flex items-center justify-between px-5 py-4 border-b" style={{ borderColor: BORDER }}>
          <div>
            <div className="flex items-center gap-2">
              <PassChip passed={passed} />
              <span className="font-mono text-sm text-white">{cell.task.task}</span>
            </div>
            <div className="mt-1 text-xs text-slate-400">{cell.modelName}</div>
          </div>
          <button
            onClick={onClose}
            className="text-slate-400 hover:text-white transition-colors"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Scorer details */}
        {Object.keys(cell.task.scorer_details).length > 0 && (
          <div className="px-5 py-3 border-b flex flex-wrap gap-2" style={{ borderColor: BORDER }}>
            {Object.entries(cell.task.scorer_details).map(([key, detail]) => (
              <div
                key={key}
                className="flex items-center gap-2 px-2 py-1 rounded text-xs"
                style={{ backgroundColor: '#141416', border: `1px solid ${BORDER}` }}
              >
                <PassChip passed={detail.pass} size="sm" />
                <span className="font-mono text-slate-300">{key}</span>
                {detail.partial != null && (
                  <span className="text-slate-500">{(detail.partial * 100).toFixed(0)}%</span>
                )}
              </div>
            ))}
          </div>
        )}

        {/* LLM output */}
        <div className="flex-1 overflow-y-auto px-5 py-4">
          <pre
            className="text-xs text-slate-300 whitespace-pre-wrap break-words font-mono leading-relaxed"
          >
            {cell.task.llm_output || <span className="text-slate-600 italic">No output recorded</span>}
          </pre>
        </div>
      </div>
    </div>
  )
}

export default function CategoryView() {
  const { name } = useParams<{ name: string }>()
  const decodedCat = decodeURIComponent(name ?? '')
  const { details, loading, error } = useData()
  const [activeCell, setActiveCell] = useState<CellInfo | null>(null)

  // Gather all tasks for this category + all model names
  const { taskNames, modelNames, matrix } = useMemo(() => {
    if (!details) return { taskNames: [], modelNames: [], matrix: new Map() }

    const taskSet = new Set<string>()
    const modelSet = new Set<string>()
    // matrix: taskName → modelName → TaskResult
    const matrix = new Map<string, Map<string, TaskResult>>()

    for (const lang of details.languages) {
      for (const modeObj of lang.modes) {
        for (const modelObj of modeObj.models) {
          for (const task of Object.values(modelObj.tasks)) {
            if (task.category !== decodedCat) continue
            taskSet.add(task.task)
            modelSet.add(modelObj.name)
            if (!matrix.has(task.task)) matrix.set(task.task, new Map())
            matrix.get(task.task)!.set(modelObj.name, task)
          }
        }
      }
    }

    return {
      taskNames: Array.from(taskSet).sort(),
      modelNames: Array.from(modelSet).sort(),
      matrix,
    }
  }, [details, decodedCat])

  // Per-model pass counts for header summary
  const modelStats = useMemo(() => {
    const stats: Record<string, { passed: number; total: number }> = {}
    for (const modelName of modelNames) {
      let passed = 0, total = 0
      for (const taskName of taskNames) {
        const t = matrix.get(taskName)?.get(modelName)
        if (t) {
          total++
          if (t.passed_tests >= t.total_tests) passed++
        }
      }
      stats[modelName] = { passed, total }
    }
    return stats
  }, [modelNames, taskNames, matrix])

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-slate-400 animate-pulse">Loading…</div>
      </div>
    )
  }

  if (error || taskNames.length === 0) {
    return (
      <div className="m-6 p-4 rounded-lg" style={{ backgroundColor: 'rgba(255,107,107,0.1)', color: BAD }}>
        {error ?? `Category "${decodedCat}" not found or has no tasks`}
        <div className="mt-3">
          <Link to="/" style={{ color: ACCENT }}>← Back to Leaderboard</Link>
        </div>
      </div>
    )
  }

  return (
    <div className="px-4 py-6 max-w-screen-2xl mx-auto">
      {/* Breadcrumb */}
      <div className="mb-4">
        <Link to="/" className="text-sm hover:underline" style={{ color: ACCENT }}>
          ← Leaderboard
        </Link>
      </div>

      <div className="mb-6">
        <h1 className="text-2xl font-bold text-white mb-1 capitalize">{decodedCat}</h1>
        <p className="text-slate-400 text-sm">
          {taskNames.length} tasks · {modelNames.length} models · Click any cell to see LLM output
        </p>
      </div>

      {/* Matrix table */}
      <div
        className="rounded-xl border overflow-x-auto"
        style={{ backgroundColor: CARD_BG, borderColor: BORDER }}
      >
        <table className="text-sm border-collapse">
          <thead>
            <tr style={{ borderBottom: `1px solid ${BORDER}` }}>
              <th
                className="px-4 py-3 text-left font-semibold text-slate-400 sticky left-0 z-10"
                style={{ backgroundColor: CARD_BG, minWidth: 240, borderRight: `1px solid ${BORDER}` }}
              >
                Task
              </th>
              {modelNames.map((m) => {
                const s = modelStats[m]
                return (
                  <th
                    key={m}
                    className="px-3 py-3 text-center font-semibold whitespace-nowrap"
                    style={{ color: '#64748b', minWidth: 120 }}
                  >
                    <Link
                      to={`/model/${encodeURIComponent(m)}`}
                      className="hover:underline block"
                      style={{ color: ACCENT }}
                    >
                      {m}
                    </Link>
                    <span className="text-xs font-normal text-slate-500 block mt-0.5">
                      {s?.passed ?? 0}/{s?.total ?? 0}
                    </span>
                  </th>
                )
              })}
            </tr>
          </thead>
          <tbody>
            {taskNames.map((taskName, rowIdx) => (
              <tr
                key={taskName}
                style={{ borderBottom: rowIdx < taskNames.length - 1 ? `1px solid ${BORDER}` : undefined }}
              >
                {/* Task name cell */}
                <td
                  className="px-4 py-2 font-mono text-xs text-slate-300 sticky left-0 z-10"
                  style={{ backgroundColor: '#0d0d0e', borderRight: `1px solid ${BORDER}` }}
                >
                  {taskName}
                </td>

                {/* Model result cells */}
                {modelNames.map((modelName) => {
                  const task = matrix.get(taskName)?.get(modelName)
                  if (!task) {
                    return (
                      <td key={modelName} className="px-3 py-2 text-center">
                        <PassChip passed={null} size="sm" />
                      </td>
                    )
                  }
                  const passed = task.passed_tests >= task.total_tests
                  return (
                    <td key={modelName} className="px-3 py-2 text-center">
                      <button
                        onClick={() => setActiveCell({ task, modelName })}
                        className="hover:opacity-80 transition-opacity"
                        title={`${modelName}: ${task.passed_tests}/${task.total_tests} tests passed`}
                      >
                        <PassChip
                          passed={passed}
                          label={`${task.passed_tests}/${task.total_tests}`}
                          size="sm"
                        />
                      </button>
                    </td>
                  )
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* LLM Output Modal */}
      <OutputModal cell={activeCell} onClose={() => setActiveCell(null)} />
    </div>
  )
}
