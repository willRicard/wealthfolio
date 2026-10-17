import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button, Icons, Page, PageContent, PageHeader } from "@wealthfolio/ui";
import { cloneProcurementItem, deleteProcurementItem, getProcurementItems } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import { useIsCompactTableViewport } from "@/hooks/use-platform";
import { ItemForm } from "../components/item-form";
import { ItemView } from "../components/item-view";
import { ItemsTable } from "../components/items-table";
import type { ProcurementItemWithStats } from "../types";

export default function ProcurementPage() {
  const { t } = useTranslation("procurement");
  const compact = useIsCompactTableViewport();
  const client = useQueryClient();
  const {
    data: items = [],
    isPending,
    isError,
  } = useQuery({ queryKey: [QueryKeys.PROCUREMENT_ITEMS], queryFn: getProcurementItems });
  const [form, setForm] = useState<{ item?: ProcurementItemWithStats }>();
  const [view, setView] = useState<ProcurementItemWithStats>();
  const invalidate = () => {
    void client.invalidateQueries({ queryKey: [QueryKeys.PROCUREMENT_ITEMS] });
    void client.invalidateQueries({ queryKey: [QueryKeys.PROCUREMENT_ITEM_PRICES] });
  };
  const mutation = useMutation({
    mutationFn: ({ kind, id }: { kind: "clone" | "delete"; id: string }) =>
      kind === "clone" ? cloneProcurementItem(id).then(() => undefined) : deleteProcurementItem(id),
    onSettled: invalidate,
  });
  return (
    <Page>
      <PageHeader
        heading={t("title")}
        text={t("subtitle")}
        actions={
          <Button size="sm" onClick={() => setForm({})}>
            <Icons.Plus className="mr-2 size-4" />
            {t("add")}
          </Button>
        }
      />
      <PageContent>
        {isPending ? (
          <p role="status">{t("loading")}</p>
        ) : isError ? (
          <p role="alert" className="text-destructive">
            {t("loadFailed")}
          </p>
        ) : (
          <ItemsTable
            items={items}
            compact={compact}
            pending={mutation.isPending}
            onView={setView}
            onEdit={(item) => setForm({ item })}
            onClone={(item) => mutation.mutate({ kind: "clone", id: item.id })}
            onDeleteMany={async (selected) => {
              for (const item of selected)
                await mutation.mutateAsync({ kind: "delete", id: item.id });
            }}
          />
        )}
        {mutation.isError && (
          <p role="alert" className="text-destructive">
            {t("error")}
          </p>
        )}
        {form && (
          <ItemForm
            item={form.item}
            onClose={() => setForm(undefined)}
            onSaved={() => {
              setForm(undefined);
              invalidate();
            }}
          />
        )}
        {view && (
          <ItemView
            item={view}
            onClose={() => setView(undefined)}
            onEdit={() => {
              setForm({ item: view });
              setView(undefined);
            }}
          />
        )}
      </PageContent>
    </Page>
  );
}
