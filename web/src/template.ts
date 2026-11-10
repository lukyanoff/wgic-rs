import { create, toJson, type DescMessage } from "@bufbuild/protobuf";

const DEFAULTS: Record<string, Record<string, unknown>> = {
  "visioner.v1.CaptureRequest": {
    sensor: "fake",
    llmCaller: "agy",
    systemPrompt: "You are a helpful visual assistant.",
    textPrompt: "",
    userPrompt: "Describe what you see in this image.",
  },
  "llm_caller.v1.CallRequest": {
    systemPrompt: "You are a concise assistant.",
    textPrompt: "",
    userPrompt: "Say hello in one sentence.",
    images: [],
  },
  "sensor.v1.CaptureRequest": {},
};

export function buildTemplate(desc: DescMessage): string {
  const preset = DEFAULTS[desc.typeName];
  if (preset) {
    return JSON.stringify(preset, null, 2);
  }
  const empty = create(desc);
  return JSON.stringify(toJson(desc, empty, { alwaysEmitImplicit: true }), null, 2);
}
