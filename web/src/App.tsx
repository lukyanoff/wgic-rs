import { useState } from 'react'
import './App.css'
import type { InstanceRow } from './micro'
import { RequestPanel } from './RequestPanel'
import { ResultPanel } from './ResultPanel'
import type { MethodEntry } from './schema'
import { ServicesPanel } from './ServicesPanel'
import { buildTemplate } from './template'
import { TracePanel } from './TracePanel'
import { useMesh } from './useMesh'
import { useRpcCall } from './useRpcCall'

const DEFAULT_NATS_WS = 'ws://localhost:9222'

/** `visioner.v1.VisionerService` → `visioner` (the proto package's first part is the module name). */
function moduleFromService(typeName: string): string {
  return typeName.split('.')[0] || 'unknown'
}

interface ConsoleHeaderProps {
  natsUrl: string
  onNatsUrl: (url: string) => void
  onRefresh: () => void
}

function ConsoleHeader({ natsUrl, onNatsUrl, onRefresh }: ConsoleHeaderProps) {
  return (
    <header className="console-header">
      <h1>WGIC Omni Console (NATS Mesh)</h1>
      <div className="conn-bar">
        <label>NATS WS:</label>
        <input value={natsUrl} onChange={(e) => onNatsUrl(e.target.value)} />
        <button onClick={onRefresh}>Refresh</button>
      </div>
    </header>
  )
}

export function App({ methods }: { methods: MethodEntry[] }) {
  const [natsUrl, setNatsUrl] = useState(DEFAULT_NATS_WS)
  const [selectedIdx, setSelectedIdx] = useState(0)
  const [typedInstance, setTypedInstance] = useState('')
  const [jsonInput, setJsonInput] = useState(() => buildTemplate(methods[0].method.input))
  const [timeoutSecs, setTimeoutSecs] = useState('150')
  const mesh = useMesh(natsUrl)
  const call = useRpcCall()

  const entry = methods[selectedIdx]
  const moduleName = moduleFromService(entry.service.typeName)
  const instances = mesh.rows.filter((r) => r.module === moduleName).map((r) => r.instance)
  // Until something is typed or picked, target the first running instance.
  const instance = typedInstance || instances[0] || 'default'

  const selectMethod = (idx: number) => {
    setSelectedIdx(idx)
    setTypedInstance('')
    setJsonInput(buildTemplate(methods[idx].method.input))
    call.reset()
  }

  const pickInstance = (row: InstanceRow) => {
    const idx = methods.findIndex((m) => moduleFromService(m.service.typeName) === row.module)
    if (idx >= 0 && idx !== selectedIdx) selectMethod(idx)
    setTypedInstance(row.instance)
  }

  const send = async () => {
    await call.send({
      natsUrl,
      module: moduleName,
      instance,
      entry,
      jsonInput,
      timeoutMs: Number(timeoutSecs) * 1000 || 150_000,
    })
    await mesh.refresh()
  }

  return (
    <div className="console-container">
      <ConsoleHeader natsUrl={natsUrl} onNatsUrl={setNatsUrl} onRefresh={() => void mesh.refresh()} />

      <div className="console-grid">
        <RequestPanel
          methods={methods}
          selectedIdx={selectedIdx}
          onSelectMethod={selectMethod}
          moduleName={moduleName}
          instances={instances}
          instance={instance}
          onInstance={setTypedInstance}
          jsonInput={jsonInput}
          onJsonInput={setJsonInput}
          timeoutSecs={timeoutSecs}
          onTimeoutSecs={setTimeoutSecs}
          loading={call.view.loading}
          onSend={() => void send()}
        />
        <ResultPanel call={call.view} kvStates={mesh.kvStates} />
      </div>

      {call.view.traceId && <TracePanel key={call.view.traceId} traceId={call.view.traceId} />}
      <ServicesPanel rows={mesh.rows} onPick={pickInstance} />
    </div>
  )
}
