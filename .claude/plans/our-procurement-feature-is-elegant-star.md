# Procurement item tax type

## Context

Shops report item prices inconsistently: some print **tax-excluded** figures on
receipts, others **tax-included**. Tax rates also differ per item (Japanese
consumption tax: 8% reduced rate for food, 10% standard). Today a procurement
purchase stores only `(quantity, subtotal)` as typed from the receipt, so a
tax-excluded ¥1,000 line and a tax-included ¥1,000 line are stored identically
and compared as equal. That makes the Procurement view's min/avg/latest price
comparisons wrong across shops.

The fix: a **tax type** on each procurement item — `Tax-included`,
`Tax-excluded 8%`, `Tax-excluded 10%`, `Tax-exempt` — used to interpret the
stored subtotal. Existing rows default to `Tax-included`, which preserves
today's behavior exactly.

**Intended outcome**

- The Activity form stays **byte-for-byte identical**: the user still enters
  only item / quantity / subtotal, exactly as printed on the receipt.
- The Procurement view shows **every** price tax-included regardless of item tax
  type, so comparisons are apples-to-apples.

### Decided semantics

Tax type lives **only** on `procurement_items`. There is no per-purchase
snapshot column. Changing an item's tax type **retroactively reinterprets all of
its historical purchases** — the stored receipt figure is never rewritten, only
re-read. This was chosen deliberately: the item is the single source of truth,
nothing is denormalized, and no stored data is ever mutated by an edit.

The trade-off, accepted: flipping an item from `Tax-included` to
`Tax-excluded 10%` shifts that item's whole displayed price history (a stored
1000 reads as 1000 before, 1100 after) and moves its min/avg/latest stats.

Two further decisions: gross-up keeps **full `Decimal` precision**, rounding only
at display; and the tax type is **not surfaced anywhere in the Procurement
view** (no badge, no column) — it appears only in the item form.

## Key insight

`unit_price` is **derived, never persisted**. It is computed in exactly one
place — `crates/storage-sqlite/src/procurement/purchases.rs:69-73`
(`subtotal / quantity`) — and every downstream consumer flows through it:

- `PurchasesService::list_for_item` → `fold_prices` → item detail chart + stats
- `ItemsService::list_with_stats` (`items/service.rs:58`) → table min/avg/latest

So applying the tax gross-up **at that one site** makes the entire Procurement
view tax-inclusive with **zero changes to any view component**.

Critically, the gross-up must happen _before_ `unit_price` is set, not inside
`fold_prices`: the detail chart tooltip renders the raw `point.unitPrice` in the
transaction currency alongside stats derived from `unit_price_base`. Grossing up
later would leave the tooltip tax-excluded while the stats around it are
tax-included.

`list_for_activity()` — which hydrates the Activity form — returns the raw
`Purchase` and **must not be touched**. That is what keeps the form showing the
figure the user typed.

## Implementation

### 1. Migration

New `crates/storage-sqlite/migrations/2026-10-20-000001_procurement_tax_type/`,
mirroring the `2026-10-19-000001_procurement_units` precedent (a plain
`ADD COLUMN`; no table rebuild is needed since nothing is retyped).

`up.sql`:

```sql
ALTER TABLE procurement_items ADD COLUMN tax_type TEXT NOT NULL DEFAULT 'INCLUDED';
```

`down.sql`:

```sql
ALTER TABLE procurement_items DROP COLUMN tax_type;
```

The `DEFAULT 'INCLUDED'` is what gives existing records backward-compatible
behavior. Follow the `unit` column precedent and add **no** `CHECK` constraint —
values are validated in the domain and again at the storage boundary.

### 2. Domain — `crates/procurement/src/items/tax.rs` (new)

Mirror `items/units.rs` exactly (const slice + predicate), plus the multiplier.
Use `SCREAMING_SNAKE` tokens, consistent with `activity_type` values like
`WITHDRAWAL`.

```rust
//! How a shop reports an item's price. Purchase subtotals are stored exactly as
//! printed on the receipt; the tax type says whether that figure already carries
//! consumption tax, so prices always compare tax-included.
pub const TAX_TYPES: &[&str] = &["INCLUDED", "EXCLUDED_8", "EXCLUDED_10", "EXEMPT"];

pub fn is_known_tax_type(tax_type: &str) -> bool {
    TAX_TYPES.contains(&tax_type)
}

/// Multiplier taking an entered subtotal to its tax-included value.
pub fn tax_multiplier(tax_type: &str) -> Decimal {
    match tax_type {
        "EXCLUDED_8" => Decimal::new(108, 2),
        "EXCLUDED_10" => Decimal::new(110, 2),
        _ => Decimal::ONE,
    }
}
```

`INCLUDED` and `EXEMPT` share a multiplier of 1 but stay distinct values, since
they mean different things on a receipt.

Deliberately **no `DEFAULT_TAX_TYPE` const**: commit `72a5038e8` removed the
equivalent `DEFAULT_UNIT` as unused. The default lives in SQL and in the
frontend module.

Register in `items/mod.rs`: `mod tax;` + `pub use tax::*;`

### 3. Domain — items

- `items/model.rs`: add `pub tax_type: String` to **both** `Item` and `SaveItem`
  (camelCase `taxType` over the wire via the existing `rename_all`).

  Like `unit`, this is a **required** field with no `#[serde(default)]` — so a
  `SaveItem` payload omitting `taxType` is rejected. That matches the `unit`
  precedent and keeps validation strict at the input boundary, but it does mean
  the two e2e specs that POST item payloads directly must be updated (below).
  Backward compatibility for _existing stored rows_ comes from the migration
  default, which is what the requirement asks for.

- `items/service.rs`:
  - `validate()` — trim + reject unknown, mirroring the `unit` block at
    `:136-143`.
  - `clone_item()` (`:101-111`) — copy `tax_type`, or clones silently reset to
    `Tax-included`.

### 4. Storage

- `crates/storage-sqlite/src/schema.rs` — append `tax_type -> Text,` **last** in
  the `procurement_items` block (after `unit`, matching diesel print-schema
  ordering for ALTER-added columns).
- `crates/storage-sqlite/src/procurement/items.rs` — four touch points,
  `tax_type` last to match schema order: struct field (`:38`), `into_model`
  (`:64`), the insert-default literal (`:142`), and `row.tax_type = input.tax_type;`
  in `save()` (`:182`).
- `crates/storage-sqlite/src/procurement/purchases.rs` — **the core change**, in
  `list()` only. Extend the existing import
  `schema::{activities, procurement_purchases as purchases}` with
  `procurement_items as items`, then chain a second
  `.inner_join(items::table)` onto the current `activities` join (the
  `joinable!(procurement_purchases -> procurement_items (item_id))` at
  `schema.rs:945` already exists), select `procurement_items::tax_type` into the
  loaded tuple, and gross up before dividing:

  ```rust
  let gross = purchase.input.subtotal
      .checked_mul(wealthfolio_procurement::items::tax_multiplier(&tax_type))
      .context("Procurement purchase subtotal overflow")?;
  let unit_price = gross
      .checked_div(purchase.input.quantity)
      .context("Procurement purchase has a zero quantity")?;
  ```

  The rule lives in the domain crate; storage only applies it — consistent with
  how `save()` already re-runs the domain `validate()`.

  Leave `list_for_activity()` and the write path (`set_for_activity`) untouched.

### 5. Frontend

- **`features/procurement/tax-types.ts`** (new) — mirror `units.ts`; const array
  - union type + `DEFAULT_PROCUREMENT_TAX_TYPE = "INCLUDED"`, plus a
    `Record<ProcurementTaxType, string>` mapping each value to its i18n label key.
    No TypeScript enum (per AGENTS.md).
- **`types/index.ts`** — add `taxType: string;` to `ProcurementItem` and
  `SaveProcurementItem`.
- **`components/item-form.tsx`** — the only UI change:
  - Zod: `taxType` with a `.refine(...)` against the const array, message
    `t("invalidTaxType")` (mirrors the `unit` refine at `:53-58`).
  - `defaultValues`: `item?.taxType ?? DEFAULT_PROCUREMENT_TAX_TYPE`.
  - A `ResponsiveSelect` `FormField` in its own full-width row after the
    brand/unit grid (`:254-287`) — the labels are too long for the `5rem`
    column that `unit` occupies.
  - `submit` spreads `...values`, so no payload mapping change is needed.
- **No change** to `activity-purchases.tsx`, `cash-activity-form.tsx`,
  `items-table.tsx`, `item-view.tsx`, `item-compact.tsx`, or `unit-price.tsx` —
  the Activity form stays identical and the view becomes tax-inclusive purely
  from the backend.
- **No change** to adapters (`adapters/shared/procurement.ts`,
  `adapters/web/core.ts`) or to Tauri/Axum commands — DTOs pass through
  opaquely, as they did for `unit`.
- **i18n — all 10 locales** (`de, en, es, fr, it, ja, ko, pt, zh, zh-Hant` under
  `apps/frontend/src/i18n/locales/<lang>/procurement.json`), 7 new keys:
  `taxType`, `taxTypeSheetTitle`, `invalidTaxType`, `taxIncluded`,
  `taxExcluded8`, `taxExcluded10`, `taxExempt`.

## Correctness traps

- **Do not gross up `list_for_activity`.** It hydrates the Activity form; the
  user must see the figure they typed.
- **Do not gross up inside `fold_prices`** — it would desync the chart tooltip's
  `unitPrice` from the tax-included stats shown beside it.
- **Division order**: multiply then divide, and keep the existing
  `checked_div` + `.context("...zero quantity")` guard.
- **Precision**: no `round_dp` in Rust. `1000 × 1.10 = 1100.00` is exact in
  `Decimal`; non-terminating divisions already exist today and are handled.
- **`clone_item`** must copy `tax_type`.
- **Column order**: `tax_type` goes last in both `schema.rs` and `ItemDB`, after
  `unit`.
- `items-table.test.tsx:20` returns a fully-annotated
  `ProcurementItemWithStats` and **will fail type-check** until it gains
  `taxType`. `activity-purchases.test.tsx:48` uses an
  `as ProcurementItemWithStats[]` cast on partial literals and compiles
  untouched.

## Architecture impact

**Unchanged** — no new storage mechanism, worker, retry, or sync primitive; no
new commands, routes, or adapter entries; no device-sync entity; procurement
remains a leaf crate that does not participate in valuation; activity data stays
owned by `crates/core`; the Activity form's behavior and request payload are
untouched; stored receipt figures are never mutated.

**Changed** — `PurchasesRepository::list()` gains a join on
`procurement_items` and now returns **tax-included** unit prices. This changes
the observable value of procurement price and stat reads. That is the point of
the feature, so this is **not** a behavior-preserving refactor. With every
existing row defaulting to `INCLUDED` (multiplier 1), the change is a no-op for
current data until a user sets a tax type.

## Verification

Rust:

```bash
cargo test --locked -p wealthfolio-procurement
cargo test --locked -p wealthfolio-storage-sqlite procurement
cargo fmt --all -- --check
cargo clippy --locked -p wealthfolio-procurement -p wealthfolio-storage-sqlite --all-targets --all-features -- -D warnings
cargo check --locked -p wealthfolio-app -p wealthfolio-server
```

New/updated Rust tests:

- `items/tax.rs` — `is_known_tax_type` accept/reject; multiplier for all four
  values.
- `items/service.rs` — add `tax_type` to the `input()` fixture (`:166-178`); new
  `trims_the_tax_type_and_rejects_one_off_the_list`, mirroring the `unit` test
  at `:197-209`.
- `crates/storage-sqlite/tests/procurement.rs` — add `tax_type` to the fixture
  (`:56`); extend `procurement_derives_unit_price_from_stored_subtotal` (`:399`)
  with an `EXCLUDED_10` case (subtotal 1000, qty 2 → `unit_price` 550) and an
  `EXCLUDED_8` case; add a test asserting an item tax-type edit **reinterprets
  existing purchases**, pinning the decided semantics.

Frontend:

```bash
pnpm --filter frontend exec vitest run src/features/procurement
pnpm --filter frontend exec vitest run src/features/spending/components/cash-activity-form.test.tsx
pnpm --filter frontend i18n:check
pnpm type-check
pnpm check
```

- `item-form.test.tsx` — new item defaults to `INCLUDED`; an edited item keeps
  its stored value (mirrors the `unit` test at `:76-103`).
- `cash-activity-form.test.tsx` must pass **unchanged** — the proof the Activity
  form did not move.
- `items-table.test.tsx:20` fixture gains `taxType` (required for type-check).

End-to-end (consult `.claude/skills/run-e2e-tests/SKILL.md` first):

```bash
pnpm test:e2e e2e/33-procurement.spec.ts
```

- Add `taxType` to the item payloads POSTed at `:205-213` and `:354` —
  **required**, not optional, since `SaveItem` has no serde default.
- Add a case covering a `Tax-excluded 10%` item: subtotal 1000 / qty 1 entered
  in the Activity form, asserting the form still round-trips 1000 while the
  Procurement view reports 1100.

Manual check: create an item with `Tax-excluded 10%`, log an activity with
subtotal 1000 / qty 1, confirm the Activity form still shows **1000** on reopen
while the Procurement view reports **1100**.
