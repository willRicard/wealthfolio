import type { ReactNode } from "react";
import { Icons, useDateFormatting } from "@wealthfolio/ui";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import type { ProcurementItemWithStats } from "../types";
import { UnitPrice } from "./unit-price";

interface ItemCompactProps {
  item: ProcurementItemWithStats;
  category: ReactNode;
  actions: ReactNode;
  onView: (item: ProcurementItemWithStats) => void;
}

export function ItemCompact({ item, category, actions, onView }: ItemCompactProps) {
  const { isBalanceHidden } = useBalancePrivacy();
  const { formatDate } = useDateFormatting();
  return (
    <div className="w-full min-w-0 max-w-[calc(100vw-7rem)] space-y-1 py-1">
      <div className="flex min-w-0 items-center gap-2">
        <button
          className="min-w-0 flex-1 truncate text-left font-medium"
          onClick={() => onView(item)}
        >
          {item.name}
        </button>
        <span className="shrink-0 font-medium">
          <UnitPrice
            value={item.latestPrice}
            currency={item.baseCurrency}
            unit={item.unit}
            isHidden={isBalanceHidden}
          />
        </span>
        {actions}
      </div>
      <div className="text-muted-foreground flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
        <span>{item.latestDate ? formatDate(item.latestDate) : "—"}</span>
        {category}
        {item.rating !== null && (
          <span className="inline-flex items-center gap-1">
            <Icons.Star className="size-3 fill-current text-yellow-500" />
            {item.rating}
          </span>
        )}
        {item.brand && <span className="min-w-0 truncate">{item.brand}</span>}
      </div>
    </div>
  );
}
