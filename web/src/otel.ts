import { ROOT_CONTEXT, SpanKind, SpanStatusCode } from '@opentelemetry/api'
import { OTLPTraceExporter } from '@opentelemetry/exporter-trace-otlp-http'
import { resourceFromAttributes } from '@opentelemetry/resources'
import { BatchSpanProcessor } from '@opentelemetry/sdk-trace-base'
import { WebTracerProvider } from '@opentelemetry/sdk-trace-web'
import { ATTR_SERVICE_NAME } from '@opentelemetry/semantic-conventions'

// Browser side of the trace: every call starts a CLIENT span here, its `traceparent` goes out in the NATS
// headers, and the span is exported over OTLP/HTTP to the collector (→ Jaeger) through the Vite proxy
// `/__otlp/spans` (same origin: no CORS, and ad blockers that drop `*/v1/traces` don't see it).

export const SERVICE_NAME = 'wgic-console'

export interface RpcSpanHandle {
  traceId: string
  traceparent: string
  endOk: () => void
  endErr: (err: unknown) => void
}

let provider: WebTracerProvider | undefined

function tracer() {
  provider ??= new WebTracerProvider({
    resource: resourceFromAttributes({ [ATTR_SERVICE_NAME]: SERVICE_NAME }),
    spanProcessors: [
      new BatchSpanProcessor(new OTLPTraceExporter({ url: new URL('/__otlp/spans', window.location.href).href }), {
        scheduledDelayMillis: 200,
      }),
    ],
  })
  return provider.getTracer(SERVICE_NAME)
}

/** Starts the root span of a new trace; end it with `endOk` / `endErr` (both export right away). */
export function startRpcSpan(name: string, attributes: Record<string, string>): RpcSpanHandle {
  const span = tracer().startSpan(name, { kind: SpanKind.CLIENT, attributes }, ROOT_CONTEXT)
  const { traceId, spanId, traceFlags } = span.spanContext()
  const end = () => {
    span.end()
    void provider?.forceFlush()
  }
  return {
    traceId,
    traceparent: `00-${traceId}-${spanId}-${traceFlags.toString(16).padStart(2, '0')}`,
    endOk: end,
    endErr: (err) => {
      span.setStatus({ code: SpanStatusCode.ERROR, message: err instanceof Error ? err.message : String(err) })
      end()
    },
  }
}
