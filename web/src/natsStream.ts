import { createInbox, TimeoutError, type Msg, type MsgHdrs, type NatsConnection } from '@nats-io/nats-core'

// Server-streaming RPCs over NATS, same wire format as the Rust adapter (crates/platform/src/nats/stream.rs):
// the request goes out with a reply inbox; every reply message is one chunk of the Connect
// streaming body, and an empty message with header `Nats-Stream-End` ends it.

export const STREAM_END = 'Nats-Stream-End'

/** Connect streaming content types: `application/connect+json` / `application/connect+proto`. */
export function isStreaming(contentType: string | null): boolean {
  return (contentType ?? '').trim().startsWith('application/connect+')
}

export type StreamCall = {
  subject: string
  body: Uint8Array
  headers: MsgHdrs
  idleTimeoutMs: number
}

/** The first reply (headers / error reply) plus the whole reply body as a stream. */
export type StreamReply = { first: Msg; body: ReadableStream<Uint8Array> }

export async function natsStream(nc: NatsConnection, call: StreamCall): Promise<StreamReply> {
  const inbox = createInbox()
  const sub = nc.subscribe(inbox)
  const messages = sub[Symbol.asyncIterator]()
  nc.publish(call.subject, call.body, { reply: inbox, headers: call.headers })
  const next = async (): Promise<Msg> => {
    let timer: ReturnType<typeof setTimeout> | undefined
    const idle = new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new TimeoutError()), call.idleTimeoutMs)
    })
    try {
      const result = await Promise.race([messages.next(), idle])
      if (result.done) throw new Error(`NATS stream ${call.subject} closed`)
      return result.value
    } finally {
      clearTimeout(timer)
    }
  }
  const first = await next()
  return { first, body: bodyStream(first, next, () => sub.unsubscribe()) }
}

function bodyStream(first: Msg, next: () => Promise<Msg>, close: () => void): ReadableStream<Uint8Array> {
  let pending: Msg | undefined = first
  return new ReadableStream<Uint8Array>({
    async pull(controller) {
      try {
        const msg = pending ?? (await next())
        pending = undefined
        if (msg.data.length > 0) controller.enqueue(new Uint8Array(msg.data))
        if (msg.headers?.get(STREAM_END)) {
          close()
          controller.close()
        }
      } catch (e) {
        close()
        controller.error(e)
      }
    },
    cancel: close,
  })
}
