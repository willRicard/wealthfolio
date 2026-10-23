import { Fragment, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icons } from "@wealthfolio/ui";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@wealthfolio/ui/components/ui/dropdown-menu";
import { ActionPalette, type ActionPaletteGroup } from "@/components/action-palette";
import { HOVER_SLOT } from "@/lib/hover-slot";
import { cn } from "@/lib/utils";
import type { ProcurementItemWithStats } from "../types";

interface ItemActionsProps {
  item: ProcurementItemWithStats;
  compact?: boolean;
  pending: boolean;
  onView: (item: ProcurementItemWithStats) => void;
  onEdit: (item: ProcurementItemWithStats) => void;
  onClone: (item: ProcurementItemWithStats) => void;
  onDelete: (item: ProcurementItemWithStats) => void;
}

export function ItemActions({
  item,
  compact,
  pending,
  onView,
  onEdit,
  onClone,
  onDelete,
}: ItemActionsProps) {
  const { t } = useTranslation("procurement");
  const [open, setOpen] = useState(false);
  const groups: ActionPaletteGroup[] = [
    {
      items: [
        {
          icon: Icons.Info,
          label: t("activity:operations.more_details"),
          onClick: () => onView(item),
        },
      ],
    },
    {
      items: [
        { icon: Icons.Pencil, label: t("edit"), onClick: () => onEdit(item) },
        { icon: Icons.Copy, label: t("clone"), disabled: pending, onClick: () => onClone(item) },
      ],
    },
    {
      items: [
        {
          icon: Icons.Trash,
          label: t("delete"),
          variant: "destructive",
          disabled: pending,
          onClick: () => onDelete(item),
        },
      ],
    },
  ];
  const trigger = (
    <button
      type="button"
      aria-label={`${t("actions")}: ${item.name}`}
      className={cn(
        "hover:bg-muted flex h-7 w-7 shrink-0 items-center justify-center rounded-md transition-colors",
        compact ? "-mr-1" : cn("data-[state=open]:opacity-100", HOVER_SLOT),
      )}
    >
      <Icons.MoreVertical className="size-4" />
    </button>
  );
  return compact ? (
    <ActionPalette open={open} onOpenChange={setOpen} groups={groups} trigger={trigger} />
  ) : (
    <DropdownMenu>
      <DropdownMenuTrigger render={trigger} />
      <DropdownMenuContent align="end">
        {groups.map((group, index) => (
          <Fragment key={index}>
            {index > 0 && <DropdownMenuSeparator />}
            {group.items.map((action) => (
              <DropdownMenuItem
                key={action.label}
                disabled={action.disabled}
                className={cn(
                  action.variant === "destructive" &&
                    "text-destructive focus:text-destructive flex cursor-pointer items-center",
                )}
                onClick={action.onClick}
              >
                <action.icon className="mr-2 size-4" />
                {action.label}
              </DropdownMenuItem>
            ))}
          </Fragment>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
