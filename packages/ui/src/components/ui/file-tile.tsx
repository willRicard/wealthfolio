import { type ReactNode } from "react";

import { cn } from "../../lib/utils";
import { Button } from "./button";
import { Icons } from "./icons";
import { Progress } from "./progress";

export interface FileTileProps {
  name: string;
  preview?: ReactNode;
  caption?: ReactNode;
  progress?: number;
  indeterminate?: boolean;
  error?: string;
  onOpen?: () => void;
  openLabel?: string;
  onRetry?: () => void;
  retryLabel?: string;
  onCancel?: () => void;
  cancelLabel?: string;
  onRemove?: () => void;
  removeLabel?: string;
  disabled?: boolean;
  className?: string;
  "data-testid"?: string;
}

/**
 * One file in an attachment list.
 */
export function FileTile({
  name,
  preview,
  caption,
  progress,
  indeterminate = false,
  error,
  onOpen,
  openLabel,
  onRetry,
  retryLabel,
  onCancel,
  cancelLabel,
  onRemove,
  removeLabel,
  disabled = false,
  className,
  "data-testid": testId,
}: FileTileProps) {
  const busy = indeterminate || (!error && typeof progress === "number" && progress < 1);
  const percent = Math.round((progress ?? 0) * 100);
  const fallback = (
    <span className="bg-muted text-muted-foreground flex h-24 w-full items-center justify-center rounded-md">
      <Icons.FileText className="size-7" aria-hidden />
    </span>
  );

  return (
    <div
      data-testid={testId}
      className={cn(
        "bg-card flex min-w-0 flex-col overflow-hidden rounded-xl border transition-colors",
        error && "border-destructive/50",
        className,
      )}
    >
      {onOpen ? (
        <button
          type="button"
          onClick={onOpen}
          disabled={disabled}
          aria-label={openLabel}
          className="focus-visible:ring-ring rounded-t-xl p-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset disabled:opacity-60"
        >
          {preview ?? fallback}
        </button>
      ) : (
        <div className="p-2">{preview ?? fallback}</div>
      )}

      <div className="flex min-w-0 items-start gap-1 px-2 pb-1">
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm" title={name}>
            {name}
          </p>
          {error ? (
            <p role="alert" className="text-destructive truncate text-xs" title={error}>
              {error}
            </p>
          ) : caption ? (
            <p className="text-muted-foreground truncate text-xs">{caption}</p>
          ) : null}
        </div>
        {busy && <Icons.Spinner className="text-muted-foreground mt-0.5 size-3.5 shrink-0 animate-spin" aria-hidden />}
      </div>

      {busy && !indeterminate && (
        <Progress value={percent} aria-valuenow={percent} aria-label={name} className="h-1 rounded-none" />
      )}
      {busy && indeterminate && (
        <div aria-hidden className="bg-secondary h-1 overflow-hidden">
          <div className="bg-primary h-full w-1/3 animate-pulse rounded-full" />
        </div>
      )}

      {(onRetry || onCancel || onRemove) && (
        <div className="flex items-center justify-end gap-1 px-1 pb-1">
          {onRetry && (
            <Button type="button" variant="ghost" size="sm" disabled={disabled} onClick={onRetry}>
              {retryLabel}
            </Button>
          )}
          {onCancel && (
            <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
              {cancelLabel}
            </Button>
          )}
          {onRemove && (
            <Button type="button" variant="ghost" size="sm" disabled={disabled} onClick={onRemove}>
              {removeLabel}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
