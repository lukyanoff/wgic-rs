import { useJaegerTrace } from './jaeger'

/** The call's trace from Jaeger: one row per span, indented by depth, with a bar on the trace's timeline. */
export function TracePanel({ traceId }: { traceId: string }) {
  const spans = useJaegerTrace(traceId)
  const t0 = spans[0]?.startUs ?? 0
  const total = Math.max(1, ...spans.map((s) => s.startUs + s.durationUs - t0))

  return (
    <footer className="trace-panel">
      <div className="trace-head">
        <span>
          Trace <code>{traceId}</code> {spans.length === 0 ? '(waiting for Jaeger…)' : `(${spans.length} spans)`}
        </span>
        <a href={`http://localhost:16686/trace/${traceId}`} target="_blank" rel="noreferrer">
          Open in Jaeger
        </a>
      </div>
      {spans.map((s) => (
        <div key={s.id} className={s.error ? 'trace-row error' : 'trace-row'}>
          <span className="trace-name" style={{ paddingLeft: s.depth * 14 }}>
            <b>{s.service}</b> {s.name}
          </span>
          <span className="trace-bar-track">
            <span
              className="trace-bar"
              style={{
                left: `${((s.startUs - t0) / total) * 100}%`,
                width: `${Math.max(0.5, (s.durationUs / total) * 100)}%`,
              }}
            />
          </span>
          <span className="trace-ms">{(s.durationUs / 1000).toFixed(0)} ms</span>
        </div>
      ))}
    </footer>
  )
}
