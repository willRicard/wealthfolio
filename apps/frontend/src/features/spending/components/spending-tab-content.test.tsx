import { render, screen } from "@/test/render";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import SpendingTabContent from "./spending-tab-content";

const { reportQuery } = vi.hoisted(() => ({ reportQuery: vi.fn() }));

vi.mock("@/lib/settings-provider", () => ({
  useSettingsContext: () => ({ settings: { baseCurrency: "USD", timezone: "UTC" } }),
}));
vi.mock("@/hooks/use-balance-privacy", () => ({
  useBalancePrivacy: () => ({ isBalanceHidden: false }),
}));
vi.mock("@/hooks/use-accounts", () => ({ useAccounts: () => ({ accounts: [] }) }));
vi.mock("@/hooks/use-taxonomies", () => ({ useTaxonomy: () => ({}) }));
vi.mock("../hooks/use-spending-settings", () => ({
  useSpendingSettings: () => ({ accountIds: ["cash"], excludedCategoryIds: [], isLoading: false }),
}));
vi.mock("../hooks/use-spending-report", () => ({
  useSpendingReport: (request: unknown, enabled: boolean, currency: string) => {
    reportQuery(request, enabled, currency);
    return {};
  },
}));
vi.mock("../hooks/use-cash-activities", () => ({
  useCashActivities: () => ({}),
  useUncategorizedCount: () => ({}),
}));
vi.mock("../hooks/use-budget", () => ({ useBudget: () => ({}) }));
vi.mock("../hooks/use-categorization-rules", () => ({ useCategorizationRules: () => ({}) }));
vi.mock("@/pages/dashboard/balance", () => ({ default: () => null }));
vi.mock("./budget-line-chart-card", () => ({ BudgetLineChartCard: () => null }));
vi.mock("./events-card", () => ({ EventsCard: () => null }));
vi.mock("./recent-activity-card", () => ({ RecentActivityCard: () => null }));
vi.mock("recharts", async (importOriginal) => ({
  ...(await importOriginal<typeof import("recharts")>()),
  ResponsiveContainer: () => null,
}));

beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("Spending tab period comparisons", () => {
  it.each([
    ["2026-09-29T12:00:00Z", "2026-08-29"],
    ["2026-09-30T12:00:00Z", "2026-08-31"],
    ["2024-02-29T12:00:00Z", "2024-01-31"],
    ["2026-03-31T12:00:00Z", "2026-02-28"],
  ])("compares MTD at %s with the matching prior month through %s", (now, priorEnd) => {
    vi.setSystemTime(new Date(now));
    localStorage.setItem("dashboard-period", JSON.stringify({ kind: "spending", code: "MTD" }));
    render(
      <MemoryRouter>
        <SpendingTabContent />
      </MemoryRouter>,
    );

    expect(reportQuery).toHaveBeenCalledWith(
      {
        startDate: `${priorEnd.slice(0, 7)}-01T00:00:00.000Z`,
        endDate: `${priorEnd}T23:59:59.999Z`,
      },
      true,
      "USD",
    );
  });

  it.each([
    { kind: "spending", code: "YTD" },
    { kind: "month", monthKey: "2025-02" },
    { kind: "range", from: "2025-03-08", to: "2025-03-10" },
  ])("links to insights without overriding the shared $kind selection", (pick) => {
    vi.setSystemTime(new Date("2026-09-29T12:00:00Z"));
    localStorage.setItem("dashboard-period", JSON.stringify(pick));
    render(
      <MemoryRouter>
        <SpendingTabContent />
      </MemoryRouter>,
    );

    const insightLinks = screen
      .getAllByRole("link")
      .filter((link) => link.getAttribute("href")?.startsWith("/spending/insights"));
    expect(insightLinks.length).toBeGreaterThan(0);
    for (const link of insightLinks) {
      const url = new URL(link.getAttribute("href")!, "http://localhost");
      expect([...url.searchParams.keys()]).toEqual(["stage"]);
    }
    expect(JSON.parse(localStorage.getItem("dashboard-period")!)).toEqual(pick);
  });
});
