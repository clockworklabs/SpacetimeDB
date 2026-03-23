// ── Details JSON ─────────────────────────────────────────────────────────────

export interface ScorerDetail {
  pass: boolean
  partial: number
  notes: Record<string, unknown>
}

export interface TaskResult {
  task: string
  lang: string
  model_name: string
  total_tests: number
  passed_tests: number
  llm_output: string
  category: string
  scorer_details: Record<string, ScorerDetail>
  vendor: string
  started_at: string
  finished_at: string
}

export interface ModelResult {
  name: string
  route_api_model: string
  tasks: Record<string, TaskResult>
}

export interface ModeResult {
  mode: string
  hash: string
  models: ModelResult[]
}

export interface LanguageResult {
  lang: string
  modes: ModeResult[]
}

export interface BenchmarkDetails {
  generated_at: string
  languages: LanguageResult[]
}

// ── Summary JSON ──────────────────────────────────────────────────────────────

export interface CategorySummary {
  tasks: number
  total_tests: number
  passed_tests: number
  pass_pct: number
  task_pass_pct: number
}

export interface ModelSummary {
  categories: Record<string, CategorySummary>
  totals: CategorySummary
}

export interface ModeSummary {
  hash: string
  models: Record<string, ModelSummary>
}

export interface LanguageSummary {
  modes: Record<string, ModeSummary>
}

export interface BenchmarkSummary {
  version: number
  generated_at: string
  by_language: Record<string, LanguageSummary>
}

// ── History ───────────────────────────────────────────────────────────────────

export interface HistorySnapshot {
  generated_at: string
  by_language: Record<string, LanguageSummary>
}

// ── UI helpers ────────────────────────────────────────────────────────────────

export interface LeaderboardRow {
  rank: number
  modelName: string
  taskPassPct: number
  passedTests: number
  totalTests: number
  tasksPassed: number
  totalTasks: number
  categories: Record<string, { passed: number; total: number; taskPassPct: number }>
}
