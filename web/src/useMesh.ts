import { useCallback, useEffect, useState } from 'react'
import { listInstances, type InstanceRow } from './micro'
import { fetchAllInstanceStates } from './rpcClient'

interface MeshData {
  /** Running instances (NATS micro discovery). */
  rows: InstanceRow[]
  /** Every instance's CAS state (KV bucket `wgic_instance_state`). */
  kvStates: Record<string, unknown>
}

export interface Mesh extends MeshData {
  refresh: () => Promise<void>
}

async function loadMesh(natsUrl: string): Promise<MeshData> {
  const [rows, kvStates] = await Promise.all([listInstances(natsUrl).catch(() => []), fetchAllInstanceStates(natsUrl)])
  return { rows, kvStates }
}

/** What runs on the mesh and its state; loaded on start / URL change and on `refresh()`. */
export function useMesh(natsUrl: string): Mesh {
  const [data, setData] = useState<MeshData>({ rows: [], kvStates: {} })

  useEffect(() => {
    let live = true
    void loadMesh(natsUrl).then((next) => live && setData(next))
    return () => {
      live = false
    }
  }, [natsUrl])

  const refresh = useCallback(async () => setData(await loadMesh(natsUrl)), [natsUrl])
  return { ...data, refresh }
}
