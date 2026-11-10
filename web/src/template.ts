import { ScalarType, type DescField, type DescMessage, type JsonValue } from '@bufbuild/protobuf'

// Well-known types with special JSON forms.
const WKT_TEMPLATES: Record<string, JsonValue> = {
  'google.protobuf.Timestamp': '1970-01-01T00:00:00Z',
  'google.protobuf.Duration': '0s',
  'google.protobuf.FieldMask': '',
  'google.protobuf.Struct': {},
  'google.protobuf.Value': null,
  'google.protobuf.ListValue': [],
  'google.protobuf.Empty': {},
  'google.protobuf.Any': {},
}

function scalarTemplate(t: ScalarType): JsonValue {
  switch (t) {
    case ScalarType.STRING:
      return ''
    case ScalarType.BOOL:
      return false
    case ScalarType.BYTES:
      return ''
    case ScalarType.INT64:
    case ScalarType.UINT64:
    case ScalarType.SINT64:
    case ScalarType.FIXED64:
    case ScalarType.SFIXED64:
      return '0'
    default:
      return 0
  }
}

function fieldTemplate(field: DescField, depth: number): JsonValue {
  switch (field.fieldKind) {
    case 'scalar':
      return scalarTemplate(field.scalar)
    case 'enum':
      return field.enum.values[0]?.name ?? null
    case 'message':
      return messageTemplate(field.message, depth + 1)
    case 'list':
    case 'map':
      return field.fieldKind === 'list' ? [] : {}
  }
}

/** A JSON skeleton for a message, so the request editor starts pre-filled. */
export function messageTemplate(message: DescMessage, depth = 0): JsonValue {
  const wkt = WKT_TEMPLATES[message.typeName]
  if (wkt !== undefined) return wkt
  // Recursive messages: stop expanding after a few levels.
  if (depth > 3) return {}
  const out: Record<string, JsonValue> = {}
  for (const field of message.fields) {
    // Only include the first member of each oneof.
    if (field.oneof && field.oneof.fields[0] !== field) continue
    out[field.jsonName] = fieldTemplate(field, depth)
  }
  return out
}
