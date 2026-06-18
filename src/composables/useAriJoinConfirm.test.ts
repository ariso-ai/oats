import { describe, it, expect } from 'vitest';
import { useAriJoinConfirm } from './useAriJoinConfirm';

describe('useAriJoinConfirm', () => {
  it('opens on requestConfirm and resolves true on confirm', async () => {
    const { open, requestConfirm, confirm } = useAriJoinConfirm();
    expect(open.value).toBe(false);
    const p = requestConfirm();
    expect(open.value).toBe(true);
    confirm();
    expect(open.value).toBe(false);
    await expect(p).resolves.toBe(true);
  });

  it('resolves false on cancel', async () => {
    const { open, requestConfirm, cancel } = useAriJoinConfirm();
    const p = requestConfirm();
    cancel();
    expect(open.value).toBe(false);
    await expect(p).resolves.toBe(false);
  });

  it('resolves a superseded request false when a new one starts', async () => {
    const { requestConfirm, confirm } = useAriJoinConfirm();
    const first = requestConfirm();
    const second = requestConfirm();
    confirm();
    await expect(first).resolves.toBe(false);
    await expect(second).resolves.toBe(true);
  });
});
