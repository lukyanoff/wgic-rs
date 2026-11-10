import { create, fromBinary, toBinary, toJson } from "@bufbuild/protobuf";
import { connect, headers as natsHeaders, type NatsConnection, type KV } from "nats.ws";
import { parseStreamFrame } from "./natsStream";
import { startRpcSpan } from "./otel";
import type { MethodEntry } from "./schema";

export interface StreamCallOptions {
  natsUrl: string;
  module: string;
  instance: string;
  entry: MethodEntry;
  jsonInput: string;
  onProgress: (step: string) => void;
}

export interface StreamCallResult {
  responseJson: string;
  traceId: string;
  durationMs: number;
}

let cachedConn: { url: string; nc: NatsConnection } | null = null;

export async function getNatsConnection(url: string): Promise<NatsConnection> {
  if (cachedConn && cachedConn.url === url && !cachedConn.nc.isClosed()) {
    return cachedConn.nc;
  }
  const nc = await connect({ servers: url });
  cachedConn = { url, nc };
  return nc;
}

export async function callNatsRpc(opts: StreamCallOptions): Promise<StreamCallResult> {
  const { natsUrl, module, instance, entry, jsonInput, onProgress } = opts;
  const parsed = JSON.parse(jsonInput || "{}");
  const msg = create(entry.method.input, parsed);
  const reqBytes = toBinary(entry.method.input, msg);

  const subject = `rpc.${module}.${instance}.${entry.service.typeName}.${entry.method.name}`;
  const span = startRpcSpan(`${entry.service.typeName}/${entry.method.name}`, {
    "rpc.system": "nats",
    "rpc.service": entry.service.typeName,
    "rpc.method": entry.method.name,
    "wgic.module": module,
    "wgic.instance": instance,
  });

  const start = performance.now();
  try {
    const nc = await getNatsConnection(natsUrl);
    const inbox = nc.newInbox();
    const sub = nc.subscribe(inbox);

    const hdrs = natsHeaders();
    hdrs.set("traceparent", span.traceparent);

    nc.publish(subject, reqBytes, { reply: inbox, headers: hdrs });

    for await (const rawMsg of sub) {
      const frame = parseStreamFrame(rawMsg.data);
      if (frame.kind === "progress") {
        onProgress(frame.message);
      } else if (frame.kind === "ok") {
        sub.unsubscribe();
        const decoded = fromBinary(entry.method.output, frame.payload);
        const responseJson = JSON.stringify(toJson(entry.method.output, decoded), null, 2);
        span.endOk();
        return {
          responseJson,
          traceId: span.traceId,
          durationMs: Math.round(performance.now() - start),
        };
      } else {
        sub.unsubscribe();
        throw new Error(`${frame.code}: ${frame.message}`);
      }
    }
    throw new Error("stream closed without terminal frame");
  } catch (err) {
    span.endErr(err);
    throw err;
  }
}

export async function fetchAllInstanceStates(natsUrl: string): Promise<Record<string, unknown>> {
  const nc = await getNatsConnection(natsUrl);
  const js = nc.jetstream();
  try {
    const kv: KV = await js.views.kv("wgic_instance_state");
    const keys = await kv.keys();
    const result: Record<string, unknown> = {};
    for await (const key of keys) {
      const entry = await kv.get(key);
      if (entry && entry.value.length > 0) {
        try {
          result[key] = JSON.parse(new TextDecoder().decode(entry.value));
        } catch {
          result[key] = new TextDecoder().decode(entry.value);
        }
      }
    }
    return result;
  } catch {
    return {};
  }
}
