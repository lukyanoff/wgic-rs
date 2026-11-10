import {
  ROOT_CONTEXT,
  SpanKind,
  SpanStatusCode,
  defaultTextMapGetter,
  defaultTextMapSetter,
  trace,
  type Attributes,
  type Context,
  type Span,
} from '@opentelemetry/api'
import {
  CompositePropagator,
  ExportResultCode,
  W3CBaggagePropagator,
  W3CTraceContextPropagator,
  hrTimeToMicroseconds,
  type ExportResult,
} from '@opentelemetry/core'
import { OTLPTraceExporter } from '@opentelemetry/exporter-trace-otlp-http'
import { resourceFromAttributes } from '@opentelemetry/resources'
import {
  BatchSpanProcessor,
  SimpleSpanProcessor,
  type ReadableSpan,
  type SpanExporter,
  type SpanProcessor,
} from '@opentelemetry/sdk-trace-base'
import { WebTracerProvider } from '@opentelemetry/sdk-trace-web'
import { ATTR_SERVICE_NAME, ATTR_SERVICE_VERSION } from '@opentelemetry/semantic-conventions'

// Browser side of the playground's OpenTelemetry: every call starts a trace here,
// W3C `traceparent` goes out in HTTP and NATS headers, spans are exported over
// OTLP/HTTP to the collector and also kept in memory for the Trace panel.

export const SERVICE_NAME = 'connect-playground'

// ------------------------------------------------------------------ settings

export interface TelemetrySettings {
  /** Export browser spans to the collector (the trace is always started). */
  exportEnabled: boolean
  /** OTLP/HTTP traces endpoint; relative URLs resolve against the page (the dev proxy). */
  collectorUrl: string
  /** Jaeger UI base URL, for "Open in Jaeger" links. Queries go via the /__jaeger dev proxy. */
  jaegerUrl: string
}

export const DEFAULT_SETTINGS: TelemetrySettings = {
  exportEnabled: true,
  collectorUrl: '/__otlp/spans',
  jaegerUrl: 'http://localhost:16686',
}

const KEY = 'playground.telemetry'

export function loadSettings(): TelemetrySettings {
  try {
    const s = localStorage.getItem(KEY)
    if (s) {
      const saved = { ...DEFAULT_SETTINGS, ...(JSON.parse(s) as Partial<TelemetrySettings>) }
      // The old default went straight to the collector; move it to the dev proxy.
      if (saved.collectorUrl === 'http://localhost:4318/v1/traces') saved.collectorUrl = DEFAULT_SETTINGS.collectorUrl
      return saved
    }
  } catch {
    // ignore
  }
  return DEFAULT_SETTINGS
}

export function saveSettings(s: TelemetrySettings) {
  try {
    localStorage.setItem(KEY, JSON.stringify(s))
  } catch {
    // ignore
  }
}

// --------------------------------------------------------------- span model

/** One span as the Trace panel shows it, from the browser or from Jaeger. */
export interface UiSpan {
  traceId: string
  spanId: string
  parentId?: string
  service: string
  name: string
  kind: string
  startUs: number
  durationUs: number
  attributes: Record<string, unknown>
  events: UiEvent[]
  status: { code: 'unset' | 'ok' | 'error'; message?: string }
  source: 'browser' | 'jaeger'
}

/** A span event; for Rust services these are the correlated log lines. */
export interface UiEvent {
  timeUs: number
  name: string
  attributes: Record<string, unknown>
}

const KIND_NAMES: Record<number, string> = {
  [SpanKind.INTERNAL]: 'internal',
  [SpanKind.SERVER]: 'server',
  [SpanKind.CLIENT]: 'client',
  [SpanKind.PRODUCER]: 'producer',
  [SpanKind.CONSUMER]: 'consumer',
}

function toUiSpan(s: ReadableSpan): UiSpan {
  const sc = s.spanContext()
  return {
    traceId: sc.traceId,
    spanId: sc.spanId,
    parentId: s.parentSpanContext?.spanId,
    service: String(s.resource.attributes[ATTR_SERVICE_NAME] ?? SERVICE_NAME),
    name: s.name,
    kind: KIND_NAMES[s.kind] ?? 'internal',
    startUs: hrTimeToMicroseconds(s.startTime),
    durationUs: hrTimeToMicroseconds(s.duration),
    attributes: { ...s.attributes },
    events: s.events.map((e) => ({
      timeUs: hrTimeToMicroseconds(e.time),
      name: e.name,
      attributes: { ...(e.attributes ?? {}) },
    })),
    status: {
      code: s.status.code === SpanStatusCode.ERROR ? 'error' : s.status.code === SpanStatusCode.OK ? 'ok' : 'unset',
      message: s.status.message,
    },
    source: 'browser',
  }
}

// ----------------------------------------------------- in-memory span store

const MAX_TRACES = 100
const local = new Map<string, UiSpan[]>()
const listeners = new Set<() => void>()
let version = 0

export function subscribeSpans(fn: () => void): () => void {
  listeners.add(fn)
  return () => listeners.delete(fn)
}

/** Changes whenever browser spans are recorded (for useSyncExternalStore). */
export function spansVersion(): number {
  return version
}

const NONE: UiSpan[] = []

export function localSpans(traceId: string): UiSpan[] {
  return local.get(traceId) ?? NONE
}

class UiExporter implements SpanExporter {
  export(spans: ReadableSpan[], done: (r: ExportResult) => void) {
    for (const s of spans) {
      const u = toUiSpan(s)
      const list = local.get(u.traceId) ?? []
      local.delete(u.traceId) // re-insert: Map keeps insertion order, newest last
      local.set(u.traceId, [...list, u])
    }
    while (local.size > MAX_TRACES) local.delete(local.keys().next().value as string)
    version++
    listeners.forEach((fn) => fn())
    done({ code: ExportResultCode.SUCCESS })
  }
  shutdown() {
    return Promise.resolve()
  }
}

// ------------------------------------------------------------- export status

export interface ExportStatus {
  state: 'off' | 'idle' | 'ok' | 'error'
  at?: number
  error?: string
}

let exportStatus: ExportStatus = { state: 'off' }
const statusListeners = new Set<() => void>()

export function getExportStatus(): ExportStatus {
  return exportStatus
}

export function subscribeExportStatus(fn: () => void): () => void {
  statusListeners.add(fn)
  return () => statusListeners.delete(fn)
}

function setExportStatus(s: ExportStatus) {
  exportStatus = s
  statusListeners.forEach((fn) => fn())
}

/** Wraps the OTLP exporter to surface failures (collector down, CORS) in the UI. */
class StatusExporter implements SpanExporter {
  private inner: SpanExporter
  constructor(inner: SpanExporter) {
    this.inner = inner
  }
  export(spans: ReadableSpan[], done: (r: ExportResult) => void) {
    this.inner.export(spans, (r) => {
      setExportStatus(
        r.code === ExportResultCode.SUCCESS
          ? { state: 'ok', at: Date.now() }
          : {
              state: 'error',
              at: Date.now(),
              error: `${r.error?.message ?? 'export failed'} (${settings.collectorUrl}${
                settings.collectorUrl.startsWith('/') ? ' via the Vite dev proxy: is the collector up?' : ''
              })`,
            },
      )
      done(r)
    })
  }
  shutdown() {
    return this.inner.shutdown()
  }
  forceFlush() {
    return this.inner.forceFlush?.() ?? Promise.resolve()
  }
}

// ------------------------------------------------------------------ provider

let provider: WebTracerProvider | undefined
let settings: TelemetrySettings = DEFAULT_SETTINGS

const propagator = new CompositePropagator({
  propagators: [new W3CTraceContextPropagator(), new W3CBaggagePropagator()],
})

/** (Re)build the tracer provider from settings. Safe to call repeatedly. */
export function configureTelemetry(s: TelemetrySettings) {
  settings = s
  const old = provider
  const spanProcessors: SpanProcessor[] = [new SimpleSpanProcessor(new UiExporter())]
  if (s.exportEnabled && s.collectorUrl.trim()) {
    spanProcessors.push(
      new BatchSpanProcessor(new StatusExporter(new OTLPTraceExporter({ url: absoluteUrl(s.collectorUrl) })), {
        scheduledDelayMillis: 200,
      }),
    )
    setExportStatus({ state: 'idle' })
  } else {
    setExportStatus({ state: 'off' })
  }
  provider = new WebTracerProvider({
    resource: resourceFromAttributes({
      [ATTR_SERVICE_NAME]: SERVICE_NAME,
      [ATTR_SERVICE_VERSION]: '0.0.0',
      'browser.user_agent': navigator.userAgent,
    }),
    spanProcessors,
  })
  void old?.shutdown()
}

/** The OTLP exporter needs an absolute URL; resolve `/__otlp/spans` against the page. */
function absoluteUrl(url: string): string {
  return new URL(url.trim(), window.location.href).href
}

export function currentSettings(): TelemetrySettings {
  return settings
}

function tracer() {
  if (!provider) configureTelemetry(loadSettings())
  return provider!.getTracer(SERVICE_NAME)
}

/** Send buffered spans now (e.g. right after a call, so Jaeger has them sooner). */
export function flushSpans(): Promise<void> {
  return provider?.forceFlush() ?? Promise.resolve()
}

// --------------------------------------------------------- span helpers

export interface Started {
  span: Span
  ctx: Context
  traceId: string
}

/** Start a span; without `parent` it is the root of a new trace. */
export function startSpan(name: string, kind: SpanKind, attributes: Attributes, parent?: Context): Started {
  const base = parent ?? ROOT_CONTEXT
  const span = tracer().startSpan(name, { kind, attributes }, base)
  return { span, ctx: trace.setSpan(base, span), traceId: span.spanContext().traceId }
}

/** End a span, marking it failed when `error` is set. */
export function endSpan(span: Span, error?: string, attributes?: Attributes) {
  if (attributes) span.setAttributes(attributes)
  if (error) span.setStatus({ code: SpanStatusCode.ERROR, message: error })
  span.end()
}

export const PROPAGATION_HEADERS = ['traceparent', 'tracestate', 'baggage']

export function isPropagationHeader(name: string): boolean {
  return PROPAGATION_HEADERS.includes(name.toLowerCase())
}

/** Write `traceparent` / `tracestate` / `baggage` for `ctx` into a plain object. */
export function traceHeaders(ctx: Context): Record<string, string> {
  const out: Record<string, string> = {}
  propagator.inject(ctx, out, defaultTextMapSetter)
  return out
}

/**
 * Parent context from header pairs (case-insensitive), e.g. a `traceparent` the user typed
 * to join an existing trace. Returns undefined when there is none.
 */
export function extractContext(headers: Iterable<[string, string]>): Context | undefined {
  const carrier: Record<string, string> = {}
  for (const [k, v] of headers) if (isPropagationHeader(k)) carrier[k.toLowerCase()] = v
  if (!carrier.traceparent) return undefined
  const ctx = propagator.extract(ROOT_CONTEXT, carrier, defaultTextMapGetter)
  return trace.getSpanContext(ctx) ? ctx : undefined
}

/** `00-<trace>-<span>-<flags>` → trace id. */
export function traceIdFromTraceparent(tp?: string | null): string | undefined {
  const m = tp?.trim().match(/^[0-9a-f]{2}-([0-9a-f]{32})-[0-9a-f]{16}-[0-9a-f]{2}$/i)
  return m ? m[1].toLowerCase() : undefined
}

export function jaegerTraceUrl(traceId: string): string {
  return `${settings.jaegerUrl.replace(/\/+$/, '')}/trace/${traceId}`
}

export { SpanKind }
