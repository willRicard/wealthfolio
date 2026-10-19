import { AmountDisplay } from "@wealthfolio/ui";

interface UnitPriceProps {
  /** Decimal string from the backend, or null when there is no price yet. */
  value: string | null;
  currency: string;
  unit: string;
  isHidden?: boolean;
  className?: string;
}

export function UnitPrice({ value, currency, unit, isHidden, className }: UnitPriceProps) {
  if (value === null) return <>—</>;
  return (
    <span className={className}>
      <AmountDisplay value={Number(value)} currency={currency} isHidden={isHidden} />
      <span className="text-muted-foreground">/{unit}</span>
    </span>
  );
}
