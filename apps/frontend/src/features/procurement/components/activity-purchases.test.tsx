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
        subtotal: "12.25",
        createdAt: "",
        updatedAt: "",
      },
    ]);
    expect(purchaseRowsToSave([...rows, newPurchaseRow()], "saved")).toEqual([
      { activityId: "saved", itemId: "item", quantity: "0.5", subtotal: "12.25" },
    ]);
  });
  it.each([
    [{ itemId: "", key: "empty" }, false],
    [{ itemId: "item", key: "missing" }, true],
    [{ itemId: "item", key: "zero", quantity: 0, subtotal: 1 }, true],
    [{ itemId: "item", key: "negative", quantity: -1, subtotal: 1 }, true],
    [{ itemId: "item", key: "missing-subtotal", quantity: 1 }, true],
    [{ itemId: "item", key: "free", quantity: 1, subtotal: 0 }, false],
  ] as const)("validates %j", (row, incomplete) => {
    expect(hasIncompletePurchaseRow([row])).toBe(incomplete);
  });
});

const items = [
  { id: "coffee", name: "Coffee", aliases: ["Morning brew"], categoryId: "groceries", unit: "kg" },
  { id: "tea", name: "Tea", aliases: [], categoryId: "groceries", unit: "袋" },
  { id: "chair", name: "Chair", aliases: [], categoryId: "home", unit: "個" },
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
          { key: "1", itemId: "tea", quantity: 1, subtotal: 2 },
          { key: "2", itemId: "" },
        ]}
        onChange={onChange}
        categoryId={categoryId}
      />
    </QueryClientProvider>,
  );
  return onChange;
}

it("scopes options by category and searches aliases across categories", async () => {
  const user = userEvent.setup();
  const onChange = picker("home");
  await user.click(screen.getByRole("button", { name: "Pick an item" }));
  expect(screen.getByRole("option", { name: "Chair" })).toBeInTheDocument();
  expect(screen.queryByRole("option", { name: "Coffee" })).not.toBeInTheDocument();
  await user.type(screen.getByPlaceholderText("Search items…"), "Morning brew");
  await user.click(screen.getByRole("option", { name: "Coffee" }));
  expect(onChange).toHaveBeenCalledWith([
    { key: "1", itemId: "tea", quantity: 1, subtotal: 2 },
    { key: "2", itemId: "coffee", quantity: 1 },
  ]);
});

it("shows items already used by another row as disabled options", async () => {
  const user = userEvent.setup();
  picker();
  await user.click(screen.getByRole("button", { name: "Pick an item" }));
  expect(screen.getByRole("option", { name: "Tea" })).toHaveAttribute("aria-disabled", "true");
  expect(screen.getByRole("option", { name: "Chair" })).toHaveAttribute("aria-disabled", "false");
});

it("shows the selected item's unit between the quantity and the subtotal", async () => {
  const user = userEvent.setup();
  picker();
  const cells = screen.getAllByRole("row")[1].querySelectorAll("td");
  expect(cells[2]).toHaveTextContent("袋");
  expect(cells[1].querySelector("input")).toHaveAccessibleName("Qty 1");
  expect(cells[3].querySelector("input")).toHaveAccessibleName("Subtotal 1");
  expect(screen.getAllByRole("row")[2].querySelectorAll("td")[2]).toHaveTextContent("");
  await user.click(screen.getByRole("button", { name: "Pick an item" }));
  await user.click(screen.getByRole("option", { name: "Coffee" }));
  expect(screen.getAllByRole("row")[1].querySelectorAll("td")[2]).toHaveTextContent("袋");
});

it("falls back to all items when the category has no matches", async () => {
  const user = userEvent.setup();
  picker("missing-category");
  await user.click(screen.getByRole("button", { name: "Pick an item" }));
  expect(screen.getByRole("option", { name: "Coffee" })).toBeInTheDocument();
  expect(screen.getByRole("option", { name: "Chair" })).toBeInTheDocument();
  expect(screen.getByRole("option", { name: "Tea" })).toHaveAttribute("aria-disabled", "true");
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
