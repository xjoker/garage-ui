export function copyText(value: string, copyFallback: () => boolean): Promise<"copied" | "manual"> {
  const clipboard = navigator.clipboard;
  if (!window.isSecureContext || typeof clipboard?.writeText !== "function") {
    return Promise.resolve(copyFallback() ? "copied" : "manual");
  }
  return clipboard.writeText(value).then(
    () => "copied" as const,
    () => "manual" as const,
  );
}
