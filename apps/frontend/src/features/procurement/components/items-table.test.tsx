import { act, fireEvent, render, screen, waitFor, within } from "@/test/render";
import { afterEach, expect, it, vi } from "vitest";
import { ItemsTable } from "./items-table";
import type { ProcurementItemWithStats } from "../types";

vi.mock("./items-compare", () => ({
  ItemsCompare: ({
    items,
    onClose,
  }: {
    items: ProcurementItemWithStats[];
    onClose: () => void;
  }) => (
    <div role="dialog" aria-label="compareTitle">
      {items.map((item) => (
        <span key={item.id}>{item.name}</span>
      ))}
      <button onClick={onClose}>close</button>
    </div>
  ),
}));
vi.mock("@/hooks", () => ({ useHapticFeedback: () => ({ triggerHaptic: vi.fn() }) }));
vi.mock("@/hooks/use-taxonomies", () => ({
  useTaxonomy: () => ({ data: { categories: [] } }),
}));
vi.mock("@/hooks/use-balance-privacy", () => ({
  useBalancePrivacy: () => ({ isBalanceHidden: false }),
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: { name?: string; count?: number }) =>
      options?.name ? `${key} ${options.name}` : options?.count ? `${key} ${options.count}` : key,
  }),
}));

function item(index: number): ProcurementItemWithStats {
  return {
    id: `id-${index}`,
    name: `Item ${index}`,
    brand: "Brand",
    memo: null,
    categoryId: "category",
    subcategoryId: null,
    rating: 3,
    unit: "kg",
    taxType: "INCLUDED",
    aliases: [],
    photo: null,
    createdAt: "2026-10-01",
    updatedAt: "2026-10-01",
    baseCurrency: "USD",
    minPrice: "1",
    avgPrice: "2",
    latestPrice: "3",
    latestDate: "2026-10-01",
    purchaseCount: 1,
    unconvertedCount: 0,
  };
}
const actions = {
  pending: false,
  onView: vi.fn(),
  onEdit: vi.fn(),
  onClone: vi.fn(),
};
afterEach(() => vi.unstubAllGlobals());

it("reveals another 50 rows when the shared scroll sentinel intersects", async () => {
  let intersect: IntersectionObserverCallback | undefined;
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      constructor(callback: IntersectionObserverCallback) {
        intersect = callback;
      }
      observe() {
        // The test invokes the captured callback directly.
      }
      disconnect() {
        // This fake observer owns no native resources.
      }
    },
  );
  render(
    <ItemsTable
      {...actions}
      compact
      items={Array.from({ length: 101 }, (_, i) => item(i))}
      onDeleteMany={vi.fn()}
    />,
  );
  expect(screen.getAllByRole("row")).toHaveLength(51);
  expect(screen.queryByRole("button", { name: "Item 50" })).not.toBeInTheDocument();
  const search = screen.getByRole("textbox");
  fireEvent.change(search, { target: { value: "Item 100" } });
  fireEvent.keyDown(search, { key: "Enter" });
  expect(screen.getByRole("button", { name: "Item 100" })).toBeInTheDocument();
  expect(screen.getAllByRole("row")).toHaveLength(2);
  fireEvent.change(search, { target: { value: "" } });
  fireEvent.keyDown(search, { key: "Enter" });
  expect(screen.getAllByRole("row")).toHaveLength(51);
  await waitFor(() => expect(intersect).toBeDefined());
  act(() =>
    intersect!([{ isIntersecting: true } as IntersectionObserverEntry], {} as IntersectionObserver),
  );
  expect(screen.getAllByRole("row")).toHaveLength(101);
});

it("keeps selection attached to the item after rows change and deletes only that selection", async () => {
  const onDeleteMany = vi.fn().mockResolvedValue(undefined);
  const first = item(1);
  const second = item(2);
  const { rerender } = render(
    <ItemsTable {...actions} items={[first, second]} onDeleteMany={onDeleteMany} compact />,
  );
  fireEvent.click(screen.getByRole("checkbox", { name: "selectItem Item 2" }));
  rerender(<ItemsTable {...actions} items={[second]} onDeleteMany={onDeleteMany} compact />);
  expect(screen.getByRole("checkbox", { name: "selectItem Item 2" })).toBeChecked();
  fireEvent.click(screen.getByRole("button", { name: "delete" }));
  const confirmation = screen.getByRole("alertdialog");
  fireEvent.click(within(confirmation).getByRole("button", { name: "delete" }));
  await waitFor(() => expect(onDeleteMany).toHaveBeenCalledWith([second]));
  await waitFor(() =>
    expect(screen.getByRole("checkbox", { name: "selectItem Item 2" })).not.toBeChecked(),
  );
});

it("renders every price per unit and keeps an unpriced item as a dash", () => {
  const priced = item(1);
  const unpriced = { ...item(2), minPrice: null, avgPrice: null, latestPrice: null };
  render(<ItemsTable {...actions} items={[priced, unpriced]} onDeleteMany={vi.fn()} />);
  const row = screen.getByRole("row", { name: /Item 1/ });
  for (const amount of ["$1.00", "$2.00", "$3.00"]) {
    expect(within(row).getByText(amount).parentElement).toHaveTextContent(`${amount}/kg`);
  }
  expect(within(screen.getByRole("row", { name: /Item 2/ })).queryByText("/kg")).toBeNull();
});

it("shows selection on a separate line and clears it without deleting items", () => {
  const onDeleteMany = vi.fn();
  render(<ItemsTable {...actions} items={[item(1), item(2)]} onDeleteMany={onDeleteMany} />);
  expect(screen.queryByRole("button", { name: "ui:dataTable.columns" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("checkbox", { name: "selectAll" }));
  const selection = screen.getByRole("region", { name: "spending:transactions.bulkActions" });
  expect(within(selection).getByText("selected 2")).toBeInTheDocument();
  fireEvent.click(within(selection).getByRole("button", { name: "common:clear" }));
  expect(
    screen.queryByRole("region", { name: "spending:transactions.bulkActions" }),
  ).not.toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "selectItem Item 1" })).not.toBeChecked();
  expect(screen.getByRole("checkbox", { name: "selectItem Item 2" })).not.toBeChecked();
  expect(onDeleteMany).not.toHaveBeenCalled();
});

it("compares only the selected items, requiring at least two", () => {
  render(<ItemsTable {...actions} items={[item(1), item(2), item(3)]} onDeleteMany={vi.fn()} />);
  fireEvent.click(screen.getByRole("checkbox", { name: "selectItem Item 1" }));
  expect(screen.getByRole("button", { name: "compare" })).toBeDisabled();
  fireEvent.click(screen.getByRole("checkbox", { name: "selectItem Item 2" }));
  fireEvent.click(screen.getByRole("button", { name: "compare" }));
  const dialog = screen.getByRole("dialog", { name: "compareTitle" });
  expect(within(dialog).getByText("Item 1")).toBeInTheDocument();
  expect(within(dialog).getByText("Item 2")).toBeInTheDocument();
  expect(within(dialog).queryByText("Item 3")).not.toBeInTheDocument();
  fireEvent.click(within(dialog).getByRole("button", { name: "close" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "selectItem Item 1" })).toBeChecked();
});
