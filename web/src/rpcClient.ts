import { create, fromBinary, toBinary, toJson } from '@bufbuild/protobuf'
import { connect, createInbox, headers as natsHeaders, type KV, type NatsConnection } from 'nats.ws'
import { parseStreamFrame } from './natsStream'
import { startRpcSpan } from './otel'
import type { MethodEntry } from './schema'

export interface StreamCallOptions {
  natsUrl: string
  module: string
  instance: string
  entry: MethodEntry
  jsonInput: string
  onProgress: (step: string) => void
}

export interface StreamCallResult {
  responseJson: string
  traceId: string
  durationMs: number
}

/** Browser-side cap for one call; a visioner call waits on a sensor plus an LLM (each up to 60 s server-side). */
const RPC_TIMEOUT_MS = 150_000

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
  const { natsUrl, module, instance, entry, jsonInput, onProgress } = opts
  const parsed = JSON.parse(jsonInput || '{}')
  const msg = create(entry.method.input, parsed)
  const reqBytes = toBinary(entry.method.input, msg)

  const subject = `rpc.${module}.${instance}.${entry.service.typeName}.${entry.method.name}`
  const span = startRpcSpan(`${entry.service.typeName}/${entry.method.name}`, {
    'rpc.system': 'nats',
    'rpc.service': entry.service.typeName,
    'rpc.method': entry.method.name,
    'wgic.module': module,
    'wgic.instance': instance,
  })

  const start = performance.now()
  try {
    const nc = await getNatsConnection(natsUrl)
    const req = { subject, data: reqBytes, traceparent: span.traceparent }
    const payload = await sendAndAwaitReply(nc, req, onProgress)
    const decoded = fromBinary(entry.method.output, payload)
    span.endOk()
    return {
      responseJson: JSON.stringify(toJson(entry.method.output, decoded), null, 2),
      traceId: span.traceId,
      durationMs: Math.round(performance.now() - start),
    }
  } catch (err) {
    span.endErr(err)
    throw err
  }
}

/**
 * Publishes the request with a private reply inbox, forwards progress frames, and resolves with the Ok payload.
 * Rejects on an Err frame, when nothing listens on `subject` (NATS 503 "no responders"), or after RPC_TIMEOUT_MS.
 */
async function sendAndAwaitReply(
  nc: NatsConnection,
  req: { subject: string; data: Uint8Array; traceparent: string },
  onProgress: (step: string) => void,
): Promise<Uint8Array> {
  const { subject } = req
  const sub = nc.subscribe(createInbox())
  let timedOut = false
  const timer = setTimeout(() => {
    timedOut = true
    sub.unsubscribe()
  }, RPC_TIMEOUT_MS)
  const headers = natsHeaders()
  headers.set('traceparent', req.traceparent)
  nc.publish(subject, req.data, { reply: sub.getSubject(), headers })

  try {
    for await (const msg of sub) {
      if (msg.headers?.code === 503) throw new Error(`unavailable: nothing is listening on ${subject}`)
      const frame = parseStreamFrame(msg.data)
      if (frame.kind === 'ok') return frame.payload
      if (frame.kind === 'err') throw new Error(`${frame.code}: ${frame.message}`)
      onProgress(frame.message)
    }
  } finally {
    clearTimeout(timer)
    sub.unsubscribe()
  }
  throw new Error(timedOut ? `deadline_exceeded: no reply in ${RPC_TIMEOUT_MS / 1000}s` : 'stream closed early')
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
