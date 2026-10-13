import { handleProfileStatus, profileHeaders } from "@/features/profiles/session";

import { uploadAbortError, type AttachmentUploadOptions } from "../shared/transaction-attachments";

const headerGetter = (xhr: XMLHttpRequest) => ({
  get: (name: string) => xhr.getResponseHeader(name),
});

function failure(body: string) {
  try {
    const parsed: unknown = JSON.parse(body);
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      "message" in parsed &&
      typeof parsed.message === "string"
    )
      return parsed.message;
  } catch {
    /* a non-JSON error body carries nothing better than the generic message */
  }
  return "Attachment request failed";
}

/**
 * POST a file and report upload progress.
 *
 * `fetch` cannot observe request-body progress, so this is the one place in the web
 * adapter that talks to `XMLHttpRequest` directly. That bypasses `profileFetch`, so
 * profile authority is applied from `profileHeaders()` and every response is put
 * through `handleProfileStatus()`.
 */
export function postFileWithProgress<T>(
  url: string,
  file: File,
  { onProgress, signal }: AttachmentUploadOptions = {},
): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    if (signal?.aborted) {
      reject(uploadAbortError());
      return;
    }

    const xhr = new XMLHttpRequest();
    xhr.open("POST", url);
    for (const [name, value] of Object.entries(profileHeaders())) xhr.setRequestHeader(name, value);

    const onAbort = () => xhr.abort();
    signal?.addEventListener("abort", onAbort);
    const settle = (run: () => void) => {
      signal?.removeEventListener("abort", onAbort);
      run();
    };

    if (onProgress)
      xhr.upload.onprogress = (event) => {
        if (event.lengthComputable && event.total > 0) onProgress(event.loaded / event.total);
      };

    xhr.onload = () =>
      settle(() => {
        const ok = xhr.status >= 200 && xhr.status < 300;
        try {
          handleProfileStatus({ ok, status: xhr.status, headers: headerGetter(xhr) });
        } catch (error) {
          reject(error instanceof Error ? error : new Error(String(error)));
          return;
        }
        if (!ok) {
          reject(new Error(failure(xhr.responseText)));
          return;
        }
        resolve(JSON.parse(xhr.responseText) as T);
      });

    xhr.onerror = () => settle(() => reject(new Error("Attachment request failed")));
    xhr.onabort = () => settle(() => reject(uploadAbortError()));

    const body = new FormData();
    body.append("file", file);
    xhr.send(body);
  });
}
