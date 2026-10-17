import { profileFetch } from "@/features/profiles/session";
import type { ProcurementPhotoDraft } from "@/features/procurement/types";
import { API_PREFIX } from "./core";
import { postFileWithProgress } from "./upload";

export function uploadProcurementPhotoDraft(file: File): Promise<ProcurementPhotoDraft> {
  return postFileWithProgress<ProcurementPhotoDraft>(
    `${API_PREFIX}/procurement/photo-drafts`,
    file,
  );
}
export async function fetchProcurementPhoto(id: string): Promise<Blob> {
  const response = await profileFetch(
    `${API_PREFIX}/procurement/items/${encodeURIComponent(id)}/photo`,
  );
  if (!response.ok) throw new Error("Photo request failed");
  return response.blob();
}
