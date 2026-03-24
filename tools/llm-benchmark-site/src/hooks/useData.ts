import { useState, useEffect } from 'react'
import type { BenchmarkDetails, BenchmarkSummary, HistorySnapshot } from '../types'

const DETAILS_URL = '../../docs/llms/llm-comparison-details.json'
const SUMMARY_URL = '../../docs/llms/llm-comparison-summary.json'
const HISTORY_MANIFEST_URL = '../../docs/llms/history/manifest.json'

interface UseDataResult {
  details: BenchmarkDetails | null
  summary: BenchmarkSummary | null
  loading: boolean
  error: string | null
}

export function useData(): UseDataResult {
  const [details, setDetails] = useState<BenchmarkDetails | null>(null)
  const [summary, setSummary] = useState<BenchmarkSummary | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false

    async function load() {
      try {
        const [detailsRes, summaryRes] = await Promise.all([
          fetch(DETAILS_URL),
          fetch(SUMMARY_URL),
        ])

        if (!detailsRes.ok) throw new Error(`Failed to load details: ${detailsRes.status} ${detailsRes.statusText}`)
        if (!summaryRes.ok) throw new Error(`Failed to load summary: ${summaryRes.status} ${summaryRes.statusText}`)

        const [detailsData, summaryData] = await Promise.all([
          detailsRes.json() as Promise<BenchmarkDetails>,
          summaryRes.json() as Promise<BenchmarkSummary>,
        ])

        if (!cancelled) {
          setDetails(detailsData)
          setSummary(summaryData)
          setError(null)
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err))
        }
      } finally {
        if (!cancelled) setLoading(false)
      }
    }

    load()
    return () => { cancelled = true }
  }, [])

  return { details, summary, loading, error }
}

// ── History files ─────────────────────────────────────────────────────────────

interface UseHistoryResult {
  snapshots: Array<{ filename: string; data: HistorySnapshot }>
  loading: boolean
}

export function useHistoryFiles(): UseHistoryResult {
  const [snapshots, setSnapshots] = useState<Array<{ filename: string; data: HistorySnapshot }>>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    let cancelled = false

    async function load() {
      try {
        // Try loading a manifest.json that lists history files
        const manifestRes = await fetch(HISTORY_MANIFEST_URL)
        if (!manifestRes.ok) {
          // No manifest → no history yet
          if (!cancelled) setLoading(false)
          return
        }

        const filenames: string[] = await manifestRes.json()

        const results = await Promise.allSettled(
          filenames.map(async (filename) => {
            const res = await fetch(`../../docs/llms/history/${filename}`)
            if (!res.ok) throw new Error(`Failed: ${filename}`)
            const data = await res.json() as HistorySnapshot
            return { filename, data }
          })
        )

        if (!cancelled) {
          const loaded = results
            .filter((r): r is PromiseFulfilledResult<{ filename: string; data: HistorySnapshot }> => r.status === 'fulfilled')
            .map((r) => r.value)
            .sort((a, b) => a.data.generated_at.localeCompare(b.data.generated_at))

          setSnapshots(loaded)
        }
      } catch {
        // Silently fail — history is optional
      } finally {
        if (!cancelled) setLoading(false)
      }
    }

    load()
    return () => { cancelled = true }
  }, [])

  return { snapshots, loading }
}
