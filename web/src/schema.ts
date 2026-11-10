import type { DescMethod, DescService } from "@bufbuild/protobuf";
import { LlmCallerService } from "./gen/llm_caller/v1/llm_caller_pb";
import { SensorService } from "./gen/sensor/v1/sensor_pb";
import { VisionerService } from "./gen/visioner/v1/visioner_pb";

export interface MethodEntry {
  label: string;
  service: DescService;
  method: DescMethod;
}

const SERVICES: DescService[] = [
  VisionerService,
  SensorService,
  LlmCallerService,
];

export const METHODS: MethodEntry[] = SERVICES.flatMap((service) =>
  service.methods.map((method) => ({
    label: `${service.typeName} / ${method.name}`,
    service,
    method,
  }))
);
