export type ServerMode = 'proxy' | 'direct'

export interface ServerEntry {
  name: string
  url: string
  /** proxy: through the Vite dev proxy (no CORS needed); direct: browser → server (needs CORS). */
  mode: ServerMode
}

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
