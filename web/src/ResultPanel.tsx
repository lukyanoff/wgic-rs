import type { CallView, ProgressStep } from './useRpcCall'

interface ResultPanelProps {
  call: CallView
  kvStates: Record<string, unknown>
}

/** Right-hand panel: live progress steps, the answer as it streams in, the response or error, and the KV state. */
export function ResultPanel({ call, kvStates }: ResultPanelProps) {
  const { loading, durationMs, steps, errorText, responseJson } = call
  const idle = !loading && !errorText && !responseJson && steps.length === 0

  return (
    <section className="panel">
      <h2>2. Progress & Response {durationMs !== null && `(${durationMs} ms)`}</h2>
      {idle && <p className="empty-hint">Send a request to see live progress and the response here.</p>}
      <ProgressList steps={steps} />
      <Outcome call={call} />

      <h2>3. Instance CAS State (NATS KV `wgic_instance_state`)</h2>
      <pre className="kv-box">
        {Object.keys(kvStates).length === 0
          ? 'No state entries yet. Invoke an RPC method to see CAS updates.'
          : JSON.stringify(kvStates, null, 2)}
      </pre>
    </section>
  )
}

/** Steps with their time since the call started, e.g. `+2.1s [screen] captured 170719 bytes`. */
function ProgressList({ steps }: { steps: ProgressStep[] }) {
  if (steps.length === 0) return null
  return (
    <div className="progress-box">
      <strong>Live progress:</strong>
      <ul>
        {steps.map((s, i) => (
          <li key={i}>
            <span className="step-at">+{(s.atMs / 1000).toFixed(1)}s</span> {s.text}
          </li>
        ))}
      </ul>
    </div>
  )
}

/** The answer while it streams in, then the final response (or the error). */
function Outcome({ call }: { call: CallView }) {
  const { loading, answer, errorText, responseJson } = call
  return (
    <>
      {answer && !responseJson && <pre className={loading ? 'answer-box streaming' : 'answer-box'}>{answer}</pre>}
      {errorText && <pre className="error-box">{errorText}</pre>}
      {responseJson && <pre className="response-box">{responseJson}</pre>}
    </>
  )
}
