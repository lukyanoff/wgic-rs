import { RequestStrategy, type NatsConnection } from 'nats.ws'
import { getNatsConnection } from './rpcClient'

// Browser version of `nats micro ls` + `nats micro stats`: every host process answers `$SRV.INFO` and
// `$SRV.STATS` (NATS micro protocol). One row per `<module>/<instance>`; HA replicas are summed.

export interface InstanceRow {
  module: string
  instance: string
  replicas: number
  requests: number
  errors: number
  avgMs: number
  lastError: string
}

interface MicroInfo {
  name: string
  id: string
  metadata?: Record<string, string> | null
}

interface MicroStats {
  id: string
  endpoints: { num_requests: number; num_errors: number; processing_time: number; last_error?: string }[]
}

/** Scatter-gather: one request, every reply that arrives within `waitMs`. */
async function collect<T>(nc: NatsConnection, subject: string, waitMs = 500): Promise<T[]> {
  const out: T[] = []
  try {
    const replies = await nc.requestMany(subject, undefined, { strategy: RequestStrategy.Timer, maxWait: waitMs })
    for await (const msg of replies) {
      if (msg.data.length > 0) out.push(msg.json<T>())
    }
  } catch {
    // no responders: nothing is running
  }
  return out
}

export async function listInstances(natsUrl: string): Promise<InstanceRow[]> {
  const nc = await getNatsConnection(natsUrl)
  const [infos, stats] = await Promise.all([collect<MicroInfo>(nc, '$SRV.INFO'), collect<MicroStats>(nc, '$SRV.STATS')])
  const statsById = new Map(stats.map((s) => [s.id, s]))
  const rows = new Map<string, InstanceRow & { totalNs: number }>()
  for (const info of infos) {
    const instance = info.metadata?.instance ?? info.id
    const key = `${info.name}/${instance}`
    const row = rows.get(key) ?? {
      module: info.name,
      instance,
      replicas: 0,
      requests: 0,
      errors: 0,
      avgMs: 0,
      lastError: '',
      totalNs: 0,
    }
    row.replicas += 1
    for (const ep of statsById.get(info.id)?.endpoints ?? []) {
      row.requests += ep.num_requests
      row.errors += ep.num_errors
      row.totalNs += ep.processing_time
      row.lastError ||= ep.last_error ?? ''
    }
    rows.set(key, row)
  }
  return [...rows.values()]
    .map(({ totalNs, ...row }) => ({ ...row, avgMs: row.requests ? Math.round(totalNs / row.requests / 1e6) : 0 }))
    .sort((a, b) => a.module.localeCompare(b.module) || a.instance.localeCompare(b.instance))
}
