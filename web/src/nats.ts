import {
  headers as natsHeaders,
  NoRespondersError,
  RequestError,
  TimeoutError,
  wsconnect,
  type Msg,
  type NatsConnection,
} from '@nats-io/nats-core'

// Calls Connect unary RPCs over NATS (WebSocket), using the same wire format as
// the Rust adapter in src/nats.rs:
// - subject: <prefix>.<package.Service>.<Method>
// - request: payload = message, headers = Content-Type + RPC metadata
// - error reply: headers Nats-Service-Error, Nats-Service-Error-Code, Connect-Code

const DEFAULT_TIMEOUT_MS = 10_000
const connections = new Map<string, Promise<NatsConnection>>()

/** One shared WebSocket connection per NATS URL. */
export function getConnection(url: string): Promise<NatsConnection> {
  let conn = connections.get(url)
  if (!conn) {
    conn = wsconnect({ servers: url, name: 'connect-playground' })
    connections.set(url, conn)
    conn.then(
      (nc) => nc.closed().then(() => connections.delete(url)),
      () => connections.delete(url),
    )
  }
  return conn
}

/** `/greet.v1.GreetService/Greet` → `rpc.greet.v1.GreetService.Greet` */
export function subjectFor(prefix: string, path: string): string {
  const p = path.replace(/^\/+/, '').replaceAll('/', '.')
  return prefix ? `${prefix}.${p}` : p
}

const CODE_FOR_STATUS: Record<number, string> = {
  400: 'invalid_argument',
  401: 'unauthenticated',
  403: 'permission_denied',
  404: 'unimplemented',
  408: 'deadline_exceeded',
  409: 'aborted',
  412: 'failed_precondition',
  413: 'resource_exhausted',
  429: 'resource_exhausted',
  499: 'canceled',
  501: 'unimplemented',
  502: 'unavailable',
  503: 'unavailable',
  504: 'deadline_exceeded',
}

function errorResponse(status: number, code: string, message: string): Response {
  return new Response(JSON.stringify({ code, message }), {
    status,
    headers: { 'content-type': 'application/json' },
  })
}

function replyToResponse(msg: Msg): Response {
  const h = msg.headers
  const errCode = h?.get('Nats-Service-Error-Code')
  if (errCode) {
    const status = Number(errCode) || 500
    const code = h?.get('Connect-Code') || CODE_FOR_STATUS[status] || 'unknown'
    return errorResponse(status, code, h?.get('Nats-Service-Error') ?? '')
  }
  const out = new Headers()
  for (const [k, vs] of h ?? []) for (const v of vs) out.append(k, v)
  // The body must be a standalone ArrayBuffer-backed view for the Response constructor.
  return new Response(new Uint8Array(msg.data), { status: 200, headers: out })
}

/**
 * A `fetch` that sends Connect unary requests over NATS request/reply, for
 * `createConnectTransport({ fetch })`. Only the URL path is used.
 */
export function natsFetch(url: string, prefix: string): typeof fetch {
  return async (input, init) => {
    const req = new Request(input, init)
    const subject = subjectFor(prefix, new URL(req.url).pathname)
    const body = new Uint8Array(await req.arrayBuffer())
    const hdrs = natsHeaders()
    req.headers.forEach((v, k) => hdrs.append(k, v))
    const timeout = Number(req.headers.get('connect-timeout-ms')) || DEFAULT_TIMEOUT_MS

    let nc: NatsConnection
    try {
      nc = await getConnection(url)
    } catch (e) {
      return errorResponse(503, 'unavailable', `cannot connect to NATS at ${url}: ${String(e)}`)
    }

    const signal = init?.signal ?? undefined
    if (signal?.aborted) throw new DOMException('aborted', 'AbortError')
    const aborted = new Promise<never>((_, reject) =>
      signal?.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')), {
        once: true,
      }),
    )
    try {
      const msg = await Promise.race([nc.request(subject, body, { timeout, headers: hdrs }), aborted])
      return replyToResponse(msg)
    } catch (e) {
      if (e instanceof DOMException && e.name === 'AbortError') throw e
      const cause = e instanceof RequestError ? e.cause : e
      if (cause instanceof NoRespondersError || (e instanceof RequestError && e.isNoResponders())) {
        return errorResponse(503, 'unavailable', `no NATS responders for ${subject}`)
      }
      if (cause instanceof TimeoutError || e instanceof TimeoutError) {
        return errorResponse(504, 'deadline_exceeded', `NATS request to ${subject} timed out`)
      }
      return errorResponse(503, 'unavailable', `NATS ${subject}: ${String(e)}`)
    }
  }
}
