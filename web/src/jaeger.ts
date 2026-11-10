import type { UiEvent, UiSpan } from './otel'

// Reads traces back from Jaeger's query API through the Vite dev proxy
// (/__jaeger → JAEGER_URL, default http://localhost:16686), so no CORS is needed.

const BASE = '/__jaeger'

interface JTag {
  key: string
  type: string
  value: unknown
}
interface JLog {
  timestamp: number
  fields: JTag[]
}
interface JSpan {
  traceID: string
  spanID: string
  operationName: string
  references?: { refType: string; traceID: string; spanID: string }[]
  startTime: number
  duration: number
  tags?: JTag[]
  logs?: JLog[]
  processID: string
}
interface JTrace {
  traceID: string
  spans: JSpan[]
  processes: Record<string, { serviceName: string; tags?: JTag[] }>
}

const tagMap = (tags?: JTag[]) => Object.fromEntries((tags ?? []).map((t) => [t.key, t.value]))

function normalize(t: JTrace): UiSpan[] {
  return t.spans.map((s) => {
    const attrs = tagMap(s.tags)
    const status = String(attrs['otel.status_code'] ?? '').toLowerCase()
    const error = status === 'error' || attrs.error === true
    const kind = String(attrs['span.kind'] ?? 'internal')
    for (const k of ['span.kind', 'otel.status_code', 'otel.status_description', 'error', 'internal.span.format'])
      delete attrs[k]
    const events: UiEvent[] = (s.logs ?? []).map((l) => {
      const f = tagMap(l.fields)
      const name = String(f.event ?? f.message ?? 'event')
      delete f.event
      return { timeUs: l.timestamp, name, attributes: f }
    })
    const parent = s.references?.find((r) => r.refType === 'CHILD_OF') ?? s.references?.[0]
    return {
      traceId: s.traceID.padStart(32, '0'),
      spanId: s.spanID.padStart(16, '0'),
      parentId: parent?.spanID.padStart(16, '0'),
      service: t.processes[s.processID]?.serviceName ?? '?',
      name: s.operationName,
      kind,
      startUs: s.startTime,
      durationUs: s.duration,
      attributes: attrs,
      events,
      status: {
        code: error ? 'error' : status === 'ok' ? 'ok' : 'unset',
        message: tagMap(s.tags)['otel.status_description'] as string | undefined,
      },
      source: 'jaeger',
    }
  })
}

/** Spans of one trace, or [] when Jaeger doesn't have it (yet). */
export async function fetchTrace(traceId: string, signal?: AbortSignal): Promise<UiSpan[]> {
  const res = await fetch(`${BASE}/api/traces/${traceId}`, { signal })
  if (res.status === 404) return []
  if (!res.ok) throw new Error(`Jaeger: HTTP ${res.status} ${await res.text().catch(() => '')}`.trim())
  const body = (await res.json()) as { data?: JTrace[] }
  return (body.data ?? []).flatMap(normalize)
}

/**
 * Exports are batched, so a fresh trace fills in over a second or two. Re-fetch until the
 * span count stops growing (or `maxMs` passes), reporting every change.
 */
export async function pollTrace(
  traceId: string,
  onSpans: (spans: UiSpan[]) => void,
  signal: AbortSignal,
  { intervalMs = 700, maxMs = 12_000, stableRounds = 3 } = {},
): Promise<void> {
  const started = Date.now()
  let last = -1
  let stable = 0
  while (!signal.aborted && Date.now() - started < maxMs) {
    const spans = await fetchTrace(traceId, signal)
    if (spans.length !== last) {
      last = spans.length
      stable = 0
      onSpans(spans)
    } else if (spans.length > 0 && ++stable >= stableRounds) {
      return
    }
    await new Promise((r) => setTimeout(r, intervalMs))
  }
}
