import { type ReactNode, useId, useRef, useState } from "react";

import { cn } from "../../lib/utils";
import { Icons } from "./icons";

export interface FileDropzoneProps {
  onFiles: (files: File[]) => void;
  ariaLabel: string;
  label: ReactNode;
  hint?: ReactNode;
  description?: ReactNode;
  accept?: string;
  multiple?: boolean;
  disabled?: boolean;
  icon?: ReactNode;
  id?: string;
  "aria-describedby"?: string;
  "data-testid"?: string;
  className?: string;
}

/**
 * Click-or-drop file picker.
 */
export function FileDropzone({
  onFiles,
  ariaLabel,
  label,
  hint,
  description,
  accept,
  multiple = false,
  disabled = false,
  icon,
  id,
  "aria-describedby": describedBy,
  "data-testid": testId,
  className,
}: FileDropzoneProps) {
  const [isDragging, setIsDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const descriptionId = useId();
  const describedByIds = [describedBy, description ? descriptionId : undefined].filter(Boolean).join(" ") || undefined;

  const openPicker = () => {
    if (!disabled) inputRef.current?.click();
  };

  const emit = (files: FileList | null | undefined) => {
    const picked = Array.from(files ?? []);
    if (picked.length) onFiles(picked);
  };

  return (
    <div
      role="button"
      tabIndex={disabled ? -1 : 0}
      aria-disabled={disabled}
      aria-label={ariaLabel}
      aria-describedby={describedByIds}
      data-testid={testId}
      onClick={openPicker}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          openPicker();
        }
      }}
      onDragOver={(event) => {
        event.preventDefault();
        event.stopPropagation();
        if (!disabled) setIsDragging(true);
      }}
      onDragLeave={(event) => {
        event.preventDefault();
        event.stopPropagation();
        setIsDragging(false);
      }}
      onDrop={(event) => {
        event.preventDefault();
        event.stopPropagation();
        setIsDragging(false);
        if (!disabled) emit(event.dataTransfer.files);
      }}
      className={cn(
        "focus-visible:ring-ring group flex flex-col items-center justify-center gap-2 rounded-xl border border-dashed px-4 py-6 text-center transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2",
        disabled
          ? "border-border bg-muted/30 cursor-default opacity-60"
          : "cursor-pointer " +
              (isDragging
                ? "border-primary bg-primary/5"
                : "border-muted-foreground/25 hover:border-muted-foreground/50 hover:bg-muted/40"),
        className,
      )}
    >
      <input
        ref={inputRef}
        id={id}
        type="file"
        accept={accept}
        multiple={multiple}
        disabled={disabled}
        className="hidden"
        onChange={(event) => {
          emit(event.target.files);
          // Let the same file be picked again after an error or a removal.
          event.target.value = "";
        }}
      />
      <span
        aria-hidden
        className={cn(
          "flex size-10 items-center justify-center rounded-full transition-colors",
          isDragging && !disabled ? "bg-primary/10 text-primary" : "bg-muted text-muted-foreground",
        )}
      >
        {icon ?? <Icons.Upload className="size-5" />}
      </span>
      <span className="text-xs font-medium">
        <span className={cn(!disabled && "text-primary")}>{label}</span>
        {hint ? <span className="text-muted-foreground"> {hint}</span> : null}
      </span>
      {description ? (
        <span id={descriptionId} className="text-muted-foreground text-xs">
          {description}
        </span>
      ) : null}
    </div>
  );
}
