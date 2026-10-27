import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  type ColumnDef,
  type ColumnFiltersState,
  type Updater,
  flexRender,
  getCoreRowModel,
  getFilteredRowModel,
  getFacetedRowModel,
  getFacetedUniqueValues,
  useReactTable,
} from "@tanstack/react-table";
import { Checkbox, Button, Icons, StarRatingDisplay, useDateFormatting } from "@wealthfolio/ui";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@wealthfolio/ui/components/ui/table";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@wealthfolio/ui/components/ui/alert-dialog";
import { DataTableToolbar } from "@wealthfolio/ui/components/ui/data-table/data-table-toolbar";
import { InfiniteScrollTrigger } from "@/components/infinite-scroll-trigger";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { useTaxonomy } from "@/hooks/use-taxonomies";
import { ItemsCompare } from "./items-compare";
import { ItemCompact } from "./item-compact";
import { ItemActions } from "./item-actions";
import { UnitPrice } from "./unit-price";
import type { ProcurementItemWithStats } from "../types";

const PAGE_SIZE = 50;

interface ItemsTableProps {
  items: ProcurementItemWithStats[];
  compact?: boolean;
  pending: boolean;
  onView: (item: ProcurementItemWithStats) => void;
  onEdit: (item: ProcurementItemWithStats) => void;
  onClone: (item: ProcurementItemWithStats) => void;
  onDeleteMany: (items: ProcurementItemWithStats[]) => Promise<void>;
}
export function ItemsTable(props: ItemsTableProps) {
  const { items, compact, onView } = props;
  const { t } = useTranslation("procurement");
  const [visibleCount, setVisibleCount] = useState(PAGE_SIZE);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([]);
  const [globalFilter, setGlobalFilter] = useState("");
  const [comparing, setComparing] = useState<ProcurementItemWithStats[]>();
  const [deleting, setDeleting] = useState<ProcurementItemWithStats[]>();
  const toggleItem = (id: string, checked: boolean) =>
    setSelectedIds((previous) => {
      const next = new Set(previous);
      if (checked) next.add(id);
      else next.delete(id);
      return next;
    });
  const { isBalanceHidden } = useBalancePrivacy();
  const { formatDate } = useDateFormatting();
  const { data: taxonomy } = useTaxonomy("procurement_categories");
  const categories = taxonomy?.categories;
  const categoryById = useMemo(() => new Map(categories?.map((c) => [c.id, c])), [categories]);
  const categoryName = (id: string | null) => (id ? (categoryById.get(id)?.name ?? id) : "—");
  const price = (value: string | null, item: ProcurementItemWithStats) => (
    <UnitPrice
      value={value}
      currency={item.baseCurrency}
      unit={item.unit}
      isHidden={isBalanceHidden}
    />
  );
  const date = (value: string | null) => (value ? formatDate(value) : "—");
  const category = (item: ProcurementItemWithStats) => {
    const entry = categoryById.get(item.subcategoryId ?? item.categoryId);
    return (
      <span
        className="inline-flex max-w-full items-center gap-1 rounded-full px-2 py-0.5 text-xs"
        style={{
          backgroundColor: entry?.color ? `${entry.color}20` : undefined,
          color: entry?.color ?? undefined,
        }}
      >
        <span
          className="size-2 shrink-0 rounded-full"
          style={{ backgroundColor: entry?.color ?? "var(--muted-foreground)" }}
        />
        <span className="truncate">{entry?.name ?? "—"}</span>
      </span>
    );
  };
  const columns: ColumnDef<ProcurementItemWithStats>[] = [
    {
      id: "select",
      header: ({ table }) => {
        const visible = table.getRowModel().rows.slice(0, visibleCount);
        const allSelected =
          visible.length > 0 && visible.every((row) => selectedIds.has(row.original.id));
        const someSelected = visible.some((row) => selectedIds.has(row.original.id));
        return (
          <Checkbox
            aria-label={t("selectAll")}
            checked={allSelected}
            indeterminate={someSelected && !allSelected}
            onCheckedChange={(checked) =>
              setSelectedIds((previous) => {
                const next = new Set(previous);
                for (const row of visible) {
                  if (checked) next.add(row.original.id);
                  else next.delete(row.original.id);
                }
                return next;
              })
            }
          />
        );
      },
      cell: ({ row }) => (
        <Checkbox
          aria-label={t("selectItem", { name: row.original.name })}
          checked={selectedIds.has(row.original.id)}
          onCheckedChange={(checked) => toggleItem(row.original.id, !!checked)}
        />
      ),
    },
    {
      accessorKey: "name",
      header: t("name"),
      cell: ({ row }) =>
        compact ? (
          <ItemCompact
            item={row.original}
            category={category(row.original)}
            actions={
              <ItemActions
                item={row.original}
                compact={compact}
                pending={props.pending}
                onView={onView}
                onEdit={props.onEdit}
                onClone={props.onClone}
                onDelete={(item) => setDeleting([item])}
              />
            }
            onView={onView}
          />
        ) : (
          <button
            className="text-left font-medium hover:underline"
            onClick={() => onView(row.original)}
          >
            {row.original.name}
          </button>
        ),
    },
    { accessorKey: "brand", header: t("brand"), cell: ({ row }) => row.original.brand ?? "—" },
    {
      accessorKey: "categoryId",
      header: t("category"),
      cell: ({ row }) => category(row.original),
      filterFn: (row, id, value: string[]) => value.includes(row.getValue(id)),
    },
    {
      accessorKey: "subcategoryId",
      header: t("subcategory"),
      cell: ({ row }) => categoryName(row.original.subcategoryId),
      filterFn: (row, id, value: string[]) => value.includes(row.getValue(id)),
    },
    {
      accessorKey: "rating",
      header: t("rating"),
      cell: ({ row }) =>
        row.original.rating === null ? (
          "—"
        ) : (
          <StarRatingDisplay rating={row.original.rating} showText={false} />
        ),
    },
    ...(["minPrice", "avgPrice", "latestPrice"] as const).map(
      (key): ColumnDef<ProcurementItemWithStats> => ({
        id: key,
        accessorFn: (row) => (row[key] === null ? undefined : Number(row[key])),
        header: t(key),
        cell: ({ row }) => price(row.original[key], row.original),
      }),
    ),
    {
      accessorKey: "latestDate",
      header: t("date"),
      cell: ({ row }) => date(row.original.latestDate),
    },
    {
      id: "actions",
      cell: ({ row }) => (
        <ItemActions
          item={row.original}
          compact={compact}
          pending={props.pending}
          onView={onView}
          onEdit={props.onEdit}
          onClone={props.onClone}
          onDelete={(item) => setDeleting([item])}
        />
      ),
    },
  ];
  const table = useReactTable({
    data: items,
    columns,
    state: {
      globalFilter,
      columnFilters,
      columnVisibility: compact
        ? {
            brand: false,
            categoryId: false,
            subcategoryId: false,
            rating: false,
            minPrice: false,
            avgPrice: false,
            latestPrice: false,
            latestDate: false,
            actions: false,
          }
        : { subcategoryId: false },
    },
    onGlobalFilterChange: (updater: Updater<string>) => {
      setGlobalFilter(updater);
      setVisibleCount(PAGE_SIZE);
    },
    onColumnFiltersChange: (updater) => {
      setColumnFilters(updater);
      setVisibleCount(PAGE_SIZE);
    },
    getCoreRowModel: getCoreRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getFacetedRowModel: getFacetedRowModel(),
    getFacetedUniqueValues: getFacetedUniqueValues(),
  });
  const filteredRows = table.getRowModel().rows;
  const visibleRows = filteredRows.slice(0, visibleCount);
  const selectedItems = filteredRows
    .filter((row) => selectedIds.has(row.original.id))
    .map((row) => row.original);
  return (
    <div className="space-y-2">
      <DataTableToolbar
        table={table}
        searchBy="name"
        filters={[
          {
            id: "categoryId",
            title: t("category"),
            options: (categories ?? [])
              .filter((c) => !c.parentId)
              .map((c) => ({ value: c.id, label: c.name })),
          },
          {
            id: "subcategoryId",
            title: t("subcategory"),
            options: (categories ?? [])
              .filter((c) => c.parentId)
              .map((c) => ({ value: c.id, label: c.name })),
          },
        ]}
      />
      {selectedItems.length > 0 && (
        <ItemsSelectionActions
          count={selectedItems.length}
          pending={props.pending}
          onCompare={() => setComparing(selectedItems)}
          onDelete={() => setDeleting(selectedItems)}
          onClear={() => setSelectedIds(new Set())}
        />
      )}
      <div className="rounded-md border">
        <Table>
          <TableHeader className="bg-muted/50">
            {table.getHeaderGroups().map((group) => (
              <TableRow key={group.id}>
                {group.headers.map((header) => (
                  <TableHead key={header.id}>
                    {header.isPlaceholder
                      ? null
                      : flexRender(header.column.columnDef.header, header.getContext())}
                  </TableHead>
                ))}
              </TableRow>
            ))}
          </TableHeader>
          <TableBody>
            {visibleRows.length ? (
              visibleRows.map((row) => (
                <TableRow
                  key={row.original.id}
                  className="group/row"
                  data-state={selectedIds.has(row.original.id) ? "selected" : undefined}
                >
                  {row.getVisibleCells().map((cell) => (
                    <TableCell key={cell.column.id}>
                      {flexRender(cell.column.columnDef.cell, cell.getContext())}
                    </TableCell>
                  ))}
                </TableRow>
              ))
            ) : (
              <TableRow>
                <TableCell
                  colSpan={table.getVisibleLeafColumns().length}
                  className="text-muted-foreground h-24 text-center"
                >
                  {t("ui:faceted.noResults")}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>
      <InfiniteScrollTrigger
        onLoadMore={() => setVisibleCount((count) => count + PAGE_SIZE)}
        hasNextPage={visibleRows.length < filteredRows.length}
        isFetching={false}
        isFetchingNextPage={false}
        hasLoadMoreError={false}
      />
      {comparing && <ItemsCompare items={comparing} onClose={() => setComparing(undefined)} />}
      <AlertDialog
        open={!!deleting}
        onOpenChange={(open) => {
          if (!open && !props.pending) setDeleting(undefined);
        }}
      >
        <AlertDialogContent className="max-sm:gap-5 max-sm:p-5">
          <AlertDialogHeader className="max-sm:items-center max-sm:space-y-3 max-sm:text-center">
            <div className="bg-destructive/10 text-destructive flex size-12 items-center justify-center rounded-full sm:hidden">
              <Icons.Trash className="size-5" />
            </div>
            <AlertDialogTitle>{t("deleteTitle")}</AlertDialogTitle>
            <AlertDialogDescription>{t("deleteMessage")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={props.pending}>{t("cancel")}</AlertDialogCancel>
            <Button
              variant="destructive"
              disabled={props.pending}
              className="max-sm:h-12"
              onClick={async () => {
                if (!deleting) return;
                try {
                  await props.onDeleteMany(deleting);
                  setSelectedIds((previous) => {
                    const next = new Set(previous);
                    for (const item of deleting) next.delete(item.id);
                    return next;
                  });
                  setDeleting(undefined);
                } catch {
                  // The page reports the error; a new confirmation uses the surviving selection.
                  setDeleting(undefined);
                }
              }}
            >
              {props.pending ? (
                <Icons.Spinner className="mr-2 size-4 animate-spin" />
              ) : (
                <Icons.Trash className="mr-2 size-4" />
              )}
              {t("delete")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

function ItemsSelectionActions({
  count,
  pending,
  onDelete,
  onCompare,
  onClear,
}: {
  count: number;
  pending: boolean;
  onDelete: () => void;
  onCompare: () => void;
  onClear: () => void;
}) {
  const { t } = useTranslation("procurement");
  return (
    <div
      role="region"
      aria-label={t("spending:transactions.bulkActions")}
      className="bg-muted/40 ring-border flex flex-wrap items-center justify-between gap-2 rounded-md px-3 py-2 ring-1"
    >
      <div className="flex items-center gap-2 text-sm">
        <Icons.Check className="size-4" />
        <span className="font-medium">{t("selected", { count })}</span>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          variant="outline"
          disabled={pending || count < 2}
          title={count < 2 ? t("compareMinTwo") : undefined}
          onClick={onCompare}
        >
          <Icons.ChartBar className="me-1.5 size-3.5" />
          {t("compare")}
        </Button>
        <Button
          size="sm"
          variant="outline"
          className="text-destructive hover:bg-destructive/10"
          disabled={pending}
          onClick={onDelete}
        >
          <Icons.Trash className="mr-1.5 size-3.5" />
          {t("delete")}
        </Button>
        <Button size="sm" variant="ghost" disabled={pending} onClick={onClear}>
          {t("common:clear")}
        </Button>
      </div>
    </div>
  );
}
