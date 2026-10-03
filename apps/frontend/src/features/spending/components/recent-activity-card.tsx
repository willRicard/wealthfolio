import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";

import { DashboardCard } from "@/components/dashboard-card";
import { formatZonedDateKey } from "@/features/spending/lib/timezone";
import { cn, formatDateISO } from "@/lib/utils";
import { PrivacyAmount, useDateFormatting, useLocalizationSettings } from "@wealthfolio/ui";

import type { CashActivity } from "../types/cash-activity";
import {
  getActivitySpendingAmount,
  getEffectiveCashActivityType,
  isCashActivityIncome,
} from "../lib/constants";
import { CategoryBadge, ReviewPill, type CategoryMetaMap } from "./category-chips";

const SPENDING_TAXONOMY = "spending_categories";

export function RecentActivityCard({
  activities,
  accountTypeById,
  categoriesMeta,
  uncategorizedCount = 0,
}: {
  activities: CashActivity[];
  accountTypeById?: Map<string, string>;
  categoriesMeta: CategoryMetaMap;
  uncategorizedCount?: number;
}) {
  const formatting = useDateFormatting();
  const { timezone } = useLocalizationSettings();
  const { t } = useTranslation();
  const recent = useMemo(() => {
    return activities
      .slice()
      .filter((activity) => {
        const accountType = accountTypeById?.get(activity.accountId);
        const activityType = getEffectiveCashActivityType(activity);
        return (
          getActivitySpendingAmount(activity, accountType) !== 0 ||
          isCashActivityIncome(activityType, accountType, activity.subtype)
        );
      })
      .sort((a, b) => b.activityDate.localeCompare(a.activityDate))
      .slice(0, 10);
  }, [activities, accountTypeById]);

  const badgesByActivityId = useMemo(() => {
    const out = new Map<
      string,
      { id: string; name: string; color: string | null; icon: string | null }[]
    >();
    recent.forEach((a) => {
      const splitCategoryIds = [
        ...new Set(
          a.splits
            .filter((split) => split.taxonomyId === SPENDING_TAXONOMY)
            .map((split) => split.categoryId),
        ),
      ];
      if (splitCategoryIds.length > 0) {
        out.set(
          a.id,
          splitCategoryIds.flatMap((id) => {
            const meta = categoriesMeta.get(id);
            if (!meta) return [];
            const parent = meta.parentId ? categoriesMeta.get(meta.parentId) : undefined;
            return [
              {
                id,
                name: meta.name,
                color: meta.color ?? parent?.color ?? null,
                icon: meta.icon ?? parent?.icon ?? null,
              },
            ];
          }),
        );
        return;
      }
      const spending = a.assignments.find((x) => x.taxonomyId === SPENDING_TAXONOMY);
      if (!spending) {
        out.set(a.id, []);
        return;
      }
      const meta = categoriesMeta.get(spending.categoryId);
      const topId = meta?.parentId ?? spending.categoryId;
      const top = categoriesMeta.get(topId) ?? meta;
      if (!top) {
        out.set(a.id, []);
        return;
      }
      out.set(a.id, [
        {
          id: topId,
          name: top.name,
          color: top.color,
          icon: meta?.icon ?? top.icon,
        },
      ]);
    });
    return out;
  }, [recent, categoriesMeta]);

  const grouped = useMemo(() => {
    const m = new Map<string, typeof recent>();
    for (const a of recent) {
      const dateKey = formatZonedDateKey(new Date(a.activityDate), timezone);
      const arr = m.get(dateKey) ?? [];
      arr.push(a);
      m.set(dateKey, arr);
    }
    return Array.from(m.entries());
  }, [recent, timezone]);

  const dayLabel = (key: string): string => {
    const today = new Date();
    const todayKey = formatZonedDateKey(today, timezone);
    const yest = new Date(`${todayKey}T12:00:00`);
    yest.setDate(yest.getDate() - 1);
    const yestKey = formatDateISO(yest);
    if (key === todayKey) return t("spending:dashboard.today");
    if (key === yestKey) return t("spending:dashboard.yesterday");
    return formatting.formatCalendarDate(key, {
      weekday: "short",
      month: "short",
      day: "numeric",
    });
  };

  return (
    <DashboardCard
      title={t("spending:dashboard.recentActivity")}
      padded={false}
      className="overflow-hidden"
      action={
        <Link
          to={
            uncategorizedCount > 0
              ? "/activities?tab=spending&status=uncategorized"
              : "/activities?tab=spending"
          }
          className="text-muted-foreground hover:text-foreground text-xs underline-offset-4 hover:underline"
        >
          {uncategorizedCount > 0
            ? t("spending:dashboard.viewAllToTag", { count: uncategorizedCount })
            : t("spending:dashboard.viewAll")}
        </Link>
      }
    >
      {recent.length === 0 ? (
        <div className="text-muted-foreground px-4 py-6 text-center text-xs md:px-5">
          {t("spending:dashboard.noRecentActivity")}
        </div>
      ) : (
        grouped.map(([dateKey, items], gi) => (
          <div
            key={dateKey}
            className={cn("px-4 py-3 md:px-5", gi > 0 && "border-border/60 border-t")}
          >
            <div className="text-muted-foreground/70 text-[10px] font-semibold uppercase tracking-wide">
              {dayLabel(dateKey)}
            </div>
            {items.map((a) => {
              const payee = (a.notes ?? "").trim();
              const spendingAmount = getActivitySpendingAmount(
                a,
                accountTypeById?.get(a.accountId),
              );
              const isOutflow = spendingAmount > 0;
              const nativeAmount =
                spendingAmount === 0 ? parseFloat(a.amount ?? "0") || 0 : Math.abs(spendingAmount);
              const parsedBaseAmount = parseFloat(a.baseAmount ?? "");
              const hasBaseAmount = Number.isFinite(parsedBaseAmount);
              const amount = hasBaseAmount ? Math.abs(parsedBaseAmount) : nativeAmount;
              const displayCurrency = hasBaseAmount ? (a.baseCurrency ?? a.currency) : a.currency;
              const badges = badgesByActivityId.get(a.id) ?? [];
              const needsReview = a.needsReview || (isOutflow && badges.length === 0);

              return (
                // Single transaction row → activities page filtered to this
                // payee (or status=uncategorized when there's no payee +
                // it's flagged for review). Matches the clickable behavior
                // of every neighboring spending widget (ranked bar rows,
                // treemap cells, budget rings) so this row no longer feels
                // like a dead row sandwiched between live ones.
                <Link
                  key={a.id}
                  to={
                    needsReview && !payee
                      ? "/activities?tab=spending&status=uncategorized"
                      : payee
                        ? `/activities?tab=spending&q=${encodeURIComponent(payee)}`
                        : "/activities?tab=spending"
                  }
                  className="hover:bg-muted/40 flex items-center gap-2.5 rounded-md py-1.5 transition-colors"
                >
                  <div className="min-w-0 flex-1">
                    <div className="text-foreground/90 truncate text-xs font-medium">
                      {payee || (
                        <span className="text-muted-foreground italic">
                          {t("spending:dashboard.noPayee")}
                        </span>
                      )}
                    </div>
                  </div>
                  {badges.length > 0 ? (
                    <div className="flex flex-wrap justify-end gap-1">
                      {badges.map((badge) => (
                        <CategoryBadge
                          key={badge.id}
                          name={badge.name}
                          color={badge.color}
                          icon={badge.icon}
                        />
                      ))}
                    </div>
                  ) : needsReview ? (
                    <ReviewPill label={t("spending:dashboard.uncategorized")} />
                  ) : null}
                  <div
                    className={cn(
                      "shrink-0 text-xs font-semibold tabular-nums",
                      isOutflow ? "text-foreground" : "text-success",
                    )}
                  >
                    {isOutflow ? "−" : "+"}
                    <PrivacyAmount value={amount} currency={displayCurrency} />
                  </div>
                </Link>
              );
            })}
          </div>
        ))
      )}
    </DashboardCard>
  );
}
