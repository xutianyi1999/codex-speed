export async function copyText(text: string, container: HTMLElement): Promise<boolean> {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      // Permissions can block the modern API even on HTTPS.
    }
  }

  // Remote HTTP pages cannot use Clipboard API. Keep the fallback inside the
  // dialog so its focus trap and inert background do not prevent selection.
  const input = document.createElement("textarea");
  input.value = text;
  input.readOnly = true;
  input.tabIndex = -1;
  input.style.cssText = "position:fixed;width:1px;height:1px;opacity:0;pointer-events:none";
  const focused = document.activeElement;
  container.append(input);
  try {
    input.focus({ preventScroll: true });
    input.select();
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    input.remove();
    if (focused instanceof HTMLElement) focused.focus({ preventScroll: true });
  }
}
