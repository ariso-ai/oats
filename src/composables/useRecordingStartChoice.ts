import { ref, type Ref } from 'vue';

export type StartChoice = 'continue' | 'new';

/**
 * Drives a single "New vs Continue" choice dialog. `requestChoice(title)` opens
 * the dialog and resolves to the user's choice (or null if cancelled/superseded).
 * One prompt is live at a time — a new request supersedes any in-flight one
 * (resolving it null). Mirrors `useAriJoinConfirm`, but three-way.
 */
export function useRecordingStartChoice(): {
  open: Ref<boolean>;
  meetingTitle: Ref<string>;
  requestChoice(title: string): Promise<StartChoice | null>;
  choose(kind: StartChoice): void;
  cancel(): void;
} {
  const open = ref(false);
  const meetingTitle = ref('');
  let resolver: ((v: StartChoice | null) => void) | null = null;

  function settle(value: StartChoice | null): void {
    open.value = false;
    const r = resolver;
    resolver = null;
    r?.(value);
  }

  function requestChoice(title: string): Promise<StartChoice | null> {
    settle(null); // supersede any in-flight prompt
    meetingTitle.value = title;
    open.value = true;
    return new Promise<StartChoice | null>((resolve) => {
      resolver = resolve;
    });
  }

  return {
    open,
    meetingTitle,
    requestChoice,
    choose: (kind: StartChoice) => settle(kind),
    cancel: () => settle(null),
  };
}
