import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { cn, Icons } from "@wealthfolio/ui";
import { fetchProcurementPhoto } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { ProcurementItem } from "../types";

async function loadPhoto(id: string): Promise<string> {
  const blob = await fetchProcurementPhoto(id);
  const url = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result === "string") resolve(reader.result);
      else reject(new Error("Could not read procurement thumbnail"));
    };
    reader.onerror = () =>
      reject(reader.error ?? new Error("Could not read procurement thumbnail"));
    reader.readAsDataURL(blob);
  });
  const image = new Image();
  image.src = url;
  await image.decode();
  return url;
}

export function ItemPhoto({
  item,
  className = "size-16 shrink-0 rounded-full object-cover",
}: {
  item: ProcurementItem;
  className?: string;
}) {
  const photoKey = item.photo?.key;
  const { data: url } = useQuery({
    queryKey: [QueryKeys.PROCUREMENT_ITEM_PHOTO, item.id, photoKey],
    queryFn: () => loadPhoto(item.id),
    enabled: !!photoKey,
    staleTime: Infinity,
  });
  const [failedUrl, setFailedUrl] = useState<string>();
  return (
    <span className={cn("relative flex items-center justify-center overflow-hidden", className)}>
      {url && url !== failedUrl ? (
        <img
          src={url}
          alt={item.name}
          className="size-full rounded-full object-cover"
          onError={() => setFailedUrl(url)}
        />
      ) : (
        <span className="bg-muted text-muted-foreground flex size-full items-center justify-center rounded-full">
          <Icons.ImageUp className="size-6" aria-hidden="true" />
        </span>
      )}
    </span>
  );
}
