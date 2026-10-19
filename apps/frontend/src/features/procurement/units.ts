export const PROCUREMENT_UNITS = [
  "kg",
  "L",
  "株",
  "缶",
  "個",
  "束",
  "玉",
  "丁",
  "杯",
  "箱",
  "袋",
  "房",
  "本",
  "枚",
] as const;

export type ProcurementUnit = (typeof PROCUREMENT_UNITS)[number];

export const DEFAULT_PROCUREMENT_UNIT: ProcurementUnit = "個";
