import { useState } from "react";
import { useQueries } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts";
import {
  Button,
  IntervalSelector,
  useAmountFormatting,
  useDateFormatting,
  useNumberFormatting,
} from "@wealthfolio/ui";
import type { DateRange } from "@wealthfolio/ui/components/financial/interval-selector";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@wealthfolio/ui/components/ui/dialog";
import {
  ChartContainer,
  ChartTooltip,
  ChartLegend,
  ChartLegendContent,
} from "@wealthfolio/ui/components/ui/chart";
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from "@wealthfolio/ui/components/ui/table";
import { getProcurementItemPrices } from "@/adapters";
import { PERFORMANCE_CHART_COLORS } from "@/components/performance-chart-colors";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { QueryKeys } from "@/lib/query-keys";
import type { ProcurementItemWithStats } from "../types";
import { UnitPrice } from "./unit-price";

interface ItemsCompareProps {
  items: ProcurementItemWithStats[];
  onClose: () => void;
}

export function ItemsCompare({ items, onClose }: ItemsCompareProps) {
  const { t } = useTranslation("procurement");
  const { isBalanceHidden } = useBalancePrivacy();
  const { formatDate } = useDateFormatting();
  const { formatAmount } = useAmountFormatting();
  const { formatPercent } = useNumberFormatting();
  const [range, setRange] = useState<DateRange>();
  const results = useQueries({
    queries: items.map((item) => ({
      queryKey: [QueryKeys.PROCUREMENT_ITEM_PRICES, item.id],
      queryFn: () => getProcurementItemPrices(item.id),
    })),
  });
  const series = items.map((item, i) => {
    const points = (results[i].data?.prices ?? [])
      .filter((point) => point.unitPriceBase !== null)
      .map((point) => ({
        ...point,
        time: new Date(point.date).getTime(),
        value: Number(point.unitPriceBase),
      }))
      .filter(
        (point) =>
          (!range?.from || point.time >= range.from.getTime()) &&
          (!range?.to || point.time <= range.to.getTime()),
      )
      .sort((a, b) => a.time - b.time);
    const values = points.map((point) => point.value);
    const first = values[0];
    const latest = values.at(-1) ?? null;
    return {
      key: `s${i}`,
      item,
      currency: results[i].data?.stats.baseCurrency ?? item.baseCurrency,
      color: PERFORMANCE_CHART_COLORS[i % PERFORMANCE_CHART_COLORS.length],
      points,
      min: values.length ? Math.min(...values) : null,
      avg: values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : null,
      latest,
      change:
        first !== undefined && first !== 0 && latest !== null ? (latest - first) / first : null,
    };
  });
  const config = Object.fromEntries(
    series.map((s) => [
      s.key,
      {
        label: `${s.item.name} (${s.currency}/${s.item.unit})`,
        color: s.color,
      },
    ]),
  );
  // Purchases are irregular: retain every series' dates on a numeric time axis.
  const byTime = series.map((s) => new Map(s.points.map((point) => [point.time, point])));
  const times = [...new Set(series.flatMap((s) => s.points.map((point) => point.time)))].sort(
    (a, b) => a - b,
  );
  const rows = times.map((time) => ({
    time,
    ...Object.fromEntries(series.map((s, i) => [s.key, byTime[i].get(time)?.value ?? null])),
  }));
  const hasPurchases = results.some((result) => result.data?.prices.length);
  const mixedUnits = new Set(items.map((item) => item.unit)).size > 1;
  const pending = results.some((result) => result.isPending);
  const failed = results.some((result) => result.isError);
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="grid max-h-[90dvh] min-w-0 grid-cols-1 gap-6 overflow-y-auto sm:max-w-4xl">
        <DialogHeader className="pe-6 text-start">
          <DialogTitle>{t("compareTitle")}</DialogTitle>
          <DialogDescription>{t("compareDescription")}</DialogDescription>
        </DialogHeader>
        <IntervalSelector
          defaultValue="ALL"
          onIntervalSelect={(code, _description, nextRange) =>
            setRange(code === "ALL" ? undefined : nextRange)
          }
        />
        {pending ? (
          <p role="status">{t("loading")}</p>
        ) : failed ? (
          <p role="alert" className="text-destructive">
            {t("loadFailed")}
          </p>
        ) : !rows.length ? (
          <p className="text-muted-foreground py-12 text-center">
            {t(hasPurchases ? "noPricesInRange" : "noPurchases")}
          </p>
        ) : isBalanceHidden ? (
          <p className="py-12 text-center">••••</p>
        ) : (
          <ChartContainer config={config} className="h-72 w-full">
            <LineChart data={rows}>
              <CartesianGrid vertical={false} />
              <XAxis
                dataKey="time"
                type="number"
                scale="time"
                domain={["dataMin", "dataMax"]}
                tickFormatter={(value: number) => formatDate(new Date(value).toISOString())}
              />
              <YAxis
                tickFormatter={(value: number) => formatAmount(value, series[0].currency, false)}
              />
              <ChartTooltip
                content={({ active, payload }) => {
                  const time = payload?.[0]?.payload?.time as number | undefined;
                  if (!active || time === undefined) return null;
                  return (
                    <div className="bg-background rounded-md border p-3 text-sm shadow">
                      <p>{formatDate(new Date(time).toISOString())}</p>
                      {series.map((s, i) => {
                        const point = byTime[i].get(time);
                        return point ? (
                          <div key={s.key} className="mt-2">
                            <p>{s.item.name}</p>
                            <UnitPrice
                              value={point.unitPriceBase}
                              currency={s.currency}
                              unit={s.item.unit}
                            />
                            {point.shop && <p className="text-muted-foreground">{point.shop}</p>}
                          </div>
                        ) : null;
                      })}
                    </div>
                  );
                }}
              />
              <ChartLegend content={<ChartLegendContent payload={[]} className="flex-wrap" />} />
              {series.map((s) => (
                <Line
                  key={s.key}
                  dataKey={s.key}
                  stroke={s.color}
                  connectNulls
                  dot={{ r: 3 }}
                  type="monotone"
                />
              ))}
            </LineChart>
          </ChartContainer>
        )}
        {mixedUnits && (
          <p role="status" className="text-muted-foreground text-sm">
            {t("mixedUnits")}
          </p>
        )}
        {results.some((result) => (result.data?.stats.unconvertedCount ?? 0) > 0) && (
          <p role="status" className="text-muted-foreground text-sm">
            {t("missingFx")}
          </p>
        )}
        {!pending && !failed && (
          <Table>
            <TableHeader>
              <TableRow>
                {["item", "minPrice", "avgPrice", "latestPrice", "change"].map((key, i) => (
                  <TableHead key={key} className={i ? "text-end" : "text-start"}>
                    {t(key)}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {series.map((s) => (
                <TableRow key={s.key}>
                  <TableCell>
                    <span className="inline-flex items-center gap-2">
                      <span
                        className="size-2 shrink-0 rounded-full"
                        style={{ backgroundColor: s.color }}
                      />
                      {s.item.name}
                    </span>
                  </TableCell>
                  {[s.min, s.avg, s.latest].map((value, i) => (
                    <TableCell key={i} className="text-end">
                      {isBalanceHidden ? (
                        "••••"
                      ) : (
                        <UnitPrice
                          value={value === null ? null : String(value)}
                          currency={s.currency}
                          unit={s.item.unit}
                        />
                      )}
                    </TableCell>
                  ))}
                  <TableCell className="text-end">
                    {isBalanceHidden ? "••••" : s.change === null ? "—" : formatPercent(s.change)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
