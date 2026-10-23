# Procurement — Compare view for multiple selected items

## Context

The Procurement table already lets you select multiple items (checkbox column +
a "Bulk actions" bar offering Delete and Clear), and the per-item dialog
(`item-view.tsx`) already charts that item's price history. But the two never
meet: to compare three products you must open three dialogs in sequence and hold
the shapes in your head. The feature's own subtitle promises "compare prices over
time", and today that is only true one item at a time.

This adds a **Compare** action to the existing selection bar that opens a dialog
charting the selected items' **unit price over time on one axis**, with a stats
table underneath. The outcome: pick N items in the table, press Compare, and see
which is getting more expensive.

**Scope: frontend only.** No backend, adapter, command, or routing changes.

## Decisions taken

| Decision     | Choice                                                       |
| ------------ | ------------------------------------------------------------ |
| Presentation | Dialog over the table (mirrors `ItemView`), not a route      |
| Metric       | Unit price over time (`unitPriceBase`, base currency)        |
| Mixed units  | Plot together; label each series with its unit + show a note |
| Item limit   | No cap; cycle the 8 `PERFORMANCE_CHART_COLORS`               |

## What is reused (no new infrastructure)

| Need                | Reuse                                                                                               | Path                                                              |
| ------------------- | --------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| Price series        | `getProcurementItemPrices(id)`                                                                      | `@/adapters` (`adapters/shared/procurement.ts:31`)                |
| Cache key           | `QueryKeys.PROCUREMENT_ITEM_PRICES` — **same key as `ItemView`**, so series already viewed are warm | `lib/query-keys.ts:26`                                            |
| Fan-out N queries   | `useQueries`                                                                                        | pattern at `pages/performance/hooks/use-performance-data.ts:96`   |
| Multi-series chart  | `ChartContainer` / `ChartTooltip` / `ChartTooltipContent` / `ChartLegend` / `ChartLegendContent`    | `@wealthfolio/ui/components/ui/chart`                             |
| Series colors       | `PERFORMANCE_CHART_COLORS` (8, cycled)                                                              | `components/performance-chart-colors.ts`                          |
| Overall chart shape | `PerformanceChart` — config-reduce + one `<Line>` per series                                        | `components/performance-chart.tsx:82-148`                         |
| Range toggle        | `IntervalSelector` (already i18n'd via `ui:interval.*`)                                             | `@wealthfolio/ui` `components/financial/interval-selector.tsx`    |
| Dialog shell        | `Dialog*`                                                                                           | `@wealthfolio/ui/components/ui/dialog` (as `item-view.tsx:50-56`) |
| Price rendering     | `UnitPrice`, `useDateFormatting`, `useAmountFormatting`, `useBalancePrivacy`                        | existing feature/UI imports                                       |

**No backend work.** The series endpoint is one-item-only, so Compare issues one
cached query per selected item. See _Tradeoffs_ for why that is acceptable here.

## Files

### 1. NEW `apps/frontend/src/features/procurement/components/items-compare.tsx`

`ItemsCompare({ items, onClose })` — the dialog.

**Load.** One query per item, sharing `ItemView`'s cache key:

```tsx
const results = useQueries({
  queries: items.map((item) => ({
    queryKey: [QueryKeys.PROCUREMENT_ITEM_PRICES, item.id],
    queryFn: () => getProcurementItemPrices(item.id),
  })),
});
```

**Build series.** Index-keyed ids (`s0`, `s1`, …) keep the `--color-<key>` CSS
custom properties `ChartContainer` emits valid — item ids are UUIDs and must not
be used as config keys.

```tsx
const series = items.map((item, i) => ({
  key: `s${i}`,
  item,
  color: PERFORMANCE_CHART_COLORS[i % PERFORMANCE_CHART_COLORS.length],
  points: (results[i].data?.prices ?? [])
    .filter((p) => p.unitPriceBase !== null && inRange(p.date))
    .map((p) => ({
      ...p,
      t: new Date(p.date).getTime(),
      value: Number(p.unitPriceBase),
    })),
}));
const chartConfig = Object.fromEntries(
  series.map((s) => [s.key, { label: s.item.name, color: s.color }]),
);
```

**Merge onto a shared time axis.** Each item is only priced on its own purchase
dates, so build the _union_ of timestamps and leave gaps `null`:

```tsx
const byTime = series.map((s) => new Map(s.points.map((p) => [p.t, p.value])));
const times = [
  ...new Set(series.flatMap((s) => s.points.map((p) => p.t))),
].sort((a, b) => a - b);
const rows = times.map((t) => ({
  t,
  ...Object.fromEntries(
    series.map((s, i) => [s.key, byTime[i].get(t) ?? null]),
  ),
}));
```

> Do **not** copy `PerformanceChart`'s merge (`performance-chart.tsx:34-43`): it
> walks only `data[0]`'s dates, which is fine for aligned daily returns but would
> silently drop every purchase date the first item doesn't share.

**Chart.** A `LineChart` over `rows`, one `<Line>` per series:

- `XAxis dataKey="t" type="number" scale="time" domain={["dataMin","dataMax"]}`
  with `tickFormatter={(v) => formatDate(new Date(v).toISOString())}`.
  A **numeric time axis, not a category axis** — purchases are irregular, and a
  category axis would space a 2-day and a 6-month gap identically.
- `<Line dataKey={s.key} stroke={s.color} connectNulls dot={{ r: 3 }} type="monotone" />`.
  `connectNulls` is **required** (opposite of `item-view.tsx:148`) because every
  series is `null` at the other items' purchase dates.
- `YAxis` formatted with `formatAmount(value, baseCurrency, false)`.
- Custom tooltip listing, per series present at that timestamp: name, unit price
  (via `UnitPrice`, so the `/unit` suffix shows), and shop.
- `ChartLegend` showing `name (currency/unit)` per the mixed-units decision.

**Stats table** under the chart — Item / Min / Avg / Latest / Δ%, one row per
series, colored swatch matching the line. Compute these **client-side from the
in-range points**, not from `data.stats`: the backend stats cover the whole
history and would contradict a filtered chart. Δ% is first→last in range.

**States**, following `item-view.tsx:87-97` exactly:
`results.some(isPending)` → `t("loading")`; `results.some(isError)` →
`t("loadFailed")`; no points at all → `t("noPurchases")`;
`isBalanceHidden` → `••••` instead of chart and figures;
any `stats.unconvertedCount > 0` → `t("missingFx")`;
selection spans >1 distinct `item.unit` → `t("mixedUnits")`.

Range: `IntervalSelector` with `defaultValue="ALL"` (procurement series are
sparse), filtering client-side — the repository exposes no date predicate and
the full series is already in hand.

### 2. MODIFY `apps/frontend/src/features/procurement/components/items-table.tsx`

- Add `comparing` state beside the existing `deleting` state, and render
  `<ItemsCompare items={comparing} onClose={() => setComparing(undefined)} />`.
  Keep it **inside this component**, next to the delete `AlertDialog`
  (`items-table.tsx:333`) — selection lives here, so nothing needs lifting to
  the page.
- In `ItemsSelectionActions` (`items-table.tsx:383-422`), add a Compare button
  before Delete, using `Icons.ChartBar`, `disabled={pending || count < 2}`.

### 3. MODIFY `apps/frontend/src/i18n/locales/<lang>/procurement.json` (10 locales)

New keys: `compare`, `compareTitle`, `compareDescription`, `compareMinTwo`,
`mixedUnits`, `change`, `noPricesInRange`.
Reuse existing `loading`, `loadFailed`, `noPurchases`, `missingFx`, `close`,
`minPrice`, `avgPrice`, `latestPrice`, `item`, `unit`.

Locales: `en, fr, de, es, pt, zh, zh-Hant, ja, ko, it`. **`ar` has no
`procurement.json` at all** — the namespace is currently untranslated there and
falls back to `en`. Leave it that way; translating the whole namespace is out of
scope.

### 4. NEW `apps/frontend/src/features/procurement/components/items-compare.test.tsx`

Mirror the mocking style of `items-table.test.tsx` / `item-view` tests. Cover:

- union merge — an item with no purchase on another's date yields `null` there,
  and no date is dropped;
- mixed-unit selection renders the `mixedUnits` note;
- `>8` items cycle back to the first color;
- balance-privacy hides figures;
- a failing series surfaces `loadFailed`.

### 5. MODIFY `e2e/33-procurement.spec.ts`

Extend the existing selection block (`33-procurement.spec.ts:179-187`, which
already selects two items and asserts the bulk bar): press Compare, assert the
dialog opens with two legend entries and two `.recharts-line` paths, then close.

## Architecture impact

**No boundaries change.** Unchanged: network/provider behavior (no new endpoint,
command, route, or adapter), sync vs. background execution, persistence, events,
error propagation, and ownership of business logic — FX conversion and price
folding stay in `crates/procurement`. The change is additive frontend rendering
over an existing query, reusing its cache key.

## Tradeoffs and deferrals

- **N requests for N items.** Compare fans out one existing per-item query per
  selected item. They are cache-shared with `ItemView` and only fire on an
  explicit click, but with no item cap a very large selection issues a
  correspondingly large burst. Accepted per the no-cap decision. If it becomes a
  problem, the fix is a batch `get_procurement_item_prices` accepting multiple
  ids — **explicitly deferred**, not built speculatively.
- **Mixed units share one Y axis.** Labelled rather than normalized, per the
  decision above.
- **Two purchases of one item at the same timestamp** collapse to the later
  value in the merge map. Timestamps are full RFC3339, so collisions are rare.

## Verification

1. `pnpm --filter frontend exec vitest run src/features/procurement` — new and
   existing feature tests.
2. `pnpm check` and `pnpm type-check`.
3. `pnpm --filter frontend i18n:check` — key parity across the 10 locales.
4. Manual, via `pnpm dev:web` at `/procurement`: select 2 items → Compare → two
   distinct colored lines on one axis; confirm a gap in one series does not break
   the other's line; switch the interval and confirm the stats table tracks the
   visible range; select 9+ items and confirm colors cycle without crashing;
   toggle balance privacy; select items with different units and confirm the note.
5. RTL: reload with `?dir=rtl` and confirm the legend, stats table and dialog
   mirror correctly (use `ms-`/`text-end` logical classes throughout).
6. E2E: `pnpm test:e2e` with the focused procurement spec — consult
   `.claude/skills/run-e2e-tests/SKILL.md` and `e2e/README.md` for the mandatory
   setup before running.

Rust checks and `pnpm build:tauri` are **not** required: no Rust or adapter
wiring is touched.
