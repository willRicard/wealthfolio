# Procurement — price tracking for purchased goods and services

## Context

Wealthfolio records _what was spent_ (Spending activities) but not _what was bought_.
There is no way to ask "what do I usually pay for this, and where was it cheapest?"

This change adds **Procurement**: a registry of goods and services you buy repeatedly,
each with a price history derived from Spending activities. A new top-level nav item and
`/procurement` route list registered items with their min / average / latest price; a view
dialog shows a price timeline for one item.

This round lays the groundwork: items are fully manageable from the UI, and the
purchase-history link (item ↔ activity ↔ qty + price) exists end-to-end on the backend
**with no UI for linking**. That boundary is deliberate — it lets the cascade rules be
implemented and regression-tested now, before the linking UI is designed.

**Architecture impact.** Unchanged boundaries: no new network calls or provider settings;
no new background worker, queue, retry, or synchronization primitive; no change to the
portfolio coordinator or `projection_state`; no new domain-event variant or planner branch;
activity create/update/delete keeps its current ownership and failure propagation. Added:
one migration (including a photo-draft table), a new leaf crate `crates/procurement`, its
SQLite repositories, parallel Tauri/Axum surfaces, a blob root for item photos, and a sweep of
abandoned photo drafts that runs once at startup (not a worker). Procurement tables are **not**
registered for device sync in this round.

---

## Storage design (confirmed)

Three tables. The key decision: **nothing derived is persisted.**

```sql
procurement_items
  id TEXT PK, name TEXT NOT NULL, brand TEXT,
  category_id TEXT NOT NULL, subcategory_id TEXT,     -- -> procurement_categories taxonomy
  rating REAL CHECK (rating >= 0 AND rating <= 5),    -- 0.5 steps, exact in binary float
  photo_backend/photo_key/photo_preview_key TEXT,     -- all nullable, single photo
  photo_filename/photo_content_type TEXT, photo_size_bytes BIGINT,
  created_at TEXT, updated_at TEXT

procurement_item_aliases
  id TEXT PK, item_id TEXT -> procurement_items(id) ON DELETE CASCADE,
  alias TEXT NOT NULL, sort_order INTEGER NOT NULL DEFAULT 0,
  UNIQUE (item_id, alias)

procurement_purchases
  id TEXT PK,
  item_id     TEXT NOT NULL -> procurement_items(id) ON DELETE CASCADE,
  activity_id TEXT NOT NULL -> activities(id)        ON DELETE CASCADE,
  quantity   TEXT NOT NULL CHECK (CAST(quantity   AS REAL) > 0),   -- Decimal-as-TEXT
  unit_price TEXT NOT NULL CHECK (CAST(unit_price AS REAL) >= 0),  -- Decimal-as-TEXT
  created_at TEXT, updated_at TEXT,
  UNIQUE (activity_id, item_id)

procurement_photo_drafts
  id TEXT PK, storage_backend TEXT NOT NULL, storage_key TEXT NOT NULL,
  preview_key TEXT NOT NULL, filename TEXT NOT NULL, content_type TEXT NOT NULL,
  size_bytes BIGINT NOT NULL, created_at TEXT NOT NULL      -- one row per uploaded, unsaved photo
```

**`procurement_purchases` stores no date, no shop, and no currency.** All three are read by
joining `activities`: `activity_date`, `notes` (the field whose UI label is "Payee" — see
`crates/core/src/activities/activities_model.rs:191`), and `currency`. This is what makes
the cascade requirement free:

| Requirement                                 | Mechanism                             | Code needed |
| ------------------------------------------- | ------------------------------------- | ----------- |
| Edit activity date → timeline moves         | joined, not copied                    | none        |
| Edit activity payee → shop changes          | joined, not copied                    | none        |
| Delete activity → purchase rows go          | `ON DELETE CASCADE`                   | none        |
| Delete item → purchases go, activities stay | `ON DELETE CASCADE` on `item_id` only | none        |

Runtime pool connections run `PRAGMA foreign_keys = ON` (`crates/storage-sqlite/src/db/mod.rs:146`),
so cascades fire. Migrations run with FKs **off** (`db/mod.rs:271`), which only matters for
`down.sql`.

### No price cache, and no stored base amount

`FxService` holds `converter: Arc<RwLock<Option<CurrencyConverter>>>` with every rate already
in memory as `HashMap<(From,To), BTreeMap<NaiveDate, Decimal>>`
(`crates/core/src/fx/currency_converter.rs:9-20`). `convert_currency_for_date` is therefore an
O(log n) in-memory lookup with no query, and `get_direct_rate` does bidirectional
nearest-neighbour so weekends and holidays never fail.

So min/avg/latest are folded in Rust over the joined rows at read time. A persisted base
amount would go stale whenever an activity's date or currency is edited or the base currency
changes; computing it is cheap enough that the staleness is not worth buying.

### Multi-currency approach

Follow the precedent in `crates/core/src/limits/limits_service.rs:151` and
`portfolio/net_worth/net_worth_service.rs` — carry **both** amounts through the read model:

- Source of truth: `unit_price` in the activity's own `currency`.
- Comparison: `fx.convert_currency_for_date(unit_price, activity.currency, base_currency, activity_date)`.
- Min / avg / latest are computed **on the converted values only**; mixing raw amounts across
  currencies would be meaningless.
- Each timeline point carries `{ date, shop, unitPrice, currency, unitPriceBase, baseCurrency }`
  so the chart plots base currency while the tooltip can show the original.
- On purchase write, call `fx.ensure_fx_pairs(vec![(activity_currency, base_currency)])` so the
  FX asset exists and market-data sync starts populating the pair.
- If a row cannot be converted, exclude it from the aggregates and flag it rather than
  silently treating it as base currency.

---

## Categories

A **duplicated** taxonomy, per your note. `taxonomy_categories` has
`PRIMARY KEY (taxonomy_id, id)` (`crates/storage-sqlite/migrations/2026-01-01-000002_taxonomies/up.sql:28`),
so the same category ids can be re-used under a second taxonomy.

Seed in the new migration: a `procurement_categories` taxonomy (`is_system = 1`,
`is_single_select = 1`, `scope = 'procurement'` — a new scope value, so it will not appear in
any existing `scope = 'activity'` UI), then copy the relevant subset from `spending_categories`,
**parents first** so the self-FK is satisfied:

```sql
INSERT INTO taxonomy_categories (id, taxonomy_id, parent_id, name, key, color, icon, sort_order)
SELECT id, 'procurement_categories', parent_id, name, key, color, icon, sort_order
  FROM taxonomy_categories
 WHERE taxonomy_id = 'spending_categories'
   AND id IN ('cat_groceries', 'cat_shopping',
              'cat_shopping_clothing', 'cat_shopping_electronics', 'cat_shopping_home')
 ORDER BY (parent_id IS NOT NULL), sort_order;
```

Copying by `SELECT` rather than literals picks up the localized names already applied by
`2026-08-02-000001_localize_spending_seeds_ja`. Resulting shape, stored as two columns:

```
Groceries   -> (cat_groceries, NULL)
Clothing    -> (cat_shopping,  cat_shopping_clothing)
Electronics -> (cat_shopping,  cat_shopping_electronics)
Home Goods  -> (cat_shopping,  cat_shopping_home)
```

The frontend reads it with the existing `useTaxonomy("procurement_categories")`
(`@/hooks/use-taxonomies`) and renders icons through
`features/spending/lib/category-icons.ts` `ICON_MAP`.

---

## Backend

### New crate `crates/procurement`

Follows the `crates/spending` leaf-crate precedent (`crates/spending/src/lib.rs` documents the
isolation contract). Depends on `wealthfolio-core` for `ActivityRepositoryTrait`, `FxServiceTrait`
and `SettingsService`; on `wealthfolio-files` behind a `photos` feature.

```
crates/procurement/src/
  lib.rs, error.rs                  -- ProcurementError via thiserror, mirroring spending/src/error.rs
  items/{mod,model,traits,service}.rs
  purchases/{mod,model,traits,service}.rs
  photos.rs                          -- #[cfg(feature = "photos")]
```

Service API:

- `ItemsService` — `list_with_stats()`, `get(id)`, `create`, `update`, `clone_item(id)`, `delete(id)`.
  `list_with_stats` makes **one** joined query for all purchases and groups in Rust — no N+1.
- `PurchasesService` — `list_for_item(item_id)`, `create`, `update`, `delete`.
- Price folding lives in one place, `purchases::stats`, so a materialized table could later be
  slotted in behind `ItemsService::list_with_stats` without touching callers.

Models use `#[serde(rename_all = "camelCase")]`; money and quantity are `rust_decimal::Decimal`
serialized as strings, matching `activities`.

**We deliberately do not hook into `activity_update_invalidates_spending_splits`**
(`crates/storage-sqlite/src/activities/repository.rs:163`). Splits are cleared on an activity
reshape because they must sum to its amount; procurement purchases carry no such invariant, and
your requirement is that edits _reflect_, not destroy. Noted as a known gap: an activity
retyped from spending to a trade keeps its procurement links.

### Storage, commands, routes

- `crates/storage-sqlite/migrations/2026-10-17-000001_procurement/{up.sql,down.sql}` — tables
  (including `procurement_photo_drafts`), FKs, indexes on `procurement_purchases(item_id)` and `(activity_id)`, taxonomy seed.
  No `projection_state` trigger (procurement does not affect valuation) and no
  `sync_table_state` row.
- `crates/storage-sqlite/src/schema.rs` — `diesel::table!` blocks plus
  `allow_tables_to_appear_in_same_query!` entries, **same commit** as the migration.
- `crates/storage-sqlite/src/procurement/` — repositories in the single-file-per-feature style
  of `src/spending/activity_splits.rs`; reads sync via `get_connection(&self.pool)`, writes
  `async` through `self.writer.exec_tx(...)`.
- Context wiring: `apps/tauri/src/context/{registry.rs,providers.rs}` and
  `apps/server/src/main_lib.rs` `AppState`.
- `apps/tauri/src/commands/procurement.rs` + `commands/mod.rs` + `generate_handler!` in
  `apps/tauri/src/lib.rs` (the `commands::module::fn` form is scraped by the parity test).
- `apps/server/src/api/procurement.rs` with `pub fn router<S>()`, merged in `apps/server/src/api.rs`.

Commands: `get_procurement_items`, `get_procurement_item`, `create_procurement_item`,
`update_procurement_item`, `clone_procurement_item`, `delete_procurement_item`,
`get_procurement_item_prices`, `create_procurement_purchase`, `update_procurement_purchase`,
`delete_procurement_purchase`, plus the three photo commands.

### Photo

Reuses `crates/files` as-is — `StoredFileService::store_with_preview(request, slot, commit)`
validates the filename, renders the WebP preview off-thread, writes both objects, then runs the
`commit` closure (our DB update) and removes both objects if commit fails.

**Upload on selection, staged as a draft.** One photo per item, no ordering. Selecting a photo in
the dialog uploads it immediately via `upload_procurement_photo_draft`. That runs
`store_with_preview` with a commit closure that inserts a row in `procurement_photo_drafts` and
returns the draft id to the frontend. Nothing is linked to an item yet.

- **Save** passes the `draft_id` to `create_procurement_item` or `update_procurement_item`. In one
  transaction, the commit copies the draft's storage references onto the item's photo columns and
  deletes the draft row. On edit, the item's previous photo blobs are removed after the commit.
- **Re-selecting** a photo discards the previous draft (row and blobs) and uploads the new one.
- **Cancel** calls `discard_procurement_photo_draft`, which removes the row and both blobs.
- **Upload failure** shows an inline error and creates no draft. Saving without a photo is still
  allowed.

**Why drafts are needed.** Uploading on selection stores files before any item exists. Without a
draft row, a cancelled dialog or an app closed mid-edit leaves blobs that nothing references. The
commit-failure cleanup in `store_with_preview` does not cover this, because the commit already
succeeded. The draft row is the record that lets cleanup find those blobs.

**Cleanup.** A sweep at startup deletes drafts older than 24 hours along with their blobs. This
mirrors `sweep_drafts` in `apps/tauri/src/context/providers.rs:467` and is not a background worker,
queue, or retry loop. Required behavior: discard on cancel, claim on save, and the startup sweep
for drafts left behind by a closed app. Not in this round: sweeping while the app is running.

**Why a new table rather than `spending_attachment_drafts`.** That table is keyed by a per-dialog
`token`, constrained to attachment content types, and guarded by a 10-attachment trigger. None of
those fit a single procurement photo, so the drafts table here is separate, using the same
idea.

Blob root `procurement-photos`, registered next to the existing
`transaction-attachments` root in `apps/tauri/src/context/providers.rs:463` and
`apps/server/src/main_lib.rs:815`.

---

## Frontend

New feature folder `apps/frontend/src/features/procurement/{adapters,components,hooks,lib,pages,types}`.

- **Nav** — `apps/frontend/src/pages/layouts/navigation/app-navigation.tsx`, in
  `buildStaticNavigation`. Note there is **no Spending nav item** (see the comment at line 109:
  spending lives on the dashboard tab), so "between Spending and Goals" resolves to **between
  Activities and Goals**. Use `<Icons.Package className="size-6" />`.
- **Route** — `<Route path="procurement" element={<ProcurementPage />} />` in
  `apps/frontend/src/routes.tsx`, alongside the existing top-level pages.
- **Adapters** — `apps/frontend/src/adapters/shared/procurement.ts` wrapping `invoke` from
  `./platform`, modelled on `adapters/shared/goals.ts`; `COMMANDS` entries + `switch` cases in
  `adapters/web/core.ts`; re-exports in **both** `adapters/web/index.ts` and
  `adapters/tauri/index.ts`. Photo upload needs the split implementation
  (`adapters/{tauri,web}/procurement-photo.ts`) that
  `adapters/*/transaction-attachments.ts` already demonstrates — `invokeRaw` with
  `x-wf-*` headers on Tauri, `postFileWithProgress` on web.

### Table page

Reuse the shared `DataTable` (`packages/ui/src/components/ui/data-table/`) client-side, as
`apps/frontend/src/pages/asset/assets-table.tsx` does — `searchBy="name"` gives the text search,
`DataTableFacetedFilter` gives Category and Subcategory, and `DataTablePagination` is imported
separately (the `DataTable` does not render it itself). At personal scale one
`get_procurement_items` call returning items **with** their stats is simpler and faster than
server-side paging.

Columns: Name · Brand · Category · Stars · Min Price · Avg Price · Price · Date · actions.
No photo column. Money cells use `<PriceDisplay value currency={baseCurrency} isHidden={isBalanceHidden} />`
from `@wealthfolio/ui`; dates use `useDateFormatting()`. Row actions are a `DropdownMenu` with
Edit · Clone · Delete, the destructive item styled `text-destructive focus:text-destructive`,
delete confirmed with the shared `ActionConfirm`. A mobile twin gated on
`useIsCompactTableViewport()` is required — every table in this repo has one.

Top-right **Add** button in the page header opens the create dialog.

### Create / edit dialog

`Dialog` + react-hook-form + Zod with the schema built inside the component so `t()` messages
localize, following `apps/frontend/src/pages/settings/general/exchange-rates/add-exchange-rate-form.tsx`.
Fields: name, photo, brand, category, subcategory (options filtered by the selected category),
rating. The photo field uploads on selection and holds the returned draft id in form state; see
Photo above. Aliases go in a collapsible **Advanced** section built on the `Collapsible` shell from
`apps/frontend/src/pages/activity/components/forms/fields/advanced-options-section.tsx`.

**Star rating needs a small extension.** `packages/ui/src/components/ui/star-rating.tsx`
already renders fractional fill, but `handleStarClick` emits whole stars only
(`onRatingChange(starIndex + 1)`, line 39). Add an optional `step?: 1 | 0.5` defaulting to `1`,
and when `step === 0.5` derive half vs. full from the pointer's x-offset within the star, in both
`handleStarClick` and `handleStarHover`. Backward compatible; existing call sites
(`addon-store-card.tsx`, `rating-dialog.tsx`) are untouched.

### View dialog

Read-only details, **no aliases**. Three stat tiles (Min · Avg · Latest) above a simple price
timeline, following the `apps/frontend/src/pages/asset/asset-history-card.tsx` layout. The chart
reuses `ChartContainer` from `packages/ui/src/components/ui/chart.tsx`; copy the shape of
`apps/frontend/src/components/history-chart-symbol.tsx` — an `Area type="monotone"` with a
gradient fill plus a `ReferenceLine y={avgPrice}`, which gives the average line for free. Tooltip
shows date, shop (activity `notes`) and the original-currency price. Empty state until purchases
exist. Footer has an Edit button that swaps the dialog to the edit form.

### i18n

New `procurement` namespace: add `"procurement"` to the list in
`apps/frontend/src/i18n/locales.ts:69` and create `procurement.json` in **all 10 locales**
(`de, en, es, fr, it, ja, ko, pt, zh, zh-Hant`), plus the `common:nav.label_procurement` key.
`StarRatingDisplay`'s review-count suffix is hardcoded English — pass `showText={false}` and
render our own text.

Add `QueryKeys.PROCUREMENT_ITEMS` / `PROCUREMENT_ITEM_PRICES` in `apps/frontend/src/lib/query-keys.ts`.

---

## Build order

1. Migration + `schema.rs` + taxonomy seed.
2. `crates/procurement` models, traits, services, errors; SQLite repositories.
3. Context wiring, Tauri commands, Axum routes.
4. Rust tests (below), then photo upload, draft, claim, discard, and startup sweep behind the
   `photos` feature.
5. Frontend adapters + parity test.
6. Table page, nav, route, i18n.
7. Create/edit dialog incl. the `StarRating` `step` prop; view dialog + chart.

---

## Verification

**Rust — the cascade rules are the point of this round.** In `crates/procurement` and
`crates/storage-sqlite`:

- deleting an activity removes its `procurement_purchases` rows and leaves the item;
- deleting an item removes its purchases and **leaves the activities** intact;
- editing an activity's date moves the timeline point and can change which price is "latest";
- editing an activity's `notes` changes the reported shop;
- a two-currency item folds min/avg/latest over **converted** values, and a row whose pair has
  no FX history is excluded rather than counted raw;
- `UNIQUE (activity_id, item_id)` rejects a duplicate link.
- photo draft: discard removes the draft row and both blobs;
- saving with a `draft_id` moves the photo onto the item and removes the draft row;
- saving an edit with a new draft removes the previous photo blobs;
- re-selecting a photo leaves exactly one draft;
- a failed commit leaves no draft row and no blobs;
- the startup sweep removes drafts older than 24 hours and keeps newer ones.

```
cargo test --locked -p wealthfolio-procurement
cargo test --locked -p wealthfolio-storage-sqlite procurement     # CONNECT_API_URL=http://test.local
cargo fmt --all -- --check
cargo clippy --locked -p wealthfolio-procurement -p wealthfolio-storage-sqlite \
      --all-targets --all-features -- -D warnings
cargo check --locked -p wealthfolio-app -p wealthfolio-server
```

**Adapter wiring** (required — this change adds commands on both runtimes):

```
pnpm --filter frontend exec vitest run src/adapters/adapter-command-parity.test.ts
```

**Frontend:**

```
pnpm --filter frontend exec vitest run
pnpm check
pnpm type-check
pnpm --filter frontend i18n:check
pnpm build && pnpm build:tauri        # new adapter modules affect both bundles
```

**Manual, `pnpm tauri dev`:** Procurement appears between Activities and Goals → create an item
with photo, 3.5 stars, two aliases → it lists with empty price columns → clone (no photo,
aliases copied) → edit → search and filter by Category then Subcategory → delete, confirm.
Then, with purchases inserted through the backend commands, re-check the table's
Min/Avg/Price/Date and the view dialog's timeline, and confirm that editing the source
activity's date and payee in Spending is reflected without any procurement write.

## Deferred

- UI for linking items to transactions (the stated boundary this round).
- Device-sync registration for the three tables (6 touchpoints per entity).
- OCR / receipt-label matching — aliases are stored but unused.
- Any filter beyond text search + Category/Subcategory.
