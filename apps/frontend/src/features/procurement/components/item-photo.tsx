import { useQuery } from "@tanstack/react-query";
import { Icons, useObjectUrl } from "@wealthfolio/ui";
import { Avatar, AvatarFallback, AvatarImage } from "@wealthfolio/ui/components/ui/avatar";
import { fetchProcurementPhoto } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { ProcurementItem } from "../types";

export function ItemPhoto({
  item,
  className = "size-16 shrink-0 rounded-full object-cover",
}: {
  item: ProcurementItem;
  className?: string;
}) {
  const photoKey = item.photo?.key;
  const { data: blob } = useQuery({
    queryKey: [QueryKeys.PROCUREMENT_ITEM_PHOTO, item.id, photoKey],
    queryFn: () => fetchProcurementPhoto(item.id),
    enabled: !!photoKey,
    staleTime: Infinity,
  });
  const url = useObjectUrl(blob);
  return (
    <Avatar className={className}>
      <AvatarImage src={url} alt={item.name} className="rounded-full object-cover" />
      <AvatarFallback className="text-muted-foreground">
        <Icons.ImageUp className="size-6" aria-hidden="true" />
      </AvatarFallback>
    </Avatar>
  );
}
