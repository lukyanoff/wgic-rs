import { useEffect, useMemo, useState, useSyncExternalStore } from 'react'
import { pollTrace } from './jaeger'
import {
  getExportStatus,
  jaegerTraceUrl,
  localSpans,
  spansVersion,
  subscribeExportStatus,
  subscribeSpans,
  type UiEvent,
  type UiSpan,
} from './otel'

// Shows one trace: browser spans at once (in-memory), then the full trace from Jaeger
// (browser + NATS + Rust services), as a waterfall plus every correlated log line.

const NO_SPANS: UiSpan[] = []

const SERVICE_COLORS = [
  '#6366f1',
  '#0ea5e9',
  '#f59e0b',
  '#10b981',
  '#ec4899',
  '#8b5cf6',
  '#14b8a6',
  '#f97316',
]

function colorOf(service: string, services: string[]): string {
  return SERVICE_COLORS[Math.max(0, services.indexOf(service)) % SERVICE_COLORS.length]
}

function fmtUs(us: number): string {
  if (us < 1000) return `${us.toFixed(0)}µs`
  if (us < 1_000_000) return `${(us / 1000).toFixed(us < 10_000 ? 2 : 1)}ms`
  return `${(us / 1_000_000).toFixed(2)}s`
}

function fmtVal(v: unknown): string {
  if (typeof v === 'string') return v
  return JSON.stringify(v)
}

interface Row {
  span: UiSpan
  depth: number
}

/** Depth-first order under each root; roots are spans whose parent isn't in the trace. */
function orderSpans(spans: UiSpan[]): Row[] {
  const ids = new Set(spans.map((s) => s.spanId))
  const children = new Map<string, UiSpan[]>()
  const roots: UiSpan[] = []
  for (const s of spans) {
    if (s.parentId && ids.has(s.parentId)) {
      const list = children.get(s.parentId) ?? []
      list.push(s)
      children.set(s.parentId, list)
    } else roots.push(s)
  }
  const byStart = (a: UiSpan, b: UiSpan) => a.startUs - b.startUs
  const out: Row[] = []
  const walk = (s: UiSpan, depth: number) => {
    out.push({ span: s, depth })
    for (const c of (children.get(s.spanId) ?? []).sort(byStart)) walk(c, depth + 1)
  }
  roots.sort(byStart).forEach((r) => walk(r, 0))
  return out
}

interface LogLine {
  event: UiEvent
  span: UiSpan
}

// Shown elsewhere in the row (level, target) or just noise in a log line.
const LOG_HIDDEN = new Set([
  'level',
  'target',
  'trace_id',
  'span_id',
  'code.filepath',
  'code.namespace',
  'code.lineno',
  'code.file.path',
  'code.line.number',
  'code.module.name',
  'thread.id',
  'thread.name',
])

export function TracePanel({ traceId }: { traceId: string }) {
  useSyncExternalStore(subscribeSpans, spansVersion)
  const exportStatus = useSyncExternalStore(subscribeExportStatus, getExportStatus)
  const browser = localSpans(traceId)
  const [remote, setRemote] = useState<{ traceId: string; spans: UiSpan[] }>()
  // Poll result for one (trace, refresh) key; a stale key means "loading".
  const [state, setState] = useState<{ key: string; status: 'done' | 'error'; error?: string }>()
  const [selected, setSelected] = useState<string>()
  const [tab, setTab] = useState<'waterfall' | 'logs'>('waterfall')
  const [nonce, setNonce] = useState(0)

  const key = `${traceId}#${nonce}`
  useEffect(() => {
    const ctrl = new AbortController()
    pollTrace(traceId, (spans) => setRemote({ traceId, spans }), ctrl.signal)
      .then(() => !ctrl.signal.aborted && setState({ key, status: 'done' }))
      .catch((e) => !ctrl.signal.aborted && setState({ key, status: 'error', error: String(e) }))
    return () => ctrl.abort()
  }, [traceId, key])

  const remoteSpans = useMemo(
    () => (remote?.traceId === traceId ? remote.spans : NO_SPANS),
    [remote, traceId],
  )
  const status = state?.key === key ? state : { status: 'loading' as const, error: undefined }

  const spans = useMemo(() => {
    const m = new Map<string, UiSpan>()
    for (const s of browser) m.set(s.spanId, s)
    for (const s of remoteSpans) m.set(s.spanId, s) // Jaeger's copy wins (same data, all services)
    return [...m.values()]
  }, [browser, remoteSpans])

  const rows = useMemo(() => orderSpans(spans), [spans])
  const services = useMemo(() => [...new Set(rows.map((r) => r.span.service))], [rows])
  const t0 = Math.min(...spans.map((s) => s.startUs))
  const t1 = Math.max(...spans.map((s) => s.startUs + s.durationUs))
  const total = Math.max(1, t1 - t0)
  const logs: LogLine[] = useMemo(
    () =>
      spans
        .flatMap((span) => span.events.map((event) => ({ event, span })))
        .sort((a, b) => a.event.timeUs - b.event.timeUs),
    [spans],
  )
  const sel = spans.find((s) => s.spanId === selected)
  const errors = spans.filter((s) => s.status.code === 'error').length

  return (
    <section className="trace">
      <div className="trace-head">
        <span className="pane-title-text">Trace</span>
        <code className="mono small" title="trace id">
          {traceId}
        </code>
        <button className="link small" onClick={() => navigator.clipboard?.writeText(traceId)}>
          copy
        </button>
        <a className="small" href={jaegerTraceUrl(traceId)} target="_blank" rel="noreferrer">
          Open in Jaeger ↗
        </a>
        <span className="spacer" />
        <span className="dim small">
          {spans.length} span{spans.length === 1 ? '' : 's'} · {services.length} service
          {services.length === 1 ? '' : 's'}
          {spans.length > 0 && ` · ${fmtUs(total)}`}
          {errors > 0 && (
            <span className="bad">
              {' '}
              · {errors} error{errors === 1 ? '' : 's'}
            </span>
          )}
        </span>
        <span className={`dim small trace-state ${status?.status ?? ''}`}>
          {status?.status === 'loading'
            ? 'collecting…'
            : status?.status === 'error'
              ? 'Jaeger unreachable'
              : remoteSpans.length === 0
                ? 'not in Jaeger'
                : 'complete'}
        </span>
        <button className="link small" onClick={() => setNonce((n) => n + 1)}>
          Refresh
        </button>
      </div>
      {status?.status === 'error' && (
        <div className="error small">
          {status.error}. Is Jaeger running (<code>podman compose up -d</code>) and is the
          playground under <code>npm run dev</code>?
        </div>
      )}
      {exportStatus.state === 'error' && (
        <div className="error small">
          Browser spans are not reaching the collector: {exportStatus.error}
        </div>
      )}

      <div className="trace-legend">
        {services.map((s) => (
          <span key={s} className="small">
            <i style={{ background: colorOf(s, services) }} /> {s}
          </span>
        ))}
        <span className="spacer" />
        <div className="segmented" role="radiogroup">
          <button
            className={tab === 'waterfall' ? 'active' : ''}
            onClick={() => setTab('waterfall')}
          >
            Spans
          </button>
          <button className={tab === 'logs' ? 'active' : ''} onClick={() => setTab('logs')}>
            Logs ({logs.length})
          </button>
        </div>
      </div>

      {tab === 'waterfall' && (
        <div className="waterfall">
          {rows.map(({ span: s, depth }) => (
            <button
              key={s.spanId}
              className={`wf-row ${selected === s.spanId ? 'active' : ''} ${s.status.code === 'error' ? 'err' : ''}`}
              onClick={() => setSelected(selected === s.spanId ? undefined : s.spanId)}
              title={`${s.service} · ${s.kind} · ${fmtUs(s.durationUs)}`}
            >
              <span className="wf-label" style={{ paddingLeft: depth * 14 }}>
                <i style={{ background: colorOf(s.service, services) }} />
                <span className="mono">{s.name}</span>
                <span className="dim small">{s.kind}</span>
                {s.events.length > 0 && <span className="badge">{s.events.length} log</span>}
              </span>
              <span className="wf-track">
                <span
                  className="wf-bar"
                  style={{
                    left: `${((s.startUs - t0) / total) * 100}%`,
                    width: `max(2px, ${(s.durationUs / total) * 100}%)`,
                    background: colorOf(s.service, services),
                  }}
                />
                <span className="wf-dur mono small">{fmtUs(s.durationUs)}</span>
              </span>
            </button>
          ))}
          {rows.length === 0 && <div className="dim small">No spans yet.</div>}
        </div>
      )}

      {tab === 'waterfall' && sel && <SpanDetail span={sel} t0={t0} />}

      {tab === 'logs' && (
        <div className="trace-logs">
          {logs.map(({ event, span }, i) => (
            <LogRow
              key={i}
              event={event}
              span={span}
              t0={t0}
              color={colorOf(span.service, services)}
              onOpen={() => {
                setSelected(span.spanId)
                setTab('waterfall')
              }}
            />
          ))}
          {logs.length === 0 && <div className="dim small">No log events in this trace (yet).</div>}
        </div>
      )}
    </section>
  )
}

function LogRow({
  event,
  span,
  t0,
  color,
  onOpen,
}: {
  event: UiEvent
  span: UiSpan
  t0: number
  color: string
  onOpen?: () => void
}) {
  const level = String(event.attributes.level ?? 'event').toLowerCase()
  const fields = Object.entries(event.attributes).filter(([k]) => !LOG_HIDDEN.has(k))
  return (
    <div className="log-row">
      <span className="mono small dim">+{fmtUs(event.timeUs - t0)}</span>
      <span className={`lvl lvl-${level}`}>{level}</span>
      <button className="link small log-span" onClick={onOpen} title={`span ${span.spanId}`}>
        <i style={{ background: color }} />
        {span.service}
      </button>
      <span className="log-msg">
        {event.name}
        {fields.map(([k, v]) => (
          <span key={k} className="mono small kv">
            {' '}
            <span className="dim">{k}=</span>
            {fmtVal(v)}
          </span>
        ))}
        {event.attributes.target !== undefined && (
          <span className="dim small"> · {String(event.attributes.target)}</span>
        )}
      </span>
    </div>
  )
}

function SpanDetail({ span, t0 }: { span: UiSpan; t0: number }) {
  const attrs = Object.entries(span.attributes).sort(([a], [b]) => a.localeCompare(b))
  return (
    <div className="span-detail">
      <div className="span-meta small">
        <b className="mono">{span.name}</b>
        <span>
          service <b>{span.service}</b>
        </span>
        <span>
          kind <b>{span.kind}</b>
        </span>
        <span>
          duration <b>{fmtUs(span.durationUs)}</b>
        </span>
        <span>
          status{' '}
          <b
            className={span.status.code === 'error' ? 'bad' : span.status.code === 'ok' ? 'ok' : ''}
          >
            {span.status.code}
            {span.status.message ? `: ${span.status.message}` : ''}
          </b>
        </span>
        <span className="mono dim">span {span.spanId}</span>
        {span.parentId && <span className="mono dim">parent {span.parentId}</span>}
        <span className="dim">
          {span.source === 'browser' ? 'from browser (not yet in Jaeger)' : 'from Jaeger'}
        </span>
      </div>
      {attrs.length > 0 && (
        <table className="attrs">
          <tbody>
            {attrs.map(([k, v]) => (
              <tr key={k}>
                <td className="mono dim">{k}</td>
                <td className="mono">{fmtVal(v)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {span.events.length > 0 && (
        <div className="trace-logs">
          {span.events.map((e, i) => (
            <LogRow key={i} event={e} span={span} t0={t0} color="transparent" />
          ))}
        </div>
      )}
    </div>
  )
}
