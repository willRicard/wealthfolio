import { useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Button,
  Icons,
  MonthYearPicker,
  Popover,
  PopoverContent,
  PopoverTrigger,
  useDateFormatting,
} from "@wealthfolio/ui";

import { monthLabel } from "../lib/month-period";

interface SpendingBaseMonthPickerProps {
  value: string;
  maxMonth: string;
  onChange: (monthKey: string) => void;
}

export function SpendingBaseMonthPicker({
  value,
  maxMonth,
  onChange,
}: SpendingBaseMonthPickerProps) {
  const { t } = useTranslation();
  const dateFormatting = useDateFormatting();
  const [open, setOpen] = useState(false);

  const trigger = (
    <Button
      type="button"
      variant="ghost"
      size="sm"
      className="h-8 gap-1.5 px-2 text-xs"
      aria-label={t("spending:insightsPage.chooseBaseMonth")}
    >
      <span className="text-muted-foreground">{t("spending:insightsPage.comparedWith")}</span>
      <span className="font-medium">{monthLabel(value, dateFormatting, "short")}</span>
      <Icons.ChevronDown className="h-3.5 w-3.5" />
    </Button>
  );

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger render={trigger} />
      <PopoverContent align="start" className="w-auto p-4">
        <MonthYearPicker
          value={value}
          maxDate={maxMonth}
          className="w-full max-w-none p-0 [&_.grid_button]:h-11"
          onChange={(month) => {
            onChange(month);
            setOpen(false);
          }}
        />
      </PopoverContent>
    </Popover>
  );
}
