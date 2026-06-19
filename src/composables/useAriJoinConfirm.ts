import { ref, type Ref } from 'vue';

/**
 * Drives a single Ari-will-join confirmation dialog. `requestConfirm()` opens
 * the dialog and resolves to the user's choice; `confirm()`/`cancel()` are
 * wired to the dialog's buttons. One prompt is live at a time — a new request
 * supersedes any in-flight one (resolving it `false`).
 */
export function useAriJoinConfirm(): {
  open: Ref<boolean>;
  requestConfirm(): Promise<boolean>;
  confirm(): void;
  cancel(): void;
} {
  const open = ref(false);
  let resolver: ((v: boolean) => void) | null = null;

  function settle(value: boolean): void {
    open.value = false;
    const r = resolver;
    resolver = null;
    r?.(value);
  }

  function requestConfirm(): Promise<boolean> {
    settle(false); // supersede any in-flight prompt
    open.value = true;
    return new Promise<boolean>((resolve) => {
      resolver = resolve;
    });
  }

  return {
    open,
    requestConfirm,
    confirm: () => settle(true),
    cancel: () => settle(false),
  };
}
