import {
  fromJson,
  toJson,
  type DescMethod,
  type DescMethodServerStreaming,
  type DescMethodUnary,
  type JsonValue,
  type Registry,
} from '@bufbuild/protobuf'
import { ConnectError, type Interceptor, type Transport } from '@connectrpc/connect'
import { createConnectTransport, createGrpcWebTransport } from '@connectrpc/connect-web'
import { natsFetch } from './nats'
import { DEFAULT_NATS_PREFIX, type ServerEntry } from './servers'

export type Protocol = 'connect-json' | 'connect-binary' | 'grpc-web'

export interface CallOptions {
  server: ServerEntry
  protocol: Protocol
  method: DescMethod
  registry: Registry
  request: JsonValue
  headers: Record<string, string>
  timeoutMs?: number
  signal?: AbortSignal
  /** Called for every response message (once for unary). */
  onMessage: (msg: JsonValue) => void
}

export interface CallResult {
  ok: boolean
  durationMs: number
  header: [string, string][]
  trailer: [string, string][]
  error?: { code: string; message: string; details: JsonValue[] }
}

function makeTransport(server: ServerEntry, protocol: Protocol, registry: Registry): Transport {
  const jsonOptions = { registry }
  if (server.mode === 'nats') {
    if (protocol === 'grpc-web') throw new ConnectError('gRPC-web is not available over NATS; use Connect')
    return createConnectTransport({
      // Unused by natsFetch except for the RPC path, which becomes the subject.
      baseUrl: 'http://nats',
      fetch: natsFetch(server.url, server.prefix ?? DEFAULT_NATS_PREFIX),
      useBinaryFormat: protocol === 'connect-binary',
      jsonOptions,
    })
  }
  const interceptors: Interceptor[] = []
  let baseUrl = server.url.replace(/\/+$/, '')
  if (server.mode === 'proxy') {
    baseUrl = `${window.location.origin}/__proxy`
    interceptors.push((next) => (req) => {
      req.header.set('x-target', server.url)
      return next(req)
    })
  }
  if (protocol === 'grpc-web') {
    return createGrpcWebTransport({ baseUrl, interceptors, useBinaryFormat: true, jsonOptions })
  }
  return createConnectTransport({
    baseUrl,
    interceptors,
    useBinaryFormat: protocol === 'connect-binary',
    jsonOptions,
  })
}

function entries(h: Headers): [string, string][] {
  return [...h.entries()]
}

export function isBrowserSupported(method: DescMethod, server?: ServerEntry): boolean {
  if (server?.mode === 'nats') return method.methodKind === 'unary'
  return method.methodKind === 'unary' || method.methodKind === 'server_streaming'
}

/** Call any unary or server-streaming method dynamically from its descriptor. */
export async function callMethod(o: CallOptions): Promise<CallResult> {
  const { method, registry } = o
  const input = fromJson(method.input, o.request, { registry })
  const started = performance.now()
  const toOut = (m: Parameters<typeof toJson>[1]) =>
    toJson(method.output, m, { registry, alwaysEmitImplicit: true })

  try {
    if (!isBrowserSupported(method, o.server)) {
      throw new ConnectError(
        o.server.mode === 'nats'
          ? `${method.methodKind} methods are not supported over NATS (request/reply is unary)`
          : `${method.methodKind} methods are not supported by browsers (fetch has no request streaming)`,
      )
    }
    const transport = makeTransport(o.server, o.protocol, o.registry)
    if (method.methodKind === 'unary') {
      const res = await transport.unary(method as DescMethodUnary, o.signal, o.timeoutMs, o.headers, input)
      o.onMessage(toOut(res.message))
      return {
        ok: true,
        durationMs: performance.now() - started,
        header: entries(res.header),
        trailer: entries(res.trailer),
      }
    }
    if (method.methodKind === 'server_streaming') {
      const res = await transport.stream(
        method as DescMethodServerStreaming,
        o.signal,
        o.timeoutMs,
        o.headers,
        (async function* () {
          yield input
        })(),
      )
      for await (const m of res.message) o.onMessage(toOut(m))
      return {
        ok: true,
        durationMs: performance.now() - started,
        header: entries(res.header),
        trailer: entries(res.trailer),
      }
    }
    throw new ConnectError(`unsupported method kind ${method.methodKind}`)
  } catch (e) {
    const err = ConnectError.from(e)
    return {
      ok: false,
      durationMs: performance.now() - started,
      header: entries(err.metadata),
      trailer: [],
      error: {
        code: codeName(err.code),
        message: err.rawMessage,
        details: err.details.map((d) =>
          'desc' in d
            ? (toJson(d.desc, d.value as never, { registry }) as JsonValue)
            : { type: d.type, debug: (d.debug ?? null) as JsonValue },
        ),
      },
    }
  }
}

const CODE_NAMES = [
  'ok', 'canceled', 'unknown', 'invalid_argument', 'deadline_exceeded', 'not_found',
  'already_exists', 'permission_denied', 'resource_exhausted', 'failed_precondition',
  'aborted', 'out_of_range', 'unimplemented', 'internal', 'unavailable', 'data_loss',
  'unauthenticated',
]

function codeName(code: number): string {
  return CODE_NAMES[code] ?? String(code)
}
