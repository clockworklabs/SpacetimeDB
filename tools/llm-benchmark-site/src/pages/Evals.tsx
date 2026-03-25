import { useEffect, useState } from 'react'

const CARD_BG = '#141416'
const BORDER = '#202126'
const ACCENT = '#4cf490'
const MUTED = '#6f7987'

interface TaskEntry {
  id: string
  title: string
  description: string
}

interface TasksManifest {
  categories: Record<string, TaskEntry[]>
}

export default function Evals() {
  const [manifest, setManifest] = useState<TasksManifest | null>(null)
  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const [search, setSearch] = useState('')

  useEffect(() => {
    fetch('../../docs/llms/tasks-manifest.json')
      .then(r => r.json())
      .then(setManifest)
      .catch(console.error)
  }, [])

  if (!manifest) {
    return (
      <div className="max-w-5xl mx-auto px-6 py-10" style={{ color: MUTED }}>
        Loading evals...
      </div>
    )
  }

  const q = search.toLowerCase()
  const totalTasks = Object.values(manifest.categories).reduce((s, t) => s + t.length, 0)

  function toggle(id: string) {
    setExpanded(prev => {
      const next = new Set(prev)
      next.has(id) ? next.delete(id) : next.add(id)
      return next
    })
  }

  return (
    <div className="max-w-5xl mx-auto px-6 py-10">
      <div className="mb-8">
        <h1 className="text-2xl font-bold mb-1">Evals</h1>
        <p className="text-sm" style={{ color: MUTED }}>
          {totalTasks} tasks across {Object.keys(manifest.categories).length} categories
        </p>
      </div>

      <input
        type="text"
        placeholder="Search tasks..."
        value={search}
        onChange={e => setSearch(e.target.value)}
        className="w-full mb-8 px-4 py-2 rounded text-sm outline-none"
        style={{
          backgroundColor: CARD_BG,
          border: `1px solid ${BORDER}`,
          color: '#e6e9f0',
        }}
      />

      {Object.entries(manifest.categories).map(([cat, tasks]) => {
        const filtered = tasks.filter(t =>
          !q || t.title.toLowerCase().includes(q) || t.description.toLowerCase().includes(q)
        )
        if (!filtered.length) return null
        return (
          <div key={cat} className="mb-8">
            <div className="flex items-center gap-3 mb-3">
              <h2 className="text-sm font-semibold uppercase tracking-widest" style={{ color: ACCENT }}>
                {cat.replace(/_/g, ' ')}
              </h2>
              <span className="text-xs px-2 py-0.5 rounded-full" style={{ backgroundColor: BORDER, color: MUTED }}>
                {filtered.length}
              </span>
            </div>
            <div className="space-y-px">
              {filtered.map(task => {
                const isOpen = expanded.has(task.id)
                const summary = task.description.split('\n')[0]
                  .replace(/^Write a SpacetimeDB backend module in (?:Rust|C#|TypeScript) that /, '')
                return (
                  <div
                    key={task.id}
                    className="overflow-hidden"
                    style={{ borderBottom: `1px solid ${BORDER}` }}
                  >
                    <button
                      className="w-full grid gap-x-4 px-2 py-2.5 text-left hover:bg-white/5 transition-colors"
                      style={{ gridTemplateColumns: '3rem 14rem 1fr 1.5rem' }}
                      onClick={() => toggle(task.id)}
                    >
                      <span className="text-xs font-mono self-center" style={{ color: MUTED }}>
                        {task.id.match(/t_\d+/)?.[0]}
                      </span>
                      <span className="text-sm font-medium self-center truncate">{task.title}</span>
                      <span className="text-xs self-center truncate" style={{ color: MUTED }}>{summary}</span>
                      <span className="text-xs self-center text-right" style={{ color: MUTED }}>{isOpen ? '▲' : '▼'}</span>
                    </button>
                    {isOpen && (
                      <div className="px-2 pb-4 pt-1">
                        <pre
                          className="text-xs whitespace-pre-wrap font-mono leading-relaxed"
                          style={{ color: '#c8d0dc' }}
                        >
                          {task.description}
                        </pre>
                      </div>
                    )}
                  </div>
                )
              })}
            </div>
          </div>
        )
      })}
    </div>
  )
}
