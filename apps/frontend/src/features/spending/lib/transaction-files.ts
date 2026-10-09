export const MAX_TRANSACTION_FILE_BYTES = 20 * 1024 * 1024;
export const MAX_TRANSACTION_FILES = 10;
const CONTENT_TYPES = new Set(["image/jpeg", "image/png", "image/webp", "application/pdf"]);

export function transactionFileError(files: File[], existingCount: number): string | undefined {
  if (files.length + existingCount > MAX_TRANSACTION_FILES) return "fileCountError";
  if (files.some((file) => file.size === 0 || file.size > MAX_TRANSACTION_FILE_BYTES))
    return "fileSizeError";
  if (files.some((file) => !CONTENT_TYPES.has(file.type))) return "fileTypeError";
  return undefined;
}
