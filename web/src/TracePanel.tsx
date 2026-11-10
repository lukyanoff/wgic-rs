export function TracePanel({ traceId }: { traceId: string }) {
  if (!traceId) return null;
  const jaegerUrl = `http://localhost:16686/trace/${traceId}`;

  return (
    <footer className="trace-panel" style={{ marginTop: "1rem", padding: "0.75rem", borderTop: "1px solid #333" }}>
      <span>Trace ID: <code>{traceId}</code></span>{" "}
      <a href={jaegerUrl} target="_blank" rel="noreferrer">
        Open in Jaeger
      </a>
    </footer>
  );
}
