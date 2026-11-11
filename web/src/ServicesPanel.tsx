import type { InstanceRow } from './micro'

interface ServicesPanelProps {
  rows: InstanceRow[]
  onPick: (row: InstanceRow) => void
}

/** `nats micro ls` + `stats` as a table; clicking a row targets that instance. */
export function ServicesPanel({ rows, onPick }: ServicesPanelProps) {
  return (
    <section className="panel services-panel">
      <h2>4. Running services (NATS micro `$SRV.INFO` / `$SRV.STATS`)</h2>
      {rows.length === 0 ? (
        <p className="empty-hint">No services answer. Start them with scripts/run-all.sh.</p>
      ) : (
        <table className="services-table">
          <thead>
            <tr>
              <th>Module / instance</th>
              <th>Replicas</th>
              <th>Calls</th>
              <th>Errors</th>
              <th>Avg</th>
              <th>Last error</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={`${r.module}/${r.instance}`} onClick={() => onPick(r)} title="Use this instance">
                <td>
                  <code>
                    {r.module}/{r.instance}
                  </code>
                </td>
                <td>{r.replicas}</td>
                <td>{r.requests}</td>
                <td className={r.errors ? 'bad' : ''}>{r.errors}</td>
                <td>{r.avgMs} ms</td>
                <td className="last-error">{r.lastError}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  )
}
