# In-App Updates — Manual End-to-End Test

This procedure must pass before declaring the updater feature complete.
The plugin handles the cryptography, but our orchestration (skip,
snooze, mandatory, window opening, progress) is only proven by running
a real install against a real GitHub release.

## Prerequisites

- A real signed v0.2.0 build of Ariso installed in `/Applications/Ariso.app`.
- The Ed25519 signing keypair from Phase 1, Task 1.
- A throwaway GitHub repo or branch where you can publish a test
  release at `v0.2.1-test`.

## Test releases

For each scenario, edit `tauri.conf.json` to point at the throwaway
endpoint, run `npm run tauri:build`, and install the resulting DMG
manually. Or: stage the test artifacts in the real repo behind a
pre-release tag (e.g., `v0.2.1-test`), publish, observe, then delete
the release.

## Scenarios

### 1. Happy path

1. Install v0.2.0.
2. Publish test release v0.2.1-test (non-mandatory) with notes.
3. Launch Ariso. Within 10 seconds, the update window should appear
   showing the correct version, notes, and "You have 0.2.0" subtitle.
4. Click **Install Update**. Progress bar advances to 100%.
5. App relaunches as v0.2.1-test. Verify in Settings → About.

### 2. Skip This Version

1. From the happy-path window, click **Skip**.
2. Wait 1 minute, then trigger another automatic check by editing
   `update.last_check_unix` in `~/Library/Application Support/ai.ariso.desktop/settings.json`
   to be 25 hours in the past.
3. Within 1 hour the scheduler ticks and runs a check. Expected:
   **no dialog appears** because v0.2.1-test is skipped.
4. From the tray menu, click **Check for Updates…**. Expected:
   dialog **does appear** because the manual path bypasses skip.
5. Publish v0.2.2-test. Trigger an automatic check. Expected:
   dialog **does appear** (newer version clears the skip).

### 3. Remind Me Later

1. From the update window, click **Later**.
2. Open Settings → About. Status should be "You're up to date" (no
   re-prompt expected).
3. Edit `update.last_check_unix` to be 25 hours in the past.
4. Force-trigger an auto-check (restart the app, wait 10s).
5. Expected: no dialog because snooze hasn't expired.
6. Edit `update.snoozed_until_unix` to be in the past. Re-trigger.
7. Expected: dialog appears.

### 4. Mandatory update

1. Publish a test release with title containing `[mandatory]`.
2. Launch Ariso. Update window appears within 10 seconds.
3. Expected: **no Skip or Later links** visible.
4. Try to close the window via the red traffic-light button.
5. Expected: window does not close (stays focused).
6. Click **Install Update**. Verify install + relaunch.

### 5. Bad signature

1. Publish a test release with a deliberately corrupted `.sig` file
   (e.g., a single random base64 string).
2. Launch Ariso. Click through to install.
3. Expected: install fails. Update window shows "Couldn't download
   update. Try again?" inline. Settings → About surfaces an error.

### 6. Offline

1. Disconnect from network.
2. Launch Ariso. Open Settings → About.
3. Click **Check for Updates**.
4. Expected: status returns to idle with an inline network error.
   No dialog appears.
5. Reconnect. Click again. Expected: normal behavior.

## Cleanup

Delete the throwaway test releases. Revert `tauri.conf.json` if you
pointed it at a test endpoint.
