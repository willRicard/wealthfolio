import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Area, AreaChart, CartesianGrid, ReferenceLine, Tooltip, XAxis, YAxis } from "recharts";
import { Button, StarRatingDisplay, useDateFormatting, useAmountFormatting } from "@wealthfolio/ui";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@wealthfolio/ui/components/ui/dialog";
import { ChartContainer } from "@wealthfolio/ui/components/ui/chart";
import { getProcurementItemPrices } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { useTaxonomy } from "@/hooks/use-taxonomies";
import { ItemPhoto } from "./item-photo";
import { UnitPrice } from "./unit-price";
import type { ProcurementItemWithStats } from "../types";

export function ItemView({
  item,
  onClose,
  onEdit,
}: {
  item: ProcurementItemWithStats;
  onClose: () => void;
  onEdit: () => void;
}) {
  const { t } = useTranslation("procurement");
  const { isBalanceHidden } = useBalancePrivacy();
  const { formatDate } = useDateFormatting();
  const { formatAmount } = useAmountFormatting();
  const { data: taxonomy } = useTaxonomy("procurement_categories");
  const { data, isPending, isError } = useQuery({
    queryKey: [QueryKeys.PROCUREMENT_ITEM_PRICES, item.id],
    queryFn: () => getProcurementItemPrices(item.id),
  });
  const stats = data?.stats ?? item;
  const prices =
    data?.prices.map((point) => ({
      ...point,
      value: point.unitPriceBase === null ? null : Number(point.unitPriceBase),
    })) ?? [];
  const category = taxonomy?.categories.find(
    (c) => c.id === (item.subcategoryId ?? item.categoryId),
  );
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="grid max-h-[90dvh] grid-cols-1 content-start gap-6 overflow-y-auto px-5 py-6 sm:max-w-2xl sm:px-6">
        <div className="flex items-center gap-3 pr-6">
          <ItemPhoto item={item} />
          <DialogHeader className="min-w-0 flex-1 text-left">
            <DialogTitle>{item.name}</DialogTitle>
            <DialogDescription className="flex items-center gap-2">
              <span className="min-w-0 truncate">
                {[item.brand, category?.name].filter(Boolean).join(" · ") || t("title")}
              </span>
              {item.rating !== null && (
                <StarRatingDisplay rating={item.rating} showText={false} className="shrink-0" />
              )}
            </DialogDescription>
          </DialogHeader>
        </div>
        {item.memo && <p className="whitespace-pre-wrap break-words text-sm">{item.memo}</p>}
        <dl className="grid grid-cols-3 gap-3">
          {(["minPrice", "avgPrice", "latestPrice"] as const).map((key) => (
            <div key={key} className="rounded-md border p-3">
              <dt className="text-muted-foreground text-xs">{t(key)}</dt>
              <dd className="mt-1 font-medium">
                <UnitPrice
                  value={stats[key]}
                  currency={stats.baseCurrency}
                  unit={item.unit}
                  isHidden={isBalanceHidden}
                />
              </dd>
            </div>
          ))}
        </dl>
        {isPending ? (
          <p role="status">{t("loading")}</p>
        ) : isError ? (
          <p role="alert" className="text-destructive">
            {t("loadFailed")}
          </p>
        ) : !prices.length ? (
          <p className="text-muted-foreground py-12 text-center">{t("noPurchases")}</p>
        ) : isBalanceHidden ? (
          <p className="py-12 text-center">••••</p>
        ) : (
          <ChartContainer
            config={{ value: { label: t("latestPrice"), color: "var(--chart-1)" } }}
            className="my-6 h-64 w-full"
          >
            <AreaChart data={prices} margin={{ top: 10, right: 15, left: 10, bottom: 0 }}>
              <defs>
                <linearGradient id="procurement-price-fill" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="var(--color-value)" stopOpacity={0.3} />
                  <stop offset="100%" stopColor="var(--color-value)" stopOpacity={0.03} />
                </linearGradient>
              </defs>
              <CartesianGrid vertical={false} />
              <XAxis dataKey="date" tickFormatter={(value: string) => formatDate(value)} />
              <YAxis
                domain={["auto", "auto"]}
                tickFormatter={(value: number) => formatAmount(value, stats.baseCurrency, false)}
              />
              <Tooltip
                content={({ active, payload }) => {
                  const entry = payload?.[0] as { payload?: (typeof prices)[number] } | undefined;
                  const point = entry?.payload;
                  return active && point ? (
                    <div className="bg-background rounded-md border p-3 text-sm shadow">
                      <p>{formatDate(point.date)}</p>
                      <p>{point.shop}</p>
                      <p>
                        <UnitPrice
                          value={point.unitPrice}
                          currency={point.currency}
                          unit={item.unit}
                        />
                      </p>
                      {point.unitPriceBase !== null && point.currency !== point.baseCurrency && (
                        <p className="text-muted-foreground">
                          <UnitPrice
                            value={point.unitPriceBase}
                            currency={point.baseCurrency}
                            unit={item.unit}
                          />
                        </p>
                      )}
                    </div>
                  ) : null;
                }}
              />
              <Area
                type="monotone"
                dataKey="value"
                stroke="var(--color-value)"
                fill="url(#procurement-price-fill)"
                connectNulls={false}
                dot={{ r: 3 }}
              />
              {stats.avgPrice !== null && (
                <ReferenceLine
                  y={Number(stats.avgPrice)}
                  stroke="var(--color-value)"
                  strokeDasharray="4 4"
                />
              )}
            </AreaChart>
          </ChartContainer>
        )}
        {stats.unconvertedCount > 0 && (
          <p role="status" className="text-muted-foreground text-sm">
            {t("missingFx")}
          </p>
        )}
        <DialogFooter>
          <Button onClick={onEdit}>{t("edit")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
