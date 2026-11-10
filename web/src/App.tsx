import { useEffect, useState } from 'react'
import './App.css'
import { callNatsRpc, fetchAllInstanceStates } from './rpcClient'
import type { MethodEntry } from './schema'
import { buildTemplate } from './template'
import { ResultPanel } from './ResultPanel'
import { TracePanel } from './TracePanel'

const DEFAULT_NATS_WS = 'ws://localhost:9222'

const PRESET_INSTANCES: Record<string, string[]> = {
  'sensor.v1.SensorService': ['fake', 'screen', 'cam'],
  'llm_caller.v1.LlmCallerService': ['agy', 'claude', 'openai'],
  'visioner.v1.VisionerService': ['default'],
}

function moduleFromService(typeName: string): string {
  return typeName.split('.')[0] || 'unknown'
}

export function App({ methods }: { methods: MethodEntry[] }) {
  const [natsUrl, setNatsUrl] = useState(DEFAULT_NATS_WS)
  const [selectedIdx, setSelectedIdx] = useState(0)
  const entry = methods[selectedIdx]
  const moduleName = moduleFromService(entry.service.typeName)

  const presets = PRESET_INSTANCES[entry.service.typeName] ?? ['default']
  const [instance, setInstance] = useState(presets[0])
  const [jsonInput, setJsonInput] = useState(() => buildTemplate(entry.method.input))

  const [progressSteps, setProgressSteps] = useState<string[]>([])
  const [responseJson, setResponseJson] = useState<string>('')
  const [errorText, setErrorText] = useState<string>('')
  const [traceId, setTraceId] = useState<string>('')
  const [durationMs, setDurationMs] = useState<number | null>(null)
  const [loading, setLoading] = useState(false)
  const [kvStates, setKvStates] = useState<Record<string, unknown>>({})

  useEffect(() => {
    const nextPresets = PRESET_INSTANCES[entry.service.typeName] ?? ['default']
    setInstance(nextPresets[0])
    setJsonInput(buildTemplate(entry.method.input))
    setProgressSteps([])
    setResponseJson('')
    setErrorText('')
  }, [selectedIdx, entry])

  const refreshKvState = async () => {
    const states = await fetchAllInstanceStates(natsUrl)
    setKvStates(states)
  }

  useEffect(() => {
    refreshKvState()
  }, [natsUrl])

  const handleSend = async () => {
    setLoading(true)
    setProgressSteps([])
    setResponseJson('')
    setErrorText('')
    setDurationMs(null)

    try {
      const res = await callNatsRpc({
        natsUrl,
        module: moduleName,
        instance,
        entry,
        jsonInput,
        onProgress: (step) => setProgressSteps((prev) => [...prev, step]),
      })
      setResponseJson(res.responseJson)
      setTraceId(res.traceId)
      setDurationMs(res.durationMs)
      await refreshKvState()
    } catch (err) {
      setErrorText(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="console-container">
      <header className="console-header">
        <h1>WGIC Omni Console (NATS Mesh)</h1>
        <div className="conn-bar">
          <label>NATS WS:</label>
          <input value={natsUrl} onChange={(e) => setNatsUrl(e.target.value)} />
          <button onClick={refreshKvState}>Refresh KV State</button>
        </div>
      </header>

      <div className="console-grid">
        <section className="panel">
          <h2>1. Target & Method</h2>
          <div className="field-row">
            <label>RPC Method:</label>
            <select value={selectedIdx} onChange={(e) => setSelectedIdx(Number(e.target.value))}>
              {methods.map((m, idx) => (
                <option key={m.label} value={idx}>
                  {m.label}
                </option>
              ))}
            </select>
          </div>

          <div className="field-row">
            <label>Module / Instance:</label>
            <div className="instance-selector">
              <code>{moduleName}/</code>
              <input value={instance} onChange={(e) => setInstance(e.target.value)} placeholder="instance_id" />
              {presets.map((p) => (
                <button
                  key={p}
                  type="button"
                  className={p === instance ? 'chip active' : 'chip'}
                  onClick={() => setInstance(p)}
                >
                  {p}
                </button>
              ))}
            </div>
          </div>

          <h3>Request Payload (JSON)</h3>
          <textarea rows={10} value={jsonInput} onChange={(e) => setJsonInput(e.target.value)} />

          <button className="send-btn" disabled={loading} onClick={handleSend}>
            {loading ? 'Streaming RPC...' : `Invoke rpc.${moduleName}.${instance}...`}
          </button>
        </section>

        <ResultPanel
          loading={loading}
          durationMs={durationMs}
          progressSteps={progressSteps}
          errorText={errorText}
          responseJson={responseJson}
          kvStates={kvStates}
        />
      </div>

      {traceId && <TracePanel traceId={traceId} />}
    </div>
  )
}
