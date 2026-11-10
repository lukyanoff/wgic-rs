import { useEffect, useMemo, useRef, useState } from 'react'
import type { DescMethod, JsonValue } from '@bufbuild/protobuf'
import './App.css'
import { callMethod, isBrowserSupported, type CallResult, type Protocol } from './call'
import { loadSchema, methodComment, methodPath, type Schema } from './schema'
import { loadServers, resetServers, saveServers, type ServerEntry } from './servers'
import { messageTemplate } from './template'

interface HistoryEntry {
  at: number
  server: ServerEntry
  methodKey: string
  protocol: Protocol
  request: string
  headers: string
  ok: boolean
  status: string
}

const HISTORY_KEY = 'playground.history'
const HISTORY_MAX = 20

function readHistory(): HistoryEntry[] {
  try {
    return JSON.parse(localStorage.getItem(HISTORY_KEY) ?? '[]') as HistoryEntry[]
  } catch {
    return []
  }
}

function writeHistory(h: HistoryEntry[]) {
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(h))
  } catch {
    // ignore
  }
}

const keyOf = (m: DescMethod) => `${m.parent.typeName}/${m.name}`
const pretty = (v: JsonValue) => JSON.stringify(v, null, 2)

function parseHeaders(text: string): Record<string, string> {
  const out: Record<string, string> = {}
  for (const line of text.split('\n')) {
    const i = line.indexOf(':')
    if (i <= 0) continue
    out[line.slice(0, i).trim()] = line.slice(i + 1).trim()
  }
  return out
}

export default function App() {
  const [schema, setSchema] = useState<Schema>()
  const [schemaError, setSchemaError] = useState<string>()
  const [servers, setServers] = useState<ServerEntry[]>([])
  const [serverIdx, setServerIdx] = useState(0)
  const [editingServers, setEditingServers] = useState(false)
  const [filter, setFilter] = useState('')
  const [chosenMethodKey, setMethodKey] = useState<string>()
  const [requests, setRequests] = useState<Record<string, string>>({})
  const [headers, setHeaders] = useState('')
  const [protocol, setProtocol] = useState<Protocol>('connect-json')
  const [timeoutMs, setTimeoutMs] = useState('10000')
  const [running, setRunning] = useState(false)
  const [messages, setMessages] = useState<JsonValue[]>([])
  const [result, setResult] = useState<CallResult>()
  const [inputError, setInputError] = useState<string>()
  const [history, setHistory] = useState<HistoryEntry[]>(readHistory)
  const abortRef = useRef<AbortController | null>(null)

  useEffect(() => {
    loadSchema()
      .then(setSchema)
      .catch((e) => setSchemaError(String(e)))
    loadServers().then(setServers)
  }, [])

  const methods = useMemo(() => {
    const map = new Map<string, DescMethod>()
    for (const s of schema?.services ?? []) for (const m of s.methods) map.set(keyOf(m), m)
    return map
  }, [schema])

  const methodKey = chosenMethodKey ?? methods.keys().next().value
  const method = methodKey ? methods.get(methodKey) : undefined
  const server = servers[serverIdx]
  const requestText =
    method && (requests[keyOf(method)] ?? pretty(messageTemplate(method.input)))

  const updateServers = (next: ServerEntry[]) => {
    setServers(next)
    saveServers(next)
  }

  const setRequestText = (text: string) => {
    if (method) setRequests((r) => ({ ...r, [keyOf(method)]: text }))
  }

  async function send() {
    if (!schema || !method || !server || requestText === undefined) return
    let request: JsonValue
    try {
      request = JSON.parse(requestText || '{}') as JsonValue
    } catch (e) {
      setInputError(`Request is not valid JSON: ${(e as Error).message}`)
      return
    }
    setInputError(undefined)
    setMessages([])
    setResult(undefined)
    setRunning(true)
    const ctrl = new AbortController()
    abortRef.current = ctrl
    try {
      const res = await callMethod({
        server,
        protocol,
        method,
        registry: schema.registry,
        request,
        headers: parseHeaders(headers),
        timeoutMs: Number(timeoutMs) > 0 ? Number(timeoutMs) : undefined,
        signal: ctrl.signal,
        onMessage: (m) => setMessages((prev) => [...prev, m]),
      })
      setResult(res)
      const entry: HistoryEntry = {
        at: Date.now(),
        server,
        methodKey: keyOf(method),
        protocol,
        request: requestText,
        headers,
        ok: res.ok,
        status: res.error?.code ?? 'ok',
      }
      setHistory((h) => {
        const next = [entry, ...h].slice(0, HISTORY_MAX)
        writeHistory(next)
        return next
      })
    } catch (e) {
      // fromJson failures (wrong field names or types) land here.
      setInputError(String(e))
    } finally {
      setRunning(false)
      abortRef.current = null
    }
  }

  function restore(h: HistoryEntry) {
    const idx = servers.findIndex((s) => s.url === h.server.url && s.mode === h.server.mode)
    if (idx >= 0) setServerIdx(idx)
    else {
      updateServers([...servers, h.server])
      setServerIdx(servers.length)
    }
    setMethodKey(h.methodKey)
    setProtocol(h.protocol)
    setHeaders(h.headers)
    setRequests((r) => ({ ...r, [h.methodKey]: h.request }))
  }

  const q = filter.trim().toLowerCase()

  return (
    <div className="app">
      <aside className="sidebar">
        <header className="brand">Connect Playground</header>

        <section className="panel">
          <div className="panel-title">
            <span>Servers</span>
            <button className="link" onClick={() => setEditingServers((v) => !v)}>
              {editingServers ? 'Done' : 'Edit'}
            </button>
          </div>
          {!editingServers && (
            <select
              value={serverIdx}
              onChange={(e) => setServerIdx(Number(e.target.value))}
              aria-label="Server"
            >
              {servers.map((s, i) => (
                <option key={i} value={i}>
                  {s.name}: {s.url} ({s.mode})
                </option>
              ))}
            </select>
          )}
          {editingServers && (
            <div className="server-editor">
              {servers.map((s, i) => (
                <div className="server-row" key={i}>
                  <input
                    value={s.name}
                    placeholder="name"
                    onChange={(e) =>
                      updateServers(servers.map((x, j) => (j === i ? { ...x, name: e.target.value } : x)))
                    }
                  />
                  <input
                    value={s.url}
                    placeholder="http://host:port"
                    onChange={(e) =>
                      updateServers(servers.map((x, j) => (j === i ? { ...x, url: e.target.value } : x)))
                    }
                  />
                  <select
                    value={s.mode}
                    onChange={(e) =>
                      updateServers(
                        servers.map((x, j) =>
                          j === i ? { ...x, mode: e.target.value as ServerEntry['mode'] } : x,
                        ),
                      )
                    }
                  >
                    <option value="proxy">proxy</option>
                    <option value="direct">direct</option>
                  </select>
                  <button
                    className="icon"
                    title="Remove"
                    onClick={() => {
                      updateServers(servers.filter((_, j) => j !== i))
                      setServerIdx(0)
                    }}
                  >
                    ×
                  </button>
                </div>
              ))}
              <div className="row">
                <button
                  onClick={() =>
                    updateServers([
                      ...servers,
                      { name: `server ${servers.length + 1}`, url: 'http://127.0.0.1:8080', mode: 'proxy' },
                    ])
                  }
                >
                  + Add server
                </button>
                <button
                  className="link"
                  onClick={() => {
                    resetServers()
                    loadServers().then((s) => {
                      setServers(s)
                      setServerIdx(0)
                    })
                  }}
                >
                  Reset to servers.json
                </button>
              </div>
            </div>
          )}
        </section>

        <section className="panel grow">
          <div className="panel-title">
            <span>Methods</span>
          </div>
          <input
            type="search"
            placeholder="Filter services and methods"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
          {schemaError && <div className="error">{schemaError}</div>}
          <nav className="tree">
            {schema?.services.map((s) => {
              const ms = s.methods.filter(
                (m) => !q || s.typeName.toLowerCase().includes(q) || m.name.toLowerCase().includes(q),
              )
              if (ms.length === 0) return null
              return (
                <div key={s.typeName} className="service">
                  <div className="service-name">{s.typeName}</div>
                  {ms.map((m) => (
                    <button
                      key={m.name}
                      className={`method ${methodKey === keyOf(m) ? 'active' : ''}`}
                      onClick={() => setMethodKey(keyOf(m))}
                      title={methodPath(m)}
                    >
                      <span>{m.name}</span>
                      {m.methodKind !== 'unary' && (
                        <span className={`badge ${isBrowserSupported(m) ? '' : 'muted'}`}>
                          {m.methodKind.replace('_', ' ')}
                        </span>
                      )}
                    </button>
                  ))}
                </div>
              )
            })}
          </nav>
        </section>
      </aside>

      <main className="main">
        {!method && <div className="empty">{schema ? 'Select a method' : 'Loading schema…'}</div>}
        {method && (
          <>
            <div className="method-header">
              <h1>
                <span className="dim">{method.parent.typeName}/</span>
                {method.name}
              </h1>
              <code className="dim">
                {method.input.typeName} → {method.output.typeName}
              </code>
              {methodComment(method) && <p className="doc">{methodComment(method)}</p>}
            </div>

            <div className="controls">
              <label>
                Protocol
                <select value={protocol} onChange={(e) => setProtocol(e.target.value as Protocol)}>
                  <option value="connect-json">Connect (JSON)</option>
                  <option value="connect-binary">Connect (binary)</option>
                  <option value="grpc-web">gRPC-web</option>
                </select>
              </label>
              <label>
                Timeout ms
                <input
                  type="number"
                  min={0}
                  value={timeoutMs}
                  onChange={(e) => setTimeoutMs(e.target.value)}
                />
              </label>
              <div className="spacer" />
              {running ? (
                <button onClick={() => abortRef.current?.abort()}>Cancel</button>
              ) : (
                <button
                  className="primary"
                  onClick={send}
                  disabled={!server || !isBrowserSupported(method)}
                  title="Ctrl+Enter"
                >
                  Send
                </button>
              )}
            </div>
            {!isBrowserSupported(method) && (
              <div className="error">
                {method.methodKind.replace('_', ' ')} methods can't be called from a browser. Use{' '}
                <code>buf curl</code> instead.
              </div>
            )}

            <div className="panes">
              <section className="pane">
                <div className="pane-title">
                  <span>Request</span>
                  <button className="link" onClick={() => setRequestText(pretty(messageTemplate(method.input)))}>
                    Reset to template
                  </button>
                </div>
                <textarea
                  className="code"
                  spellCheck={false}
                  value={requestText}
                  onChange={(e) => setRequestText(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) send()
                  }}
                />
                <div className="pane-title">
                  <span>Headers</span>
                  <span className="dim small">one per line, key: value</span>
                </div>
                <textarea
                  className="code headers"
                  spellCheck={false}
                  placeholder="authorization: Bearer …"
                  value={headers}
                  onChange={(e) => setHeaders(e.target.value)}
                />
                {inputError && <div className="error">{inputError}</div>}
              </section>

              <section className="pane">
                <div className="pane-title">
                  <span>Response</span>
                  {result && (
                    <span className={`status ${result.ok ? 'ok' : 'bad'}`}>
                      {result.ok ? 'ok' : result.error?.code} · {result.durationMs.toFixed(0)} ms
                    </span>
                  )}
                  {running && <span className="dim small">waiting…</span>}
                </div>
                <pre className="code output">
                  {result?.error
                    ? pretty({
                        code: result.error.code,
                        message: result.error.message,
                        ...(result.error.details.length ? { details: result.error.details } : {}),
                      })
                    : messages.length === 1 && method.methodKind === 'unary'
                      ? pretty(messages[0])
                      : messages.map((m) => pretty(m)).join('\n')}
                </pre>
                {result && (result.header.length > 0 || result.trailer.length > 0) && (
                  <details className="meta">
                    <summary>Headers &amp; trailers</summary>
                    <pre className="code">
                      {result.header.map(([k, v]) => `${k}: ${v}`).join('\n')}
                      {result.trailer.length > 0 &&
                        '\n\n# trailers\n' + result.trailer.map(([k, v]) => `${k}: ${v}`).join('\n')}
                    </pre>
                  </details>
                )}
              </section>
            </div>

            {history.length > 0 && (
              <section className="history">
                <div className="pane-title">
                  <span>History</span>
                  <button
                    className="link"
                    onClick={() => {
                      setHistory([])
                      writeHistory([])
                    }}
                  >
                    Clear
                  </button>
                </div>
                <ul>
                  {history.map((h) => (
                    <li key={h.at}>
                      <button className="history-item" onClick={() => restore(h)}>
                        <span className={`dot ${h.ok ? 'ok' : 'bad'}`} />
                        <span className="mono">{h.methodKey}</span>
                        <span className="dim">{h.server.name}</span>
                        <span className="dim">{h.protocol}</span>
                        <span className="dim">{h.status}</span>
                        <span className="dim right">{new Date(h.at).toLocaleTimeString()}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </>
        )}
      </main>
    </div>
  )
}
