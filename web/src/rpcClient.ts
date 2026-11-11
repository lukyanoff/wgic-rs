import { create, fromBinary, toBinary, toJson } from '@bufbuild/protobuf'
import { connect, createInbox, headers as natsHeaders, type KV, type Msg, type NatsConnection } from 'nats.ws'
import { parseStreamFrame } from './natsStream'
import { startRpcSpan } from './otel'
import type { MethodEntry } from './schema'

export interface StreamCallOptions {
  natsUrl: string
  module: string
  instance: string
  entry: MethodEntry
  jsonInput: string
  /** Sent as `Wgic-Timeout-Ms`: the server stops the call (and everything it called) when it runs out. */
  timeoutMs: number
  onProgress: (step: string) => void
  /** A piece of the answer while it is being written. */
  onText: (delta: string) => void
}

export interface StreamCallResult {
  responseJson: string
  traceId: string
  durationMs: number
}

/** A failed call; `traceId` still finds it in Jaeger. */
export class RpcCallError extends Error {
  readonly traceId: string
  constructor(message: string, traceId: string) {
    super(message)
    this.traceId = traceId
  }
}

/** The browser waits this much longer than the server's deadline, so the server's own error wins. */
const GRACE_MS = 2_000

let cachedConn: { url: string; nc: NatsConnection } | null = null

export async function getNatsConnection(url: string): Promise<NatsConnection> {
  if (cachedConn && cachedConn.url === url && !cachedConn.nc.isClosed()) {
    return cachedConn.nc
  }
  const nc = await connect({ servers: url })
  cachedConn = { url, nc }
  return nc
}

export async function callNatsRpc(opts: StreamCallOptions): Promise<StreamCallResult> {
  const { natsUrl, module, instance, entry } = opts
  const reqBytes = toBinary(entry.method.input, create(entry.method.input, JSON.parse(opts.jsonInput || '{}')))
  const subject = `rpc.${module}.${instance}.${entry.service.typeName}.${entry.method.name}`
  const span = startRpcSpan(`${entry.service.typeName}/${entry.method.name}`, {
    'rpc.system': 'nats',
    'rpc.service': entry.service.typeName,
    'rpc.method': entry.method.name,
    'wgic.target': `${module}/${instance}`,
  })

  const start = performance.now()
  try {
    const nc = await getNatsConnection(natsUrl)
    const req = { subject, data: reqBytes, traceparent: span.traceparent, timeoutMs: opts.timeoutMs }
    const payload = await sendAndAwaitReply(nc, req, opts)
    const decoded = fromBinary(entry.method.output, payload)
    span.endOk()
    return {
      responseJson: JSON.stringify(toJson(entry.method.output, decoded), null, 2),
      traceId: span.traceId,
      durationMs: Math.round(performance.now() - start),
    }
  } catch (err) {
    span.endErr(err)
    throw new RpcCallError(err instanceof Error ? err.message : String(err), span.traceId)
  }
}

interface OutgoingRequest {
  subject: string
  data: Uint8Array
  traceparent: string
  timeoutMs: number
}

/**
 * Publishes the request with a private reply inbox, forwards progress and text frames, and resolves with the Ok
 * payload. Rejects on a micro error reply, when nothing listens on `subject` (NATS 503), or after the timeout.
 */
async function sendAndAwaitReply(
  nc: NatsConnection,
  req: OutgoingRequest,
  handlers: Pick<StreamCallOptions, 'onProgress' | 'onText'>,
): Promise<Uint8Array> {
  const sub = nc.subscribe(createInbox())
  let timedOut = false
  const timer = setTimeout(() => {
    timedOut = true
    sub.unsubscribe()
  }, req.timeoutMs + GRACE_MS)
  const headers = natsHeaders()
  headers.set('traceparent', req.traceparent)
  headers.set('Wgic-Timeout-Ms', String(req.timeoutMs))
  nc.publish(req.subject, req.data, { reply: sub.getSubject(), headers })

  try {
    for await (const msg of sub) {
      const payload = handleReply(msg, req.subject, handlers)
      if (payload) return payload
    }
  } finally {
    clearTimeout(timer)
    sub.unsubscribe()
  }
  throw new Error(timedOut ? `deadline_exceeded: no reply in ${req.timeoutMs / 1000}s` : 'stream closed early')
}

/** One inbox message: the Ok payload when the call is done, `undefined` for progress. Throws on errors. */
function handleReply(msg: Msg, subject: string, handlers: Pick<StreamCallOptions, 'onProgress' | 'onText'>) {
  const serviceError = msg.headers?.get('Nats-Service-Error')
  if (serviceError) throw new Error(serviceError)
  if (msg.headers?.code === 503) throw new Error(`unavailable: nothing is listening on ${subject}`)
  const frame = parseStreamFrame(msg.data)
  if (frame.kind === 'ok') return frame.payload
  if (frame.kind === 'text') handlers.onText(frame.text)
  else handlers.onProgress(frame.message)
  return undefined
}

/** Every key of the `wgic_instance_state` KV bucket, decoded as JSON (or raw text). `{}` when NATS is down. */
export async function fetchAllInstanceStates(natsUrl: string): Promise<Record<string, unknown>> {
  try {
    const nc = await getNatsConnection(natsUrl)
    const kv: KV = await nc.jetstream().views.kv('wgic_instance_state')
    // Drain the key iterator before any kv.get(): awaiting inside it ends the iteration early.
    const keys: string[] = []
    for await (const key of await kv.keys()) keys.push(key)
    keys.sort()

    const result: Record<string, unknown> = {}
    for (const key of keys) {
      const entry = await kv.get(key)
      if (entry && entry.value.length > 0) result[key] = decodeValue(entry.value)
    }
    return result
  } catch {
    return {}
  }
}

function decodeValue(bytes: Uint8Array): unknown {
  const text = new TextDecoder().decode(bytes)
  try {
    return JSON.parse(text)
  } catch {
    return text
  }
}
