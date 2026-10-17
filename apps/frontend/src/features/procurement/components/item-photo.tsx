import { useEffect, useState } from "react";
import { Icons } from "@wealthfolio/ui";
import { Avatar, AvatarFallback, AvatarImage } from "@wealthfolio/ui/components/ui/avatar";
import { fetchProcurementPhoto } from "@/adapters";
import type { ProcurementItem } from "../types";

export function ItemPhoto({
  item,
  className = "size-16 shrink-0 rounded-full object-cover",
}: {
  item: ProcurementItem;
  className?: string;
}) {
  const [url, setUrl] = useState<string>();
  const photoKey = item.photo?.key;
  useEffect(() => {
    let disposed = false;
    let objectUrl: string | undefined;
    setUrl(undefined);
    if (photoKey) {
      void fetchProcurementPhoto(item.id)
        .then((blob) => {
          if (disposed) return;
          objectUrl = URL.createObjectURL(blob);
          setUrl(objectUrl);
        })
        .catch(() => {
          if (!disposed) setUrl(undefined);
        });
    }
    return () => {
      disposed = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [item.id, photoKey]);
  return (
    <Avatar className={className}>
      <AvatarImage src={url} alt={item.name} className="rounded-full object-cover" />
      <AvatarFallback className="text-muted-foreground">
        <Icons.ImageUp className="size-6" aria-hidden="true" />
      </AvatarFallback>
    </Avatar>
  );
}
