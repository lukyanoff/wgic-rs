import { useEffect, useState } from 'react'

// Reads one trace back from Jaeger's query API through the Vite proxy (/__jaeger → JAEGER_URL), so no CORS.

export interface TraceSpan {
  id: string
  service: string
  name: string
  startUs: number
  durationUs: number
  error: boolean
  depth: number
}

interface JSpan {
  spanID: string
  operationName: string
  references?: { refType: string; spanID: string }[]
  startTime: number
  duration: number
  tags?: { key: string; value: unknown }[]
  processID: string
}

interface JTrace {
  spans: JSpan[]
  processes: Record<string, { serviceName: string }>
}

function isError(s: JSpan): boolean {
  return (s.tags ?? []).some(
    (t) => (t.key === 'error' && t.value === true) || (t.key === 'otel.status_code' && t.value === 'ERROR'),
  )
}

/** Jaeger's spans → a list in start order, each with its depth in the call tree. */
function toTraceSpans(t: JTrace): TraceSpan[] {
  const parentOf = new Map(t.spans.map((s) => [s.spanID, s.references?.find((r) => r.refType === 'CHILD_OF')?.spanID]))
  const depth = (id: string | undefined, seen = 0): number => {
    const parent = id ? parentOf.get(id) : undefined
    return parent && parentOf.has(parent) && seen < 50 ? 1 + depth(parent, seen + 1) : 0
  }
  return t.spans
    .map((s) => ({
      id: s.spanID,
      service: t.processes[s.processID]?.serviceName ?? '?',
      name: s.operationName,
      startUs: s.startTime,
      durationUs: s.duration,
      error: isError(s),
      depth: depth(s.spanID),
    }))
    .sort((a, b) => a.startUs - b.startUs)
}

async function fetchTrace(traceId: string, signal: AbortSignal): Promise<TraceSpan[]> {
  const res = await fetch(`/__jaeger/api/traces/${traceId}`, { signal })
  if (!res.ok) return [] // 404 until the spans are exported
  const body = (await res.json()) as { data?: JTrace[] }
  return (body.data ?? []).flatMap(toTraceSpans)
}

/** Spans of `traceId`, re-fetched every second until the count is stable (exports are batched). */
export function useJaegerTrace(traceId: string): TraceSpan[] {
  const [spans, setSpans] = useState<TraceSpan[]>([])
  useEffect(() => {
    const abort = new AbortController()
    const poll = async () => {
      let stableRounds = 0
      for (let round = 0; round < 15 && stableRounds < 3 && !abort.signal.aborted; round++) {
        const next = await fetchTrace(traceId, abort.signal).catch(() => [])
        setSpans((prev) => (next.length === prev.length ? prev : next))
        stableRounds = next.length > 0 ? stableRounds + 1 : 0
        await new Promise((r) => setTimeout(r, 1000))
      }
    }
    void poll()
    return () => abort.abort()
  }, [traceId])
  return spans
}
