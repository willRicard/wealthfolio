import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  Button,
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  Icons,
  MoneyInput,
  Popover,
  PopoverContent,
  PopoverTrigger,
  QuantityInput,
} from "@wealthfolio/ui";
import { getProcurementItems } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type {
  ProcurementItemWithStats,
  ProcurementPurchase,
  SaveProcurementPurchase,
} from "../types";

export interface ActivityPurchaseRow {
  key: string;
  itemId: string;
  quantity?: number;
  subtotal?: number;
}

let rowSequence = 0;
const nextKey = () => `purchase-${(rowSequence += 1)}`;

export const newPurchaseRow = (): ActivityPurchaseRow => ({ key: nextKey(), itemId: "" });

export const purchaseRowsFrom = (purchases: ProcurementPurchase[]): ActivityPurchaseRow[] =>
  purchases.map((purchase) => ({
    key: nextKey(),
    itemId: purchase.itemId,
    quantity: Number(purchase.quantity),
    subtotal: Number(purchase.subtotal),
  }));

export const hasIncompletePurchaseRow = (rows: ActivityPurchaseRow[]): boolean =>
  rows.some(
    (row) =>
      !!row.itemId &&
      (row.quantity === undefined || row.quantity <= 0 || row.subtotal === undefined),
  );

export const purchaseRowsToSave = (
  rows: ActivityPurchaseRow[],
  activityId: string,
): SaveProcurementPurchase[] =>
  rows
    .filter((row) => row.itemId)
    .map((row) => ({
      itemId: row.itemId,
      activityId,
      quantity: String(row.quantity ?? 0),
      subtotal: String(row.subtotal ?? 0),
    }));

interface ActivityPurchasesProps {
  rows: ActivityPurchaseRow[];
  onChange: (rows: ActivityPurchaseRow[]) => void;
  currency?: string;
  categoryId?: string;
  disabled?: boolean;
  showErrors?: boolean;
}

export function ActivityPurchases({
  rows,
  onChange,
  currency,
  categoryId,
  disabled,
  showErrors,
}: ActivityPurchasesProps) {
  const { t } = useTranslation("procurement");
  const { data: items = [], isError } = useQuery({
    queryKey: [QueryKeys.PROCUREMENT_ITEMS],
    queryFn: getProcurementItems,
  });
  const itemsById = useMemo(() => new Map(items.map((item) => [item.id, item])), [items]);
  const update = (index: number, change: Partial<ActivityPurchaseRow>) =>
    onChange(rows.map((row, i) => (i === index ? { ...row, ...change } : row)));
  const move = (index: number, delta: number) => {
    const next = [...rows];
    const [row] = next.splice(index, 1);
    next.splice(index + delta, 0, row);
    onChange(next);
  };
  const invalid = (row: ActivityPurchaseRow, value: number | undefined) =>
    !!showErrors && !!row.itemId && (value === undefined || value <= 0);

  return (
    <section className="space-y-3">
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-sm font-medium">{t("purchases")}</span>
        {currency && <span className="text-muted-foreground text-xs">{currency}</span>}
      </div>
      {isError && (
        <p role="alert" className="text-destructive text-sm">
          {t("loadFailed")}
        </p>
      )}
      <table className="w-full text-sm">
        <thead className="text-muted-foreground">
          <tr className="text-left text-xs">
            <th className="font-normal">{t("item")}</th>
            <th className="w-16 px-1 text-right font-normal">{t("quantity")}</th>
            <th className="w-6 px-1" />
            <th className="w-20 px-1 text-right font-normal">{t("subtotal")}</th>
            <th className="w-[4.5rem]" />
          </tr>
        </thead>
        <tbody>
          {rows.map((row, index) => (
            <tr key={row.key} className="border-b">
              <td className="min-w-0 py-1 pr-2">
                <ItemPicker
                  items={items}
                  selected={itemsById.get(row.itemId)}
                  categoryId={categoryId}
                  takenIds={rows.filter((_, i) => i !== index).map((other) => other.itemId)}
                  disabled={disabled}
                  invalid={!!showErrors && !row.itemId}
                  onSelect={(itemId) => update(index, { itemId, quantity: row.quantity ?? 1 })}
                />
              </td>
              <td className="px-1 py-1">
                <QuantityInput
                  aria-label={`${t("quantity")} ${index + 1}`}
                  className="h-8 px-2"
                  maxDecimalPlaces={4}
                  value={row.quantity ?? ""}
                  disabled={disabled}
                  aria-invalid={invalid(row, row.quantity) || undefined}
                  onValueChange={(quantity) => update(index, { quantity })}
                />
              </td>
              <td className="text-muted-foreground px-1 py-1 text-xs">
                {itemsById.get(row.itemId)?.unit ?? ""}
              </td>
              <td className="px-1 py-1">
                <MoneyInput
                  aria-label={`${t("subtotal")} ${index + 1}`}
                  className="h-8 px-2"
                  value={row.subtotal ?? ""}
                  disabled={disabled}
                  aria-invalid={
                    (!!showErrors && !!row.itemId && row.subtotal === undefined) || undefined
                  }
                  onValueChange={(subtotal) => update(index, { subtotal })}
                />
              </td>
              <td>
                <div className="flex items-center justify-end">
                  <NudgeButton
                    label={t("moveUp")}
                    disabled={disabled || index === 0}
                    onClick={() => move(index, -1)}
                  >
                    <Icons.ChevronUp className="size-3.5" aria-hidden />
                  </NudgeButton>
                  <NudgeButton
                    label={t("moveDown")}
                    disabled={disabled || index === rows.length - 1}
                    onClick={() => move(index, 1)}
                  >
                    <Icons.ChevronDown className="size-3.5" aria-hidden />
                  </NudgeButton>
                  <NudgeButton
                    label={`${t("delete")} ${index + 1}`}
                    disabled={disabled}
                    onClick={() => onChange(rows.filter((_, i) => i !== index))}
                  >
                    <Icons.Trash className="size-3.5" aria-hidden />
                  </NudgeButton>
                </div>
              </td>
            </tr>
          ))}
          <tr>
            <td colSpan={5} className="pt-2">
              <Button
                type="button"
                variant="ghost"
                className="text-muted-foreground w-full justify-start border border-dashed"
                disabled={disabled}
                onClick={() => onChange([...rows, newPurchaseRow()])}
              >
                <Icons.Plus className="mr-2 size-4" />
                {t("addPurchase")}
              </Button>
            </td>
          </tr>
        </tbody>
      </table>
      {showErrors && hasIncompletePurchaseRow(rows) && (
        <p role="alert" className="text-destructive text-sm">
          {t("purchaseIncomplete")}
        </p>
      )}
    </section>
  );
}

function NudgeButton({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:ring-ring flex size-6 items-center justify-center rounded-full focus-visible:outline-none focus-visible:ring-2 disabled:opacity-30"
    >
      {children}
    </button>
  );
}

function ItemPicker({
  items,
  selected,
  categoryId,
  takenIds,
  disabled,
  invalid,
  onSelect,
}: {
  items: ProcurementItemWithStats[];
  selected?: ProcurementItemWithStats;
  categoryId?: string;
  takenIds: string[];
  disabled?: boolean;
  invalid?: boolean;
  onSelect: (itemId: string) => void;
}) {
  const { t } = useTranslation("procurement");
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const taken = useMemo(() => new Set(takenIds.filter(Boolean)), [takenIds]);
  const options = useMemo(() => {
    if (search.trim() || !categoryId) return items;
    const inCategory = items.filter(
      (item) => item.categoryId === categoryId || item.subcategoryId === categoryId,
    );
    return inCategory.length ? inCategory : items;
  }, [items, search, categoryId]);

  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) setSearch("");
      }}
    >
      <PopoverTrigger
        render={
          <button
            type="button"
            disabled={disabled}
            aria-invalid={invalid || undefined}
            className="border-input bg-input-bg dark:bg-input/30 hover:bg-accent/30 aria-[invalid]:border-destructive flex h-8 w-full items-center justify-between gap-1 rounded-md border px-2 text-left text-sm disabled:opacity-50"
          >
            <span className={selected ? "truncate" : "text-muted-foreground truncate"}>
              {selected?.name ?? t("pickItem")}
            </span>
            <Icons.ChevronDown className="size-3.5 shrink-0 opacity-50" aria-hidden />
          </button>
        }
      />
      <PopoverContent className="w-70 p-0" align="start">
        <Command>
          <CommandInput placeholder={t("searchItems")} value={search} onValueChange={setSearch} />
          <CommandList>
            <CommandEmpty>{t("noItems")}</CommandEmpty>
            <CommandGroup>
              {options.map((item) => (
                <CommandItem
                  key={item.id}
                  value={`${item.name} ${item.aliases.join(" ")} ${item.id}`}
                  disabled={taken.has(item.id) && item.id !== selected?.id}
                  onSelect={() => {
                    onSelect(item.id);
                    setOpen(false);
                    setSearch("");
                  }}
                  className="flex items-center gap-2"
                >
                  <span className="truncate">{item.name}</span>
                  {item.brand && (
                    <span className="text-muted-foreground truncate text-xs">{item.brand}</span>
                  )}
                  {item.id === selected?.id && (
                    <Icons.Check className="text-muted-foreground ml-auto size-3.5" />
                  )}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
