export type ServerMode = 'proxy' | 'direct' | 'nats'

export interface ServerEntry {
  name: string
  url: string
  /**
   * proxy: HTTP through the Vite dev proxy (no CORS needed)
   * direct: HTTP browser → server (needs CORS)
   * nats: NATS micro over WebSocket; `url` is ws://host:port
   */
  mode: ServerMode
  /** nats mode: subject prefix (default "rpc"). */
  prefix?: string
}

export const DEFAULT_NATS_PREFIX = 'rpc'

const KEY = 'playground.servers'

export async function loadServers(): Promise<ServerEntry[]> {
  try {
    const saved = localStorage.getItem(KEY)
    if (saved) return JSON.parse(saved) as ServerEntry[]
  } catch {
    // ignore storage errors and fall back to servers.json
  }
  const res = await fetch('/servers.json')
  return res.ok ? ((await res.json()) as ServerEntry[]) : []
}

export function saveServers(servers: ServerEntry[]) {
  try {
    localStorage.setItem(KEY, JSON.stringify(servers))
  } catch {
    // ignore
  }
}

export function resetServers() {
  try {
    localStorage.removeItem(KEY)
  } catch {
    // ignore
  }
}

const SELECTED_KEY = 'playground.server'

/** Index of the server picked last time (by name), or 0. */
export function selectedServerIndex(servers: ServerEntry[]): number {
  try {
    const name = localStorage.getItem(SELECTED_KEY)
    return Math.max(
      0,
      servers.findIndex((s) => s.name === name),
    )
  } catch {
    return 0
  }
}

/** Remember the picked server, so a reload keeps it. */
export function saveSelectedServer(server: ServerEntry | undefined) {
  try {
    if (server) localStorage.setItem(SELECTED_KEY, server.name)
  } catch {
    // ignore
  }
}
