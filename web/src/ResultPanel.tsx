interface ResultPanelProps {
  loading: boolean
  durationMs: number | null
  progressSteps: string[]
  errorText: string
  responseJson: string
  kvStates: Record<string, unknown>
}

/** Right-hand panel: live progress frames, the terminal response or error, and the KV instance state. */
export function ResultPanel(props: ResultPanelProps) {
  const { loading, durationMs, progressSteps, errorText, responseJson, kvStates } = props
  const idle = !loading && !errorText && !responseJson && progressSteps.length === 0

  return (
    <section className="panel">
      <h2>2. Progress & Response {durationMs !== null && `(${durationMs} ms)`}</h2>
      {idle && <p className="empty-hint">Send a request to see live progress and the response here.</p>}
      {progressSteps.length > 0 && (
        <div className="progress-box">
          <strong>Live Progress Stream:</strong>
          <ul>
            {progressSteps.map((s, i) => (
              <li key={i}>{s}</li>
            ))}
          </ul>
        </div>
      )}
      {errorText && <pre className="error-box">{errorText}</pre>}
      {responseJson && <pre className="response-box">{responseJson}</pre>}

      <h2>3. Instance CAS State (NATS KV `wgic_instance_state`)</h2>
      <pre className="kv-box">
        {Object.keys(kvStates).length === 0
          ? 'No state entries yet. Invoke an RPC method to see CAS updates.'
          : JSON.stringify(kvStates, null, 2)}
      </pre>
    </section>
  )
}
