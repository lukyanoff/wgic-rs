import { createFileRegistry, fromBinary, type DescMethod, type DescService } from '@bufbuild/protobuf'
import { FileDescriptorSetSchema } from '@bufbuild/protobuf/wkt'

export interface MethodEntry {
  label: string
  service: DescService
  method: DescMethod
}

/** Load every method from the descriptor set built by `npm run gen` (`web/public/descriptors.binpb`). */
export async function loadMethods(url = '/descriptors.binpb'): Promise<MethodEntry[]> {
  const res = await fetch(url)
  if (!res.ok) {
    throw new Error(`failed to fetch ${url}: ${res.status}. Run "npm run gen" in the repo root.`)
  }
  const set = fromBinary(FileDescriptorSetSchema, new Uint8Array(await res.arrayBuffer()))
  const services: DescService[] = []
  for (const t of createFileRegistry(set)) {
    if (t.kind === 'service') services.push(t)
  }
  services.sort((a, b) => a.typeName.localeCompare(b.typeName))
  return services.flatMap((service) =>
    service.methods.map((method) => ({ label: `${service.typeName} / ${method.name}`, service, method })),
  )
}
