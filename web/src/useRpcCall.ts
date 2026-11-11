import { useState } from 'react'
import { callNatsRpc, RpcCallError, type StreamCallOptions } from './rpcClient'

export interface ProgressStep {
  /** ms since the call started */
  atMs: number
  text: string
}

export interface CallView {
  loading: boolean
  steps: ProgressStep[]
  /** The answer so far, from text frames. */
  answer: string
  responseJson: string
  errorText: string
  durationMs: number | null
  traceId: string
}

export const IDLE_CALL: CallView = {
  loading: false,
  steps: [],
  answer: '',
  responseJson: '',
  errorText: '',
  durationMs: null,
  traceId: '',
}

export type SendOptions = Omit<StreamCallOptions, 'onProgress' | 'onText'>

/** One call at a time: its live progress, streamed answer text and result. */
export function useRpcCall() {
  const [view, setView] = useState<CallView>(IDLE_CALL)

  const send = async (opts: SendOptions) => {
    const started = performance.now()
    const atMs = () => Math.round(performance.now() - started)
    setView({ ...IDLE_CALL, loading: true })
    try {
      const res = await callNatsRpc({
        ...opts,
        onProgress: (text) => setView((v) => ({ ...v, steps: [...v.steps, { atMs: atMs(), text }] })),
        onText: (delta) => setView((v) => ({ ...v, answer: v.answer + delta })),
      })
      setView((v) => ({ ...v, loading: false, ...res }))
    } catch (err) {
      const traceId = err instanceof RpcCallError ? err.traceId : ''
      const errorText = err instanceof Error ? err.message : String(err)
      setView((v) => ({ ...v, loading: false, errorText, traceId, durationMs: atMs() }))
    }
  }

  return { view, send, reset: () => setView(IDLE_CALL) }
}
