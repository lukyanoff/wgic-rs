import type { MethodEntry } from './schema'

interface RequestPanelProps {
  methods: MethodEntry[]
  selectedIdx: number
  onSelectMethod: (idx: number) => void
  moduleName: string
  /** Running instances of this module (from NATS micro discovery). */
  instances: string[]
  instance: string
  onInstance: (instance: string) => void
  jsonInput: string
  onJsonInput: (json: string) => void
  timeoutSecs: string
  onTimeoutSecs: (secs: string) => void
  loading: boolean
  onSend: () => void
}

/** Left panel: method, target instance, timeout and the JSON request. */
export function RequestPanel(p: RequestPanelProps) {
  return (
    <section className="panel">
      <h2>1. Target & Method</h2>
      <div className="field-row">
        <label>RPC Method:</label>
        <select value={p.selectedIdx} onChange={(e) => p.onSelectMethod(Number(e.target.value))}>
          {p.methods.map((m, idx) => (
            <option key={m.label} value={idx}>
              {m.label}
            </option>
          ))}
        </select>
      </div>

      <div className="field-row">
        <label>Module / Instance (chips: running instances):</label>
        <div className="instance-selector">
          <code>{p.moduleName}/</code>
          <input value={p.instance} onChange={(e) => p.onInstance(e.target.value)} placeholder="instance_id" />
          {p.instances.map((name) => (
            <button
              key={name}
              type="button"
              className={name === p.instance ? 'chip active' : 'chip'}
              onClick={() => p.onInstance(name)}
            >
              {name}
            </button>
          ))}
        </div>
      </div>

      <div className="field-row timeout-row">
        <label>Timeout (s), enforced by the server too:</label>
        <input type="number" min={1} value={p.timeoutSecs} onChange={(e) => p.onTimeoutSecs(e.target.value)} />
      </div>

      <h3>Request Payload (JSON)</h3>
      <textarea rows={10} value={p.jsonInput} onChange={(e) => p.onJsonInput(e.target.value)} />

      <button className="send-btn" disabled={p.loading} onClick={p.onSend}>
        {p.loading ? 'Streaming RPC...' : `Invoke rpc.${p.moduleName}.${p.instance}...`}
      </button>
    </section>
  )
}
