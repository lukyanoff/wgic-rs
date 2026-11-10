export interface RpcSpanHandle {
  traceId: string;
  spanId: string;
  traceparent: string;
  endOk: () => void;
  endErr: (err: unknown) => void;
}

function randomHex(byteLength: number): string {
  const bytes = new Uint8Array(byteLength);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

export function startRpcSpan(
  _name: string,
  _attrs?: Record<string, string>
): RpcSpanHandle {
  const traceId = randomHex(16);
  const spanId = randomHex(8);
  const traceparent = `00-${traceId}-${spanId}-01`;

  return {
    traceId,
    spanId,
    traceparent,
    endOk: () => {},
    endErr: () => {},
  };
}
