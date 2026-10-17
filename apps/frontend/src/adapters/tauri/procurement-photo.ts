import { invoke, invokeRaw } from "./core";
import type { ProcurementPhotoDraft } from "@/features/procurement/types";

export async function uploadProcurementPhotoDraft(file: File): Promise<ProcurementPhotoDraft> {
  return invokeRaw<ProcurementPhotoDraft>(
    "upload_procurement_photo_draft",
    await file.arrayBuffer(),
    {
      "x-wf-filename": encodeURIComponent(file.name),
      "x-wf-content-type": file.type,
    },
  );
}
export async function fetchProcurementPhoto(id: string): Promise<Blob> {
  const value = await invoke<ArrayBuffer | number[]>("read_procurement_photo", { id });
  const bytes = value instanceof ArrayBuffer ? new Uint8Array(value) : Uint8Array.from(value);
  return new Blob([bytes], { type: "image/webp" });
}
