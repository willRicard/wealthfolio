import type { ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@/test/render";
import { getProcurementItemPrices } from "@/adapters";
import { PERFORMANCE_CHART_COLORS } from "@/components/performance-chart-colors";
import { QueryKeys } from "@/lib/query-keys";
import type { ProcurementItemPrices, ProcurementItemWithStats, ProcurementPrice } from "../types";
import { ItemsCompare } from "./items-compare";

const mocks = vi.hoisted(() => ({ hidden: false, rows: [] as Record<string, number | null>[] }));
vi.mock("@/adapters", () => ({ getProcurementItemPrices: vi.fn() }));
vi.mock("@/hooks/use-balance-privacy", () => ({
  useBalancePrivacy: () => ({ isBalanceHidden: mocks.hidden }),
}));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock("@wealthfolio/ui/components/ui/chart", () => ({
  ChartContainer: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ChartTooltip: () => null,
  ChartLegend: () => null,
  ChartLegendContent: () => null,
}));
vi.mock("recharts", () => ({
  LineChart: ({ data, children }: { data: typeof mocks.rows; children: ReactNode }) => {
    mocks.rows = data;
    return <div data-testid="chart">{children}</div>;
  },
  Line: ({
    stroke,
    dataKey,
    connectNulls,
  }: {
    stroke: string;
    dataKey: string;
    connectNulls: boolean;
  }) => (
    <div data-testid="line" data-color={stroke} data-key={dataKey} data-connect={connectNulls} />
  ),
  XAxis: () => null,
  YAxis: () => null,
  CartesianGrid: () => null,
}));

function item(index: number, unit = "kg"): ProcurementItemWithStats {
  return {
    id: `item-${index}`,
    name: `Item ${index}`,
    unit,
    brand: null,
    memo: null,
    categoryId: "category",
    subcategoryId: null,
    rating: null,
    taxType: "INCLUDED",
    aliases: [],
    photo: null,
    createdAt: "2026-01-01",
    updatedAt: "2026-01-01",
    baseCurrency: "USD",
    minPrice: "999",
    avgPrice: "999",
    latestPrice: "999",
    latestDate: null,
    purchaseCount: 0,
    unconvertedCount: 0,
  };
}
function point(date: string, value: string | null): ProcurementPrice {
  return {
    itemId: "item",
    activityId: date,
    date,
    shop: "Shop",
    quantity: "1",
    unitPrice: value ?? "1",
    unitPriceBase: value,
    currency: "USD",
    baseCurrency: "USD",
  };
}
function setup(
  items: ProcurementItemWithStats[],
  prices: ProcurementPrice[][],
  options: { error?: boolean; missingFx?: boolean } = {},
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  if (!options.error)
    items.forEach((entry, i) =>
      client.setQueryData<ProcurementItemPrices>([QueryKeys.PROCUREMENT_ITEM_PRICES, entry.id], {
        prices: prices[i],
        stats: { ...entry, unconvertedCount: options.missingFx ? 1 : 0 },
      }),
    );
  const onClose = vi.fn();
  render(
    <QueryClientProvider client={client}>
      <ItemsCompare items={items} onClose={onClose} />
    </QueryClientProvider>,
  );
  return { onClose };
}
beforeEach(() => {
  mocks.hidden = false;
  mocks.rows = [];
  vi.mocked(getProcurementItemPrices).mockReset();
});

it("shares cached queries and merges the union of dates with gaps", () => {
  setup(
    [item(0), item(1)],
    [[point("2026-01-01", "10"), point("2026-03-01", "20")], [point("2026-02-01", "30")]],
  );
  expect(mocks.rows).toEqual([
    { time: Date.parse("2026-01-01"), s0: 10, s1: null },
    { time: Date.parse("2026-02-01"), s0: null, s1: 30 },
    { time: Date.parse("2026-03-01"), s0: 20, s1: null },
  ]);
  expect(getProcurementItemPrices).not.toHaveBeenCalled();
  expect(screen.getAllByTestId("line").every((line) => line.dataset.connect === "true")).toBe(true);
  const row = screen.getByRole("row", { name: /Item 0/ });
  expect(within(row).getByText("$15.00")).toBeInTheDocument();
  expect(within(row).getByText("100.00%")).toBeInTheDocument();
});

it("labels mixed units and warns about prices missing FX", () => {
  setup([item(0), item(1, "L")], [[point("2026-01-01", "10")], []], { missingFx: true });
  expect(screen.getByText("mixedUnits")).toBeInTheDocument();
  expect(screen.getByText("missingFx")).toBeInTheDocument();
});

it("cycles colors for selections larger than eight items", () => {
  setup(
    Array.from({ length: 9 }, (_, i) => item(i)),
    Array.from({ length: 9 }, () => [point("2026-01-01", "10")]),
  );
  const lines = screen.getAllByTestId("line");
  expect(lines).toHaveLength(9);
  expect(lines[0].dataset.color).toBe(PERFORMANCE_CHART_COLORS[0]);
  expect(lines[8].dataset.color).toBe(PERFORMANCE_CHART_COLORS[0]);
});

it("hides the chart, prices and percentage changes with balance privacy", () => {
  mocks.hidden = true;
  setup([item(0), item(1)], [[point("2026-01-01", "10")], [point("2026-01-01", "20")]]);
  expect(screen.queryByTestId("chart")).not.toBeInTheDocument();
  expect(screen.queryByText(/\$/)).not.toBeInTheDocument();
  expect(screen.queryByText(/%/)).not.toBeInTheDocument();
  expect(screen.getAllByText("••••")).toHaveLength(9);
});

it("surfaces a failed series without showing partial statistics", async () => {
  vi.mocked(getProcurementItemPrices).mockRejectedValue(new Error("Unavailable"));
  setup([item(0), item(1)], [], { error: true });
  expect(screen.getByRole("status")).toHaveTextContent("loading");
  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("loadFailed"));
  expect(screen.queryByRole("table")).not.toBeInTheDocument();
});

it("filters chart and statistics together and restores all history", () => {
  const recent = new Date().toISOString();
  setup(
    [item(0), item(1)],
    [[point("2000-01-01", "5"), point(recent, "20")], [point("2000-01-01", "30")]],
  );
  fireEvent.click(screen.getByRole("button", { name: "ui:interval.label.1M" }));
  expect(mocks.rows).toHaveLength(1);
  expect(within(screen.getByRole("row", { name: /Item 0/ })).getAllByText("$20.00")).toHaveLength(
    3,
  );
  expect(within(screen.getByRole("row", { name: /Item 1/ })).getAllByText("—")).toHaveLength(4);
  fireEvent.click(screen.getByRole("button", { name: "ui:interval.label.ALL" }));
  expect(mocks.rows).toHaveLength(2);
});

it("distinguishes no purchases from no converted prices and closes", () => {
  const { onClose } = setup([item(0), item(1)], [[], []]);
  expect(screen.getByText("noPurchases")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "close" }));
  expect(onClose).toHaveBeenCalledOnce();
});

it("reports no converted prices rather than no purchases", () => {
  setup([item(0), item(1)], [[point("2026-01-01", null)], []], { missingFx: true });
  expect(screen.getByText("noPricesInRange")).toBeInTheDocument();
  expect(screen.getByText("missingFx")).toBeInTheDocument();
});
