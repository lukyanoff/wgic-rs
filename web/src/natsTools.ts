import {
  headers as natsHeaders,
  NoRespondersError,
  RequestError,
  TimeoutError,
  type Msg,
  type MsgHdrs,
  type NatsConnection,
} from '@nats-io/nats-core'
import type { Context } from '@opentelemetry/api'
import { getConnection } from './nats'
import {
  endSpan,
  extractContext,
  flushSpans,
  isPropagationHeader,
  SpanKind,
  startSpan,
  traceHeaders,
  traceIdFromTraceparent,
} from './otel'

// Browser equivalents of `nats micro ls|info|ping|stats|req` and `nats req|pub|sub`.
// Discovery uses the micro protocol subjects: $SRV.<PING|INFO|STATS>[.<name>[.<id>]].

export interface MicroEndpointInfo {
  name: string
  subject: string
  queue_group?: string
  metadata?: Record<string, string> | null
}

export interface MicroInfo {
  type?: string
  name: string
  id: string
  version: string
  description?: string
  metadata?: Record<string, string> | null
  endpoints: MicroEndpointInfo[]
}

export interface MicroPing {
  type?: string
  name: string
  id: string
  version: string
  metadata?: Record<string, string> | null
}

export interface MicroEndpointStats {
  name: string
  subject: string
  queue_group?: string
  num_requests: number
  num_errors: number
  last_error?: string
  /** nanoseconds */
  processing_time: number
  /** nanoseconds */
  average_processing_time: number
  data?: unknown
}

export interface MicroStats {
  type?: string
  name: string
  id: string
  version: string
  started?: string
  endpoints: MicroEndpointStats[]
}

export interface Timed<T> {
  value: T
  /** ms from send to this reply's arrival */
  rttMs: number
}

export type HeaderList = [string, string][]

export function isNoResponders(e: unknown): boolean {
  return (
    e instanceof NoRespondersError ||
    (e instanceof RequestError && e.isNoResponders()) ||
    (e instanceof Error && e.cause instanceof NoRespondersError)
  )
}

function isTimeout(e: unknown): boolean {
  return e instanceof TimeoutError || (e instanceof Error && e.cause instanceof TimeoutError)
}

export function describeError(e: unknown, subject: string): string {
  if (isNoResponders(e)) return `no responders for ${subject}`
  if (isTimeout(e)) return `request to ${subject} timed out`
  return String(e)
}

/** Scatter-gather: send one request, collect every reply until `waitMs` passes. */
export async function collect<T>(nc: NatsConnection, subject: string, waitMs = 750): Promise<Timed<T>[]> {
  const out: Timed<T>[] = []
  const started = performance.now()
  try {
    const replies = await nc.requestMany(subject, undefined, { strategy: 'timer', maxWait: waitMs })
    for await (const m of replies) {
      try {
        out.push({ value: m.json<T>(), rttMs: performance.now() - started })
      } catch {
        // ignore malformed replies
      }
    }
  } catch (e) {
    if (!isNoResponders(e)) throw e
  }
  return out
}

/** `$SRV.<VERB>`, `$SRV.<VERB>.<name>`, or `$SRV.<VERB>.<name>.<id>`. */
export function srvSubject(verb: 'PING' | 'INFO' | 'STATS', name?: string, id?: string): string {
  return ['$SRV', verb, name?.trim(), name?.trim() && id?.trim()].filter(Boolean).join('.')
}

const byNameId = (a: { name: string; id: string }, b: { name: string; id: string }) =>
  a.name.localeCompare(b.name) || a.id.localeCompare(b.id)

/** `nats micro info [name] [id]` (and the data behind `micro ls`). */
export async function microInfo(url: string, name?: string, id?: string): Promise<MicroInfo[]> {
  const nc = await getConnection(url)
  const replies = await collect<MicroInfo>(nc, srvSubject('INFO', name, id))
  return replies.map((r) => r.value).sort(byNameId)
}

/** `nats micro ping [name]`: every instance with its round-trip time. */
export async function microPing(url: string, name?: string): Promise<Timed<MicroPing>[]> {
  const nc = await getConnection(url)
  const replies = await collect<MicroPing>(nc, srvSubject('PING', name))
  return replies.sort((a, b) => byNameId(a.value, b.value))
}

/** `nats micro stats [name] [id]` */
export async function microStats(url: string, name?: string, id?: string): Promise<MicroStats[]> {
  const nc = await getConnection(url)
  const replies = await collect<MicroStats>(nc, srvSubject('STATS', name, id))
  return replies.map((r) => r.value).sort(byNameId)
}

export interface ServiceSummary {
  name: string
  versions: string[]
  description?: string
  instances: MicroInfo[]
}

/** `nats micro ls`: instances grouped by service name. */
export function groupServices(infos: MicroInfo[]): ServiceSummary[] {
  const map = new Map<string, ServiceSummary>()
  for (const i of infos) {
    const s = map.get(i.name) ?? {
      name: i.name,
      versions: [],
      description: i.description,
      instances: [],
    }
    if (!s.versions.includes(i.version)) s.versions.push(i.version)
    s.instances.push(i)
    map.set(i.name, s)
  }
  return [...map.values()].sort((a, b) => a.name.localeCompare(b.name))
}

/** Unique endpoints of a service across its instances (by name). */
export function serviceEndpoints(instances: MicroInfo[]): MicroEndpointInfo[] {
  const map = new Map<string, MicroEndpointInfo>()
  for (const i of instances) for (const e of i.endpoints ?? []) if (!map.has(e.name)) map.set(e.name, e)
  return [...map.values()].sort((a, b) => a.name.localeCompare(b.name))
}

export function parseHeaderLines(text: string): HeaderList {
  const out: HeaderList = []
  for (const line of text.split('\n')) {
    const i = line.indexOf(':')
    if (i > 0) out.push([line.slice(0, i).trim(), line.slice(i + 1).trim()])
  }
  return out
}

function toNatsHeaders(list: HeaderList): MsgHdrs | undefined {
  if (list.length === 0) return undefined
  const h = natsHeaders()
  for (const [k, v] of list) h.append(k, v)
  return h
}

export function headerList(h?: MsgHdrs): HeaderList {
  const out: HeaderList = []
  for (const [k, vs] of h ?? []) for (const v of vs) out.push([k, v])
  return out
}

export interface ReqResult {
  ok: boolean
  rttMs: number
  headers: HeaderList
  data: Uint8Array
  /** Set for transport failures (no responders, timeout). */
  error?: string
  /** Set when the reply carries Nats-Service-Error* headers. */
  serviceError?: { code: string; message: string }
  /** Trace this request belongs to. */
  traceId: string
}

function serviceErrorOf(h?: MsgHdrs): ReqResult['serviceError'] {
  const code = h?.get('Nats-Service-Error-Code')
  return code ? { code, message: h?.get('Nats-Service-Error') ?? '' } : undefined
}

/**
 * Start a PRODUCER span for a raw NATS send and return the headers to send: the user's
 * headers with the span's `traceparent` (a user-typed `traceparent` becomes the parent).
 */
function tracedSend(
  op: 'request' | 'publish',
  url: string,
  subject: string,
  payload: string,
  headers: HeaderList,
  parent?: Context,
) {
  const started = startSpan(
    `${op === 'request' ? 'send' : 'publish'} ${subject}`,
    SpanKind.PRODUCER,
    {
      'messaging.system': 'nats',
      'messaging.operation.type': op === 'request' ? 'send' : 'publish',
      'messaging.destination.name': subject,
      'messaging.message.body.size': new TextEncoder().encode(payload).length,
      'server.address': url,
      'playground.tool': `nats ${op === 'request' ? 'req' : 'pub'}`,
    },
    parent ?? extractContext(headers),
  )
  const out: HeaderList = [
    ...headers.filter(([k]) => !isPropagationHeader(k)),
    ...Object.entries(traceHeaders(started.ctx)),
  ]
  return { ...started, headers: out }
}

/** `nats req <subject> <payload> [-H k:v]`. `parent` groups several requests in one trace. */
export async function natsRequest(
  url: string,
  subject: string,
  payload: string,
  headers: HeaderList,
  timeoutMs: number,
  parent?: Context,
): Promise<ReqResult> {
  const nc = await getConnection(url)
  const t = tracedSend('request', url, subject, payload, headers, parent)
  const started = performance.now()
  let r: ReqResult
  try {
    const msg: Msg = await nc.request(subject, payload, {
      timeout: timeoutMs,
      headers: toNatsHeaders(t.headers),
    })
    const serviceError = serviceErrorOf(msg.headers)
    r = {
      ok: !serviceError,
      rttMs: performance.now() - started,
      headers: headerList(msg.headers),
      data: msg.data,
      serviceError,
      traceId: t.traceId,
    }
  } catch (e) {
    r = {
      ok: false,
      rttMs: performance.now() - started,
      headers: [],
      data: new Uint8Array(),
      error: describeError(e, subject),
      traceId: t.traceId,
    }
  }
  endSpan(t.span, r.error ?? (r.serviceError ? `${r.serviceError.code}: ${r.serviceError.message}` : undefined), {
    'messaging.nats.reply_bytes': r.data.length,
  })
  if (!parent) void flushSpans()
  return r
}

/** `nats pub <subject> <payload> [-H k:v]`. Returns the trace id. */
export async function natsPublish(url: string, subject: string, payload: string, headers: HeaderList): Promise<string> {
  const nc = await getConnection(url)
  const t = tracedSend('publish', url, subject, payload, headers)
  try {
    nc.publish(subject, payload, { headers: toNatsHeaders(t.headers) })
    await nc.flush()
    endSpan(t.span)
  } catch (e) {
    endSpan(t.span, String(e))
    throw e
  } finally {
    void flushSpans()
  }
  return t.traceId
}

export interface TailMsg {
  at: number
  subject: string
  reply?: string
  headers: HeaderList
  data: Uint8Array
  /** From a `traceparent` header, when the message carries one. */
  traceId?: string
}

/** `nats sub <subject>`: returns an unsubscribe function. */
export async function natsSubscribe(
  url: string,
  subject: string,
  onMsg: (m: TailMsg) => void,
  onError: (e: Error) => void,
): Promise<() => void> {
  const nc = await getConnection(url)
  const sub = nc.subscribe(subject, {
    callback: (err, m) => {
      if (err) return onError(err)
      const headers = headerList(m.headers)
      onMsg({
        at: Date.now(),
        subject: m.subject,
        reply: m.reply,
        headers,
        data: m.data,
        traceId: traceIdFromTraceparent(headers.find(([k]) => k.toLowerCase() === 'traceparent')?.[1]),
      })
    },
  })
  sub.closed.then((e) => e instanceof Error && onError(e))
  return () => sub.unsubscribe()
}

/** `nats server info` / `nats rtt` */
export async function serverInfo(url: string) {
  const nc = await getConnection(url)
  const rttMs = await nc.rtt()
  return { server: nc.getServer(), rttMs, info: nc.info ?? {} }
}

export type Decoded =
  | { kind: 'empty' }
  | { kind: 'json'; text: string }
  | { kind: 'text'; text: string }
  | { kind: 'binary'; text: string }

/** JSON (pretty) → UTF-8 text → hex dump. */
export function decodePayload(data: Uint8Array): Decoded {
  if (data.length === 0) return { kind: 'empty' }
  let text: string
  try {
    text = new TextDecoder('utf-8', { fatal: true }).decode(data)
  } catch {
    return { kind: 'binary', text: hexDump(data) }
  }
  // Control chars other than whitespace usually mean binary (e.g. protobuf).
  // eslint-disable-next-line no-control-regex -- detecting binary payloads on purpose
  if (/[\u0000-\u0008\u000e-\u001f]/.test(text)) return { kind: 'binary', text: hexDump(data) }
  try {
    return { kind: 'json', text: JSON.stringify(JSON.parse(text), null, 2) }
  } catch {
    return { kind: 'text', text }
  }
}

export function hexDump(data: Uint8Array): string {
  const lines: string[] = []
  for (let off = 0; off < data.length; off += 16) {
    const row = data.subarray(off, off + 16)
    const hex = [...row].map((b) => b.toString(16).padStart(2, '0')).join(' ')
    const ascii = [...row].map((b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : '.')).join('')
    lines.push(`${off.toString(16).padStart(6, '0')}  ${hex.padEnd(47)}  ${ascii}`)
  }
  return lines.join('\n')
}

export function formatNanos(ns: number): string {
  if (!ns) return '0'
  if (ns < 1_000) return `${ns}ns`
  if (ns < 1_000_000) return `${(ns / 1_000).toFixed(ns < 10_000 ? 1 : 0)}µs`
  if (ns < 1_000_000_000) return `${(ns / 1_000_000).toFixed(ns < 10_000_000 ? 2 : 1)}ms`
  return `${(ns / 1_000_000_000).toFixed(2)}s`
}

export function formatMs(ms: number): string {
  return ms < 10 ? `${ms.toFixed(2)}ms` : `${ms.toFixed(0)}ms`
}
