import {
  createFileRegistry,
  fromBinary,
  type DescMethod,
  type DescService,
  type FileRegistry,
} from '@bufbuild/protobuf'
import { FileDescriptorSetSchema } from '@bufbuild/protobuf/wkt'

export interface Schema {
  registry: FileRegistry
  services: DescService[]
}

/** Load the descriptor set produced by `buf build -o web/public/descriptors.binpb`. */
export async function loadSchema(url = '/descriptors.binpb'): Promise<Schema> {
  const res = await fetch(url)
  if (!res.ok) {
    throw new Error(`failed to fetch ${url}: ${res.status}. Run "npm run gen" in the repo root.`)
  }
  const set = fromBinary(FileDescriptorSetSchema, new Uint8Array(await res.arrayBuffer()))
  const registry = createFileRegistry(set)
  const services: DescService[] = []
  for (const t of registry) {
    if (t.kind === 'service') services.push(t)
  }
  services.sort((a, b) => a.typeName.localeCompare(b.typeName))
  return { registry, services }
}

/** Leading comment of a method, from source info (if buf kept it). */
export function methodComment(method: DescMethod): string | undefined {
  const service = method.parent
  const file = service.file
  const path = [6, file.services.indexOf(service), 2, service.methods.indexOf(method)]
  const loc = file.proto.sourceCodeInfo?.location.find(
    (l) => l.path.length === path.length && l.path.every((v, i) => v === path[i]),
  )
  return loc?.leadingComments.trim() || undefined
}

export function methodPath(method: DescMethod): string {
  return `/${method.parent.typeName}/${method.name}`
}
