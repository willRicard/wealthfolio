import { useQuery } from "@tanstack/react-query";

import { QueryKeys } from "@/lib/query-keys";

import { getSpendingReport } from "../adapters/reports";
import type { MonthlyReport, ReportRequest } from "../types/report";

export function useSpendingReport(request: ReportRequest, enabled = true, baseCurrency?: string) {
  return useQuery<MonthlyReport, Error>({
    queryKey: [QueryKeys.SPENDING_REPORT, request, baseCurrency ?? null],
    queryFn: () => getSpendingReport(request),
    enabled,
  });
}
