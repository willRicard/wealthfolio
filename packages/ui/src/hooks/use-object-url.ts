import { useEffect, useState } from "react";

/**
 * Object URL for a `Blob` or `File`, revoked when the source changes and on unmount.
 */
export function useObjectUrl(source: Blob | undefined): string | undefined {
  const [url, setUrl] = useState<string>();

  useEffect(() => {
    if (!source) {
      setUrl(undefined);
      return;
    }
    const objectUrl = URL.createObjectURL(source);
    setUrl(objectUrl);
    return () => URL.revokeObjectURL(objectUrl);
  }, [source]);

  return url;
}
