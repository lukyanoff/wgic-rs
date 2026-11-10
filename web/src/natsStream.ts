export const KIND_PROGRESS = 0x01
export const KIND_OK = 0x02
export const KIND_ERR = 0x03

export type ParsedStreamFrame =
  | { kind: 'progress'; message: string }
  | { kind: 'ok'; payload: Uint8Array }
  | { kind: 'err'; code: string; message: string }

const decoder = new TextDecoder()

export function parseStreamFrame(bytes: Uint8Array): ParsedStreamFrame {
  if (bytes.length === 0) {
    return { kind: 'err', code: 'internal', message: 'empty stream frame' }
  }
  const tag = bytes[0]
  const body = bytes.subarray(1)

  if (tag === KIND_PROGRESS) {
    return { kind: 'progress', message: decoder.decode(body) }
  }
  if (tag === KIND_OK) {
    return { kind: 'ok', payload: body }
  }
  if (tag === KIND_ERR) {
    const text = decoder.decode(body)
    const sep = text.indexOf(': ')
    if (sep !== -1) {
      return {
        kind: 'err',
        code: text.slice(0, sep),
        message: text.slice(sep + 2),
      }
    }
    return { kind: 'err', code: 'internal', message: text }
  }

  return {
    kind: 'err',
    code: 'internal',
    message: `unknown stream frame tag: 0x${tag.toString(16)}`,
  }
}
