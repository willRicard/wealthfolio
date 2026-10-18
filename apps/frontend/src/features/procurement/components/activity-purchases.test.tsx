import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import userEvent from "@testing-library/user-event";
import { render, screen } from "@/test/render";
import { QueryKeys } from "@/lib/query-keys";
import type { ProcurementItemWithStats } from "../types";
import { beforeAll, afterAll, describe, expect, it, vi } from "vitest";
import {
  ActivityPurchases,
  hasIncompletePurchaseRow,
  newPurchaseRow,
  purchaseRowsFrom,
  purchaseRowsToSave,
} from "./activity-purchases";

describe("purchase rows", () => {
  it("assigns unique UI keys and round-trips decimal values", () => {
    expect(newPurchaseRow().key).not.toBe(newPurchaseRow().key);
    const rows = purchaseRowsFrom([
      {
        activityId: "activity",
        itemId: "item",
        quantity: "0.5",
        unitPrice: "12.25",
        createdAt: "",
        updatedAt: "",
      },
    ]);
    expect(purchaseRowsToSave([...rows, newPurchaseRow()], "saved")).toEqual([
      { activityId: "saved", itemId: "item", quantity: "0.5", unitPrice: "12.25" },
    ]);
  });
  it.each([
    [{ itemId: "", key: "empty" }, false],
    [{ itemId: "item", key: "missing" }, true],
    [{ itemId: "item", key: "zero", quantity: 0, unitPrice: 1 }, true],
    [{ itemId: "item", key: "negative", quantity: -1, unitPrice: 1 }, true],
    [{ itemId: "item", key: "missing-price", quantity: 1 }, true],
    [{ itemId: "item", key: "free", quantity: 1, unitPrice: 0 }, false],
  ] as const)("validates %j", (row, incomplete) => {
    expect(hasIncompletePurchaseRow([row])).toBe(incomplete);
  });
});

const items = [
  { id: "coffee", name: "Coffee", aliases: ["Morning brew"], categoryId: "groceries" },
  { id: "tea", name: "Tea", aliases: [], categoryId: "groceries" },
  { id: "chair", name: "Chair", aliases: [], categoryId: "home" },
] as ProcurementItemWithStats[];
vi.mock("@/adapters", () => ({ getProcurementItems: vi.fn(() => Promise.resolve(items)) }));

function picker(categoryId?: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData([QueryKeys.PROCUREMENT_ITEMS], items);
  const onChange = vi.fn();
  render(
    <QueryClientProvider client={client}>
      <ActivityPurchases
        rows={[
          { key: "1", itemId: "tea", quantity: 1, unitPrice: 2 },
          { key: "2", itemId: "" },
        ]}
        onChange={onChange}
        categoryId={categoryId}
      />
    </QueryClientProvider>,
  );
  return onChange;
}

it("scopes options by category, searches aliases across categories, and excludes already selected items", async () => {
  const user = userEvent.setup();
  const onChange = picker("home");
  await user.click(screen.getByRole("button", { name: "Pick an item" }));
  expect(screen.getByRole("option", { name: "Chair" })).toBeInTheDocument();
  expect(screen.queryByRole("option", { name: "Coffee" })).not.toBeInTheDocument();
  await user.type(screen.getByPlaceholderText("Search items…"), "Morning brew");
  await user.click(screen.getByRole("option", { name: "Coffee" }));
  expect(onChange).toHaveBeenCalledWith([
    { key: "1", itemId: "tea", quantity: 1, unitPrice: 2 },
    { key: "2", itemId: "coffee", quantity: 1 },
  ]);
});

it("falls back to all available items when the category has no matches", async () => {
  const user = userEvent.setup();
  picker("missing-category");
  await user.click(screen.getByRole("button", { name: "Pick an item" }));
  expect(screen.getByRole("option", { name: "Coffee" })).toBeInTheDocument();
  expect(screen.getByRole("option", { name: "Chair" })).toBeInTheDocument();
  expect(screen.queryByRole("option", { name: "Tea" })).not.toBeInTheDocument();
});

// cmdk measures its list with ResizeObserver, which jsdom does not implement.
beforeAll(() =>
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {
        /* No layout in jsdom. */
      }
      unobserve() {
        /* No layout in jsdom. */
      }
      disconnect() {
        /* No layout in jsdom. */
      }
    },
  ),
);
afterAll(() => vi.unstubAllGlobals());

const originalScrollIntoView = HTMLElement.prototype.scrollIntoView;
beforeAll(() => {
  HTMLElement.prototype.scrollIntoView = vi.fn();
});
afterAll(() => {
  HTMLElement.prototype.scrollIntoView = originalScrollIntoView;
});
