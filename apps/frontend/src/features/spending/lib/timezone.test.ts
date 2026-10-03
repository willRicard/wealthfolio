import { describe, expect, it } from "vitest";

import {
  addCalendarMonths,
  getZonedDateParts,
  matchingPreviousMonthDates,
  zonedCalendarDateBoundaryToDate,
} from "./timezone";

describe("spending timezone helpers", () => {
  it("converts configured timezone day boundaries to UTC instants", () => {
    const date = { year: 2026, month: 5, day: 1 };

    expect(zonedCalendarDateBoundaryToDate(date, "start", "America/Toronto").toISOString()).toBe(
      "2026-05-01T04:00:00.000Z",
    );
    expect(zonedCalendarDateBoundaryToDate(date, "end", "America/Toronto").toISOString()).toBe(
      "2026-05-02T03:59:59.999Z",
    );
  });

  it("uses the configured timezone when extracting calendar dates", () => {
    const instant = new Date("2026-05-01T02:00:00.000Z");

    expect(getZonedDateParts(instant, "America/Toronto")).toEqual({
      year: 2026,
      month: 4,
      day: 30,
    });
    expect(getZonedDateParts(instant, "Europe/London")).toEqual({
      year: 2026,
      month: 5,
      day: 1,
    });
  });

  it("clamps month arithmetic to the destination month", () => {
    expect(addCalendarMonths({ year: 2026, month: 3, day: 31 }, -1)).toEqual({
      year: 2026,
      month: 2,
      day: 28,
    });
  });

  it.each([
    [
      { year: 2026, month: 10, day: 3 },
      { year: 2026, month: 9, day: 3 },
    ],
    [
      { year: 2026, month: 10, day: 31 },
      { year: 2026, month: 9, day: 30 },
    ],
    [
      { year: 2026, month: 3, day: 31 },
      { year: 2026, month: 2, day: 28 },
    ],
    [
      { year: 2026, month: 1, day: 3 },
      { year: 2025, month: 12, day: 3 },
    ],
  ])("matches the previous calendar month through %o", (end, expectedEnd) => {
    const start = { ...end, day: 1 };
    const result = matchingPreviousMonthDates(start, end);

    expect(result.start).toEqual({ ...expectedEnd, day: 1 });
    expect(result.end).toEqual(expectedEnd);
  });
});
