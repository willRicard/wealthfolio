import { describe, expect, it, vi } from "vitest";

import { describeCategories } from "./change-descriptor";
import { buildHeadline } from "./headline";

describe("buildHeadline", () => {
  it("uses translated copy for empty periods", () => {
    const t = vi.fn((key: string) =>
      key === "spending:whatChanged.headlineNoActivity"
        ? "この期間に記録された支出はありません。"
        : key,
    );

    const headline = buildHeadline({
      periodState: { kind: "no_activity_either_side" },
      movers: [],
      currentTotal: 0,
      priorTotal: 0,
      priorLabel: "前回",
      metaLabel: "",
      t,
    });

    expect(headline.fragments).toEqual([
      { type: "text", text: "この期間に記録された支出はありません。" },
    ]);
  });

  it("removes date-abbreviation punctuation before the translated comparison suffix", () => {
    const t = vi.fn((key: string, options?: Record<string, unknown>) => {
      if (key === "spending:whatChanged.headlineLeadPrefix") return "You spent ";
      if (key === "spending:whatChanged.headlineMoreSuffix") {
        return ` more than ${String(options?.priorLabel)}.`;
      }
      return key;
    });

    const headline = buildHeadline({
      periodState: { kind: "valid_comparison" },
      movers: [],
      currentTotal: 200,
      priorTotal: 100,
      priorLabel: "juil.",
      metaLabel: "",
      t,
    });

    expect(headline.fragments).toContainEqual({ type: "text", text: " more than juil." });
  });

  it.each([
    {
      current: [150, 0, 0],
      prior: [0, 130, 100],
      expectedDriver: "B",
      expectedOpposite: "A",
      prefix: "spending:whatChanged.headlineDropDriverPrefix",
    },
    {
      current: [0, 130, 100],
      prior: [150, 0, 0],
      expectedDriver: "B",
      expectedOpposite: "A",
      prefix: "spending:whatChanged.headlineRiseDriverPrefix",
    },
  ])(
    "selects a driver aligned with the total when the largest movement is opposite: $prefix",
    ({ current, prior, expectedDriver, expectedOpposite, prefix }) => {
      const currentTotal = current.reduce((sum, value) => sum + value, 0);
      const priorTotal = prior.reduce((sum, value) => sum + value, 0);
      const movers = describeCategories(
        current.map((value, index) => ({
          id: String.fromCharCode(65 + index),
          current: value,
          prior: prior[index],
        })),
        currentTotal,
        priorTotal,
      ).map((descriptor) => ({ ...descriptor, name: descriptor.id }));

      const headline = buildHeadline({
        periodState: { kind: "valid_comparison" },
        movers,
        currentTotal,
        priorTotal,
        priorLabel: "last month",
        metaLabel: "",
        t: (key) => key,
      });

      expect(headline.fragments).toContainEqual({ type: "text", text: prefix });
      expect(
        headline.fragments
          .filter((fragment) => fragment.type === "mover")
          .map((fragment) => fragment.descriptor.name),
      ).toEqual([expectedDriver, expectedOpposite]);
    },
  );
});
