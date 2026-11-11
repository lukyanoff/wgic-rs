// Messages on a call's reply inbox (see the Rust `sdk::wire`). Errors are not frames: they arrive as a NATS
// micro error reply (empty payload, header `Nats-Service-Error: "<code>: <message>"`).

export const KIND_PROGRESS = 0x01
export const KIND_OK = 0x02
export const KIND_TEXT = 0x04

export type ParsedStreamFrame =
  | { kind: 'progress'; message: string }
  | { kind: 'text'; text: string }
  | { kind: 'ok'; payload: Uint8Array }

const decoder = new TextDecoder()

export function parseStreamFrame(bytes: Uint8Array): ParsedStreamFrame {
  const tag = bytes[0]
  const body = bytes.subarray(1)
  if (tag === KIND_PROGRESS) return { kind: 'progress', message: decoder.decode(body) }
  if (tag === KIND_TEXT) return { kind: 'text', text: decoder.decode(body) }
  if (tag === KIND_OK) return { kind: 'ok', payload: body }
  throw new Error(`internal: unknown stream frame tag 0x${(tag ?? 0).toString(16)}`)
}
