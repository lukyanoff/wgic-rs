import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import {
  decodePayload,
  formatMs,
  formatNanos,
  groupServices,
  microInfo,
  microPing,
  microStats,
  natsPublish,
  natsRequest,
  natsSubscribe,
  parseHeaderLines,
  serverInfo,
  serviceEndpoints,
  type HeaderList,
  type MicroInfo,
  type MicroPing,
  type MicroStats,
  type ReqResult,
  type TailMsg,
  type Timed,
} from './natsTools'
import { endSpan, extractContext, flushSpans, SpanKind, startSpan } from './otel'
import { TracePanel } from './TracePanel'

type Tab = 'services' | 'ping' | 'stats' | 'request' | 'raw' | 'subscribe' | 'server'

const TABS: { id: Tab; label: string }[] = [
  { id: 'services', label: 'Services' },
  { id: 'ping', label: 'Ping' },
  { id: 'stats', label: 'Stats' },
  { id: 'request', label: 'Request' },
  { id: 'raw', label: 'Raw' },
  { id: 'subscribe', label: 'Subscribe' },
  { id: 'server', label: 'Server' },
]

interface Preset {
  service: string
  endpoint: string
  /** bumps so the same preset can be applied twice */
  n: number
}

interface Props {
  url: string
  /** Open an RPC path (`/pkg.Service/Method`) in the RPC tab; false if not in the schema. */
  onOpenMethod: (rpcPath: string) => boolean
  hasMethod: (rpcPath: string) => boolean
}

export function NatsConsole({ url, onOpenMethod, hasMethod }: Props) {
  const [tab, setTab] = useState<Tab>('services')
  const [preset, setPreset] = useState<Preset>()

  const openRequest = (service: string, endpoint: string) => {
    setPreset((p) => ({ service, endpoint, n: (p?.n ?? 0) + 1 }))
    setTab('request')
  }

  return (
    <div className="nats-console">
      <nav className="subtabs" role="tablist">
        {TABS.map((t) => (
          <button
            key={t.id}
            role="tab"
            aria-selected={tab === t.id}
            className={tab === t.id ? 'active' : ''}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
        <span className="dim small mono right">{url}</span>
      </nav>
      {/* Keep state across sub-tabs; Subscribe unmounts so it unsubscribes. */}
      <div hidden={tab !== 'services'}>
        <ServicesTab
          url={url}
          onOpenMethod={onOpenMethod}
          hasMethod={hasMethod}
          onRequest={openRequest}
        />
      </div>
      <div hidden={tab !== 'ping'}>
        <PingTab url={url} />
      </div>
      <div hidden={tab !== 'stats'}>
        <StatsTab url={url} active={tab === 'stats'} />
      </div>
      <div hidden={tab !== 'request'}>
        <RequestTab url={url} preset={preset} />
      </div>
      <div hidden={tab !== 'raw'}>
        <RawTab url={url} />
      </div>
      {tab === 'subscribe' && <SubscribeTab url={url} />}
      <div hidden={tab !== 'server'}>
        <ServerTab url={url} active={tab === 'server'} />
      </div>
    </div>
  )
}

// ---------------------------------------------------------------- shared bits

const shQuote = (s: string) => `'${s.replace(/'/g, `'\\''`)}'`

function CliHint({ cmd }: { cmd: string }) {
  const [copied, setCopied] = useState(false)
  return (
    <div className="cli">
      <code>
        <span className="dim">$ </span>
        {cmd}
      </code>
      <button
        className="link"
        onClick={() => {
          navigator.clipboard?.writeText(cmd).then(
            () => {
              setCopied(true)
              setTimeout(() => setCopied(false), 1200)
            },
            () => {},
          )
        }}
      >
        {copied ? 'Copied' : 'Copy'}
      </button>
    </div>
  )
}

function Toolbar({ children }: { children: ReactNode }) {
  return <div className="toolbar">{children}</div>
}

function Field({ label, children, grow }: { label: string; children: ReactNode; grow?: boolean }) {
  return (
    <label className={`field ${grow ? 'grow' : ''}`}>
      <span>{label}</span>
      {children}
    </label>
  )
}

function ErrorBox({ error }: { error?: string }) {
  return error ? <div className="error">{error}</div> : null
}

function Empty({ children }: { children: ReactNode }) {
  return <div className="empty-row dim">{children}</div>
}

function HeadersView({ headers }: { headers: HeaderList }) {
  if (headers.length === 0) return null
  return (
    <table className="kv">
      <tbody>
        {headers.map(([k, v], i) => (
          <tr key={i} className={/^nats-service-error/i.test(k) ? 'bad' : ''}>
            <th>{k}</th>
            <td className="mono">{v}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

function PayloadView({ data }: { data: Uint8Array }) {
  const d = useMemo(() => decodePayload(data), [data])
  if (d.kind === 'empty') return <div className="dim small">empty payload</div>
  return (
    <div className="payload">
      <span className="badge">
        {d.kind} · {data.length} B
      </span>
      <pre className="code">{d.text}</pre>
    </div>
  )
}

function useAsync<T>(fn: () => Promise<T>) {
  const [data, setData] = useState<T>()
  const [error, setError] = useState<string>()
  const [loading, setLoading] = useState(false)
  const fnRef = useRef(fn)
  useEffect(() => {
    fnRef.current = fn
  })
  const run = useCallback(async () => {
    setLoading(true)
    setError(undefined)
    try {
      setData(await fnRef.current())
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [])
  return { data, error, loading, run }
}

const metaText = (m?: Record<string, string> | null) =>
  m && Object.keys(m).length
    ? Object.entries(m)
        .map(([k, v]) => `${k}: ${v}`)
        .join(', ')
    : ''

// ------------------------------------------------- micro ls / info (Services)

function ServicesTab({
  url,
  onOpenMethod,
  hasMethod,
  onRequest,
}: {
  url: string
  onOpenMethod: (rpc: string) => boolean
  hasMethod: (rpc: string) => boolean
  onRequest: (service: string, endpoint: string) => void
}) {
  const [name, setName] = useState('')
  const [id, setId] = useState('')
  const [epFilter, setEpFilter] = useState('')
  const [json, setJson] = useState(false)
  const [open, setOpen] = useState<Record<string, boolean>>({})
  const { data, error, loading, run } = useAsync(() => microInfo(url, name, id))

  useEffect(() => {
    run()
  }, [url, run])

  const epRe = useMemo(() => {
    try {
      return epFilter ? new RegExp(epFilter, 'i') : undefined
    } catch {
      return undefined
    }
  }, [epFilter])
  const services = useMemo(() => groupServices(data ?? []), [data])
  const cmd = name
    ? `nats micro info ${name}${id ? ` ${id}` : ''}${epFilter ? ` -e ${shQuote(epFilter)}` : ''}${json ? ' -j' : ''}`
    : `nats micro ls${json ? ' -j' : ''}`

  return (
    <section className="tool">
      <Toolbar>
        <Field label="service">
          <input value={name} placeholder="all" onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label="instance id">
          <input
            value={id}
            placeholder="all"
            disabled={!name}
            onChange={(e) => setId(e.target.value)}
          />
        </Field>
        <Field label="endpoint filter (-e)">
          <input
            value={epFilter}
            placeholder="regex"
            onChange={(e) => setEpFilter(e.target.value)}
          />
        </Field>
        <label className="check">
          <input type="checkbox" checked={json} onChange={(e) => setJson(e.target.checked)} /> JSON
        </label>
        <button className="primary" onClick={run} disabled={loading}>
          {loading ? 'Loading…' : 'Refresh'}
        </button>
      </Toolbar>
      <CliHint cmd={cmd} />
      <ErrorBox error={error} />
      {data && json && <pre className="code">{JSON.stringify(data, null, 2)}</pre>}
      {data && !json && services.length === 0 && <Empty>No micro services running.</Empty>}
      {data &&
        !json &&
        services.map((s) => {
          const isOpen = open[s.name] ?? services.length === 1
          const endpoints = serviceEndpoints(s.instances).filter((e) => !epRe || epRe.test(e.name))
          return (
            <div key={s.name} className="card">
              <button
                className="card-head"
                aria-expanded={isOpen}
                onClick={() => setOpen((o) => ({ ...o, [s.name]: !isOpen }))}
              >
                <span className="caret">{isOpen ? '▾' : '▸'}</span>
                <span className="mono strong">{s.name}</span>
                <span className="badge">
                  {s.instances.length} instance{s.instances.length === 1 ? '' : 's'}
                </span>
                <span className="dim small">v{s.versions.join(', v')}</span>
                <span className="dim small right">{s.description}</span>
              </button>
              {isOpen && (
                <div className="card-body">
                  <h4>Endpoints</h4>
                  {endpoints.length === 0 ? (
                    <Empty>No endpoints match.</Empty>
                  ) : (
                    <table className="grid">
                      <thead>
                        <tr>
                          <th>Name</th>
                          <th>Subject</th>
                          <th>Queue</th>
                          <th>Metadata</th>
                          <th />
                        </tr>
                      </thead>
                      <tbody>
                        {endpoints.map((e) => {
                          const rpc = e.metadata?.rpc
                          return (
                            <tr key={e.name}>
                              <td className="mono">{e.name}</td>
                              <td className="mono">{e.subject}</td>
                              <td className="mono">{e.queue_group}</td>
                              <td className="mono small">{metaText(e.metadata)}</td>
                              <td className="actions">
                                <button className="link" onClick={() => onRequest(s.name, e.name)}>
                                  Request
                                </button>
                                {rpc && (
                                  <button
                                    className="link"
                                    disabled={!hasMethod(rpc)}
                                    title={
                                      hasMethod(rpc) ? rpc : `${rpc} is not in the loaded schema`
                                    }
                                    onClick={() => onOpenMethod(rpc)}
                                  >
                                    Open in RPC
                                  </button>
                                )}
                              </td>
                            </tr>
                          )
                        })}
                      </tbody>
                    </table>
                  )}
                  <h4>Instances</h4>
                  <table className="grid">
                    <thead>
                      <tr>
                        <th>ID</th>
                        <th>Version</th>
                        <th>Endpoints</th>
                        <th>Metadata</th>
                      </tr>
                    </thead>
                    <tbody>
                      {s.instances.map((i) => (
                        <tr key={i.id}>
                          <td className="mono">{i.id}</td>
                          <td>{i.version}</td>
                          <td>{i.endpoints?.length ?? 0}</td>
                          <td className="mono small">{metaText(i.metadata)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )
        })}
    </section>
  )
}

// ------------------------------------------------------------- micro ping

function PingTab({ url }: { url: string }) {
  const [name, setName] = useState('')
  const { data, error, loading, run } = useAsync<Timed<MicroPing>[]>(() => microPing(url, name))
  return (
    <section className="tool">
      <Toolbar>
        <Field label="service">
          <input value={name} placeholder="all" onChange={(e) => setName(e.target.value)} />
        </Field>
        <button className="primary" onClick={run} disabled={loading}>
          {loading ? 'Pinging…' : 'Ping'}
        </button>
      </Toolbar>
      <CliHint cmd={`nats micro ping${name ? ` ${name}` : ''}`} />
      <ErrorBox error={error} />
      {data && data.length === 0 && <Empty>No micro services answered.</Empty>}
      {data && data.length > 0 && (
        <table className="grid">
          <thead>
            <tr>
              <th>Service</th>
              <th>ID</th>
              <th>Version</th>
              <th className="num">RTT</th>
            </tr>
          </thead>
          <tbody>
            {data.map(({ value: p, rttMs }) => (
              <tr key={p.id}>
                <td className="mono">{p.name}</td>
                <td className="mono">{p.id}</td>
                <td>{p.version}</td>
                <td className="num mono">{formatMs(rttMs)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  )
}

// ------------------------------------------------------------ micro stats

function StatsTable({ stats }: { stats: MicroStats[] }) {
  const rows = stats.flatMap((s) => (s.endpoints ?? []).map((e) => ({ s, e })))
  if (rows.length === 0) return <Empty>No stats.</Empty>
  const total = rows.reduce(
    (t, { e }) => ({
      req: t.req + e.num_requests,
      err: t.err + e.num_errors,
      time: t.time + e.processing_time,
    }),
    { req: 0, err: 0, time: 0 },
  )
  return (
    <table className="grid">
      <thead>
        <tr>
          <th>Service</th>
          <th>ID</th>
          <th>Endpoint</th>
          <th className="num">Requests</th>
          <th className="num">Errors</th>
          <th>Last error</th>
          <th className="num">Processing</th>
          <th className="num">Average</th>
        </tr>
      </thead>
      <tbody>
        {rows.map(({ s, e }) => (
          <tr key={`${s.id}/${e.name}`}>
            <td className="mono">{s.name}</td>
            <td className="mono" title={s.started ? `started ${s.started}` : undefined}>
              {s.id}
            </td>
            <td className="mono">{e.name}</td>
            <td className="num mono">{e.num_requests}</td>
            <td className={`num mono ${e.num_errors ? 'bad' : ''}`}>{e.num_errors}</td>
            <td className="small">{e.last_error}</td>
            <td className="num mono">{formatNanos(e.processing_time)}</td>
            <td className="num mono">{formatNanos(e.average_processing_time)}</td>
          </tr>
        ))}
      </tbody>
      <tfoot>
        <tr>
          <td colSpan={3}>Total</td>
          <td className="num mono">{total.req}</td>
          <td className="num mono">{total.err}</td>
          <td />
          <td className="num mono">{formatNanos(total.time)}</td>
          <td className="num mono">{formatNanos(total.req ? total.time / total.req : 0)}</td>
        </tr>
      </tfoot>
    </table>
  )
}

function StatsTab({ url, active }: { url: string; active: boolean }) {
  const [name, setName] = useState('')
  const [id, setId] = useState('')
  const [json, setJson] = useState(false)
  const [auto, setAuto] = useState(false)
  const { data, error, loading, run } = useAsync(() => microStats(url, name, id))

  useEffect(() => {
    if (!auto || !active) return
    run()
    const t = setInterval(run, 2000)
    return () => clearInterval(t)
  }, [auto, active, run])

  return (
    <section className="tool">
      <Toolbar>
        <Field label="service">
          <input value={name} placeholder="all" onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label="instance id">
          <input
            value={id}
            placeholder="all"
            disabled={!name}
            onChange={(e) => setId(e.target.value)}
          />
        </Field>
        <label className="check">
          <input type="checkbox" checked={json} onChange={(e) => setJson(e.target.checked)} /> JSON
        </label>
        <label className="check">
          <input type="checkbox" checked={auto} onChange={(e) => setAuto(e.target.checked)} />{' '}
          auto-refresh 2s
        </label>
        <button className="primary" onClick={run} disabled={loading}>
          {loading ? 'Loading…' : 'Refresh'}
        </button>
      </Toolbar>
      <CliHint
        cmd={`nats micro stats ${name || '<service>'}${id ? ` ${id}` : ''}${json ? ' -j' : ''}`}
      />
      <ErrorBox error={error} />
      {data && json && <pre className="code">{JSON.stringify(data, null, 2)}</pre>}
      {data &&
        !json &&
        (data.length === 0 ? (
          <Empty>No micro services answered.</Empty>
        ) : (
          <StatsTable stats={data} />
        ))}
    </section>
  )
}

// ------------------------------------------------- micro req (+ --count)

interface Distribution {
  id: string
  delta: number
}

interface CountSummary {
  sent: number
  ok: number
  failed: number
  min: number
  avg: number
  max: number
  distribution?: Distribution[]
}

function ReplyView({ r, trace = true }: { r: ReqResult; trace?: boolean }) {
  return (
    <>
      <div className="reply">
        <div className="reply-status">
          <span className={`status ${r.ok ? 'ok' : 'bad'}`}>
            {r.error ? 'error' : r.serviceError ? `service error ${r.serviceError.code}` : 'ok'}
          </span>
          <span className="dim small mono">{formatMs(r.rttMs)}</span>
        </div>
        <ErrorBox error={r.error ?? (r.serviceError ? r.serviceError.message : undefined)} />
        <HeadersView headers={r.headers} />
        {!r.error && <PayloadView data={r.data} />}
      </div>
      {trace && <TracePanel key={r.traceId} traceId={r.traceId} />}
    </>
  )
}

function RequestTab({ url, preset }: { url: string; preset?: Preset }) {
  const services = useAsync(() => microInfo(url))
  const [service, setService] = useState('')
  const [endpoint, setEndpoint] = useState('')
  const [payload, setPayload] = useState('{"name":"Jane"}')
  const [headers, setHeaders] = useState('')
  const [timeoutMs, setTimeoutMs] = useState('5000')
  const [count, setCount] = useState('1')
  const [running, setRunning] = useState(false)
  const [last, setLast] = useState<ReqResult>()
  const [batchTrace, setBatchTrace] = useState<string>()
  const [summary, setSummary] = useState<CountSummary>()
  const [error, setError] = useState<string>()

  const runServices = services.run
  useEffect(() => {
    runServices()
  }, [url, runServices])

  // Apply "Request" clicks from the Services tab (adjust state when the prop changes).
  const [appliedPreset, setAppliedPreset] = useState(0)
  if (preset && preset.n !== appliedPreset) {
    setAppliedPreset(preset.n)
    setService(preset.service)
    setEndpoint(preset.endpoint)
  }

  const groups = useMemo(() => groupServices(services.data ?? []), [services.data])
  const svc = groups.find((g) => g.name === service) ?? groups[0]
  const endpoints = svc ? serviceEndpoints(svc.instances) : []
  const ep = endpoints.find((e) => e.name === endpoint) ?? endpoints[0]
  const n = Math.max(1, Math.min(1000, Number(count) || 1))
  const hdrs = parseHeaderLines(headers)

  async function send() {
    if (!svc || !ep) return
    setRunning(true)
    setError(undefined)
    setSummary(undefined)
    setBatchTrace(undefined)
    // --count N: one trace with a parent span over all N requests.
    const batch =
      n > 1
        ? startSpan(
            `nats micro req ×${n}`,
            SpanKind.INTERNAL,
            {
              'messaging.system': 'nats',
              'micro.service': svc.name,
              'micro.endpoint': ep.name,
              'playground.count': n,
            },
            extractContext(hdrs),
          )
        : undefined
    let batchError: string | undefined
    try {
      const before = n > 1 ? await microStats(url, svc.name) : undefined
      const rtts: number[] = []
      let ok = 0
      let r: ReqResult | undefined
      for (let i = 0; i < n; i++) {
        r = await natsRequest(url, ep.subject, payload, hdrs, Number(timeoutMs) || 5000, batch?.ctx)
        rtts.push(r.rttMs)
        if (r.ok) ok++
        if (r.error) break // transport failure: stop like the CLI does
      }
      if (batch && ok < rtts.length) batchError = `${rtts.length - ok} of ${rtts.length} failed`
      setLast(r)
      if (n > 1) {
        const after = await microStats(url, svc.name)
        const reqs = (list?: MicroStats[]) =>
          new Map(
            (list ?? []).map((s) => [
              s.id,
              s.endpoints.find((e) => e.name === ep.name)?.num_requests ?? 0,
            ]),
          )
        const b = reqs(before)
        const distribution = [...reqs(after)].map(([id, v]) => ({
          id,
          delta: v - (b.get(id) ?? 0),
        }))
        setSummary({
          sent: rtts.length,
          ok,
          failed: rtts.length - ok,
          min: Math.min(...rtts),
          max: Math.max(...rtts),
          avg: rtts.reduce((a, x) => a + x, 0) / rtts.length,
          distribution: distribution.sort((x, y) => y.delta - x.delta),
        })
      }
    } catch (e) {
      batchError = String(e)
      setError(String(e))
    } finally {
      if (batch) {
        endSpan(batch.span, batchError)
        void flushSpans()
        setBatchTrace(batch.traceId)
      }
      setRunning(false)
    }
  }

  const cmd = `nats micro req ${svc?.name ?? '<service>'} ${ep?.name ?? '<endpoint>'} ${shQuote(payload)}${hdrs
    .map(([k, v]) => ` -H ${shQuote(`${k}:${v}`)}`)
    .join('')}${n > 1 ? ` --count ${n}` : ''}`
  const maxDelta = Math.max(1, ...(summary?.distribution?.map((d) => d.delta) ?? [1]))

  return (
    <section className="tool">
      <Toolbar>
        <Field label="service">
          <select value={svc?.name ?? ''} onChange={(e) => setService(e.target.value)}>
            {groups.length === 0 && (
              <option value="">{services.loading ? 'loading…' : 'no services'}</option>
            )}
            {groups.map((g) => (
              <option key={g.name} value={g.name}>
                {g.name} ({g.instances.length})
              </option>
            ))}
          </select>
        </Field>
        <Field label="endpoint" grow>
          <select value={ep?.name ?? ''} onChange={(e) => setEndpoint(e.target.value)}>
            {endpoints.map((e) => (
              <option key={e.name} value={e.name}>
                {e.name} → {e.subject}
              </option>
            ))}
          </select>
        </Field>
        <button className="link" onClick={services.run} disabled={services.loading}>
          {services.loading ? 'Loading…' : 'Reload'}
        </button>
      </Toolbar>
      <ErrorBox error={services.error} />
      <div className="req-grid">
        <Field label="payload" grow>
          <textarea
            className="code"
            spellCheck={false}
            value={payload}
            onChange={(e) => setPayload(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && (e.ctrlKey || e.metaKey) && send()}
          />
        </Field>
        <Field label="headers (key: value per line)">
          <textarea
            className="code"
            spellCheck={false}
            placeholder="Content-Type: application/json"
            value={headers}
            onChange={(e) => setHeaders(e.target.value)}
          />
        </Field>
      </div>
      <Toolbar>
        <Field label="timeout ms">
          <input
            type="number"
            min={1}
            value={timeoutMs}
            onChange={(e) => setTimeoutMs(e.target.value)}
          />
        </Field>
        <Field label="count (--count)">
          <input
            type="number"
            min={1}
            max={1000}
            value={count}
            onChange={(e) => setCount(e.target.value)}
          />
        </Field>
        <div className="spacer" />
        <button className="primary" onClick={send} disabled={running || !ep}>
          {running ? 'Sending…' : n > 1 ? `Send ×${n}` : 'Send'}
        </button>
      </Toolbar>
      <CliHint cmd={cmd} />
      <ErrorBox error={error} />
      {summary && (
        <div className="summary">
          <div className="summary-stats">
            <span>
              sent <b>{summary.sent}</b>
            </span>
            <span className="ok">
              ok <b>{summary.ok}</b>
            </span>
            <span className={summary.failed ? 'bad' : 'dim'}>
              failed <b>{summary.failed}</b>
            </span>
            <span className="dim mono">
              rtt min {formatMs(summary.min)} · avg {formatMs(summary.avg)} · max{' '}
              {formatMs(summary.max)}
            </span>
          </div>
          <h4>Load distribution (stats delta per instance)</h4>
          {summary.distribution?.length ? (
            <div className="bars">
              {summary.distribution.map((d) => (
                <div key={d.id} className="bar-row">
                  <span className="mono small">{d.id}</span>
                  <div className="bar">
                    <div style={{ width: `${(d.delta / maxDelta) * 100}%` }} />
                  </div>
                  <span className="mono small num">{d.delta}</span>
                </div>
              ))}
            </div>
          ) : (
            <Empty>No stats available.</Empty>
          )}
          <h4>Last reply</h4>
        </div>
      )}
      {last && <ReplyView r={last} trace={!batchTrace} />}
      {batchTrace && <TracePanel key={batchTrace} traceId={batchTrace} />}
    </section>
  )
}

// ---------------------------------------------------------- nats req / pub

function RawTab({ url }: { url: string }) {
  const [mode, setMode] = useState<'req' | 'pub'>('req')
  const [subject, setSubject] = useState('rpc.greet.v1.GreetService.Greet')
  const [payload, setPayload] = useState('{"name":"Raw"}')
  const [headers, setHeaders] = useState('')
  const [timeoutMs, setTimeoutMs] = useState('5000')
  const [result, setResult] = useState<ReqResult>()
  const [published, setPublished] = useState<{ text: string; traceId: string }>()
  const [error, setError] = useState<string>()
  const [running, setRunning] = useState(false)
  const hdrs = parseHeaderLines(headers)

  async function send() {
    if (!subject.trim()) return
    setRunning(true)
    setError(undefined)
    setResult(undefined)
    setPublished(undefined)
    try {
      if (mode === 'req') {
        setResult(await natsRequest(url, subject.trim(), payload, hdrs, Number(timeoutMs) || 5000))
      } else {
        const traceId = await natsPublish(url, subject.trim(), payload, hdrs)
        setPublished({
          text: `Published ${new TextEncoder().encode(payload).length} B to ${subject.trim()}`,
          traceId,
        })
      }
    } catch (e) {
      setError(String(e))
    } finally {
      setRunning(false)
    }
  }

  const cmd = `nats ${mode} ${subject.trim() || '<subject>'} ${shQuote(payload)}${hdrs
    .map(([k, v]) => ` -H ${shQuote(`${k}:${v}`)}`)
    .join('')}${mode === 'req' && timeoutMs !== '5000' ? ` --timeout ${timeoutMs}ms` : ''}`

  return (
    <section className="tool">
      <Toolbar>
        <div className="segmented" role="radiogroup">
          {(['req', 'pub'] as const).map((m) => (
            <button key={m} className={mode === m ? 'active' : ''} onClick={() => setMode(m)}>
              {m === 'req' ? 'request' : 'publish'}
            </button>
          ))}
        </div>
        <Field label="subject" grow>
          <input className="mono" value={subject} onChange={(e) => setSubject(e.target.value)} />
        </Field>
        {mode === 'req' && (
          <Field label="timeout ms">
            <input
              type="number"
              min={1}
              value={timeoutMs}
              onChange={(e) => setTimeoutMs(e.target.value)}
            />
          </Field>
        )}
      </Toolbar>
      <div className="req-grid">
        <Field label="payload" grow>
          <textarea
            className="code"
            spellCheck={false}
            value={payload}
            onChange={(e) => setPayload(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && (e.ctrlKey || e.metaKey) && send()}
          />
        </Field>
        <Field label="headers (key: value per line)">
          <textarea
            className="code"
            spellCheck={false}
            placeholder="Content-Type: application/json"
            value={headers}
            onChange={(e) => setHeaders(e.target.value)}
          />
        </Field>
      </div>
      <Toolbar>
        <div className="spacer" />
        <button className="primary" onClick={send} disabled={running}>
          {running ? 'Sending…' : mode === 'req' ? 'Request' : 'Publish'}
        </button>
      </Toolbar>
      <CliHint cmd={cmd} />
      <ErrorBox error={error} />
      {published && <div className="dim">{published.text}</div>}
      {published && <TracePanel key={published.traceId} traceId={published.traceId} />}
      {result && <ReplyView r={result} />}
    </section>
  )
}

// ---------------------------------------------------------------- nats sub

const TAIL_MAX = 200

function SubscribeTab({ url }: { url: string }) {
  const [subject, setSubject] = useState('rpc.>')
  const [active, setActive] = useState<string>()
  const [paused, setPaused] = useState(false)
  const [msgs, setMsgs] = useState<TailMsg[]>([])
  const [error, setError] = useState<string>()
  const [openIdx, setOpenIdx] = useState<number>()
  const unsubRef = useRef<(() => void) | null>(null)
  const pausedRef = useRef(paused)
  useEffect(() => {
    pausedRef.current = paused
  }, [paused])

  const stop = useCallback(() => {
    unsubRef.current?.()
    unsubRef.current = null
    setActive(undefined)
  }, [])

  useEffect(() => stop, [stop]) // unsubscribe when the tab unmounts

  async function start() {
    stop()
    setError(undefined)
    const subj = subject.trim()
    try {
      unsubRef.current = await natsSubscribe(
        url,
        subj,
        (m) => {
          if (!pausedRef.current) setMsgs((prev) => [m, ...prev].slice(0, TAIL_MAX))
        },
        (e) => {
          setError(String(e))
          setActive(undefined)
        },
      )
      setActive(subj)
    } catch (e) {
      setError(String(e))
    }
  }

  return (
    <section className="tool">
      <Toolbar>
        <Field label="subject (wildcards: * and >)" grow>
          <input
            className="mono"
            value={subject}
            onChange={(e) => setSubject(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && start()}
          />
        </Field>
        {active ? (
          <button onClick={stop}>Unsubscribe</button>
        ) : (
          <button className="primary" onClick={start}>
            Subscribe
          </button>
        )}
        <label className="check">
          <input type="checkbox" checked={paused} onChange={(e) => setPaused(e.target.checked)} />{' '}
          pause
        </label>
        <button className="link" onClick={() => setMsgs([])}>
          Clear
        </button>
      </Toolbar>
      <CliHint cmd={`nats sub ${shQuote(subject.trim() || '>')}`} />
      <ErrorBox error={error} />
      <div className="dim small">
        {active ? `Listening on ${active}` : 'Not subscribed'} · {msgs.length} message
        {msgs.length === 1 ? '' : 's'} (last {TAIL_MAX} kept). Tip: <code>$SRV.&gt;</code> shows
        discovery traffic, <code>_INBOX.&gt;</code> shows replies.
      </div>
      {msgs.length > 0 && (
        <ul className="tail">
          {msgs.map((m, i) => (
            <li key={`${m.at}-${i}`}>
              <button
                className="tail-row"
                onClick={() => setOpenIdx(openIdx === i ? undefined : i)}
              >
                <span className="dim mono small">{new Date(m.at).toLocaleTimeString()}</span>
                <span className="mono">{m.subject}</span>
                {m.reply && <span className="dim mono small">reply → {m.reply}</span>}
                {m.traceId && (
                  <span className="dim mono small" title={`trace ${m.traceId}`}>
                    trace {m.traceId.slice(0, 8)}…
                  </span>
                )}
                <span className="dim small right">
                  {m.headers.length ? `${m.headers.length} hdr · ` : ''}
                  {m.data.length} B
                </span>
              </button>
              {openIdx === i && (
                <div className="tail-detail">
                  <HeadersView headers={m.headers} />
                  <PayloadView data={m.data} />
                  {m.traceId && <TracePanel key={m.traceId} traceId={m.traceId} />}
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

// -------------------------------------------------- server info / nats rtt

function ServerTab({ url, active }: { url: string; active: boolean }) {
  const { data, error, loading, run } = useAsync(() => serverInfo(url))
  useEffect(() => {
    if (active) run()
  }, [active, url, run])
  const entries = Object.entries((data?.info ?? {}) as Record<string, unknown>)
  return (
    <section className="tool">
      <Toolbar>
        <button className="primary" onClick={run} disabled={loading}>
          {loading ? 'Loading…' : 'Refresh'}
        </button>
        {data && (
          <span className="mono">
            connected to {data.server} · rtt <b>{formatMs(data.rttMs)}</b>
          </span>
        )}
      </Toolbar>
      <CliHint cmd="nats rtt  # and the server INFO this connection received" />
      <ErrorBox error={error} />
      {data && (
        <table className="kv">
          <tbody>
            {entries.map(([k, v]) => (
              <tr key={k}>
                <th>{k}</th>
                <td className="mono">{typeof v === 'object' ? JSON.stringify(v) : String(v)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  )
}

export type { MicroInfo }
