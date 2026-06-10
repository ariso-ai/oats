# Sage Desktop Agent Instructions

Use `/Users/michaelgeiger/desktop-dev.sh` as the canonical local development
entrypoint for this Sage desktop repo. It owns the native Tauri app lifecycle,
fresh desktop app state, the shared desktop tmux console, desktop OAuth
reproduction, and the optional paired Agents platform checkout used by desktop
testing.

Choose the launcher by ownership:

- Use `~/desktop-dev.sh` for Sage/Tauri work, desktop first-run state, native
  OAuth, settings, tray/window behavior, and any task where the desktop app is
  the thing under test.
- Use `~/dev.sh` from the Agents checkout for Ari platform services, web UI,
  databases, API logs, magic links, and browser-facing platform debugging.
- Use `~/desktop-dev.sh --with-agents-platform` when a desktop task needs a
  local Ari platform; the desktop launcher will delegate platform startup to
  the isolated Agents checkout and point the native app at that stack.

Common desktop commands:

```bash
/Users/michaelgeiger/desktop-dev.sh --with-agents-platform
/Users/michaelgeiger/desktop-dev.sh restart-desktop --fresh-install
/Users/michaelgeiger/desktop-dev.sh checkout-agents
```

Use `/Users/michaelgeiger/desktop-dev.sh --fresh-install` when testing first-run
behavior, auth, settings, onboarding, or anything that can be affected by
existing app data. Use `restart-desktop --fresh-install` when the platform is
already running and only the native app should be reset/relaunched. Use
`checkout-agents` to create or refresh the isolated Agents checkout used by this
desktop worktree.

Use `~/dev.sh` only inside the Agents repo or when directly debugging the
Agents platform stack. The paired Agents checkout lives at:

```bash
/Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/agents
```

Run lints, tests, builds, and Cargo checks on the host machine, not inside the
OrbStack VM. Use the active `desktop-dev.sh` tmux session first for native
runtime debugging and logs. Use `~/.dev-sh/$WORKSPACE_ID`, VM
`dev-$WORKSPACE_ID`, and Agents `~/dev.sh` logs only when investigating the
platform/API side of a desktop flow.

During browser use, if you encounter a login screen in the Agents web UI, use
agent mail to request and open a magic link, then continue without asking for
manual login. For web testing, use the Codex in-app browser by default and open
generated links in new in-app tabs so testing remains inside the agent's
authenticated session.

When completing a desktop task, perform code verification, then include the
relevant desktop run command and any changed-page URLs from the paired
Agents/dev stack when human review needs them. Prefer Tailscale FQDNs over
localhost URLs for web review. Generate fresh, unused magic links with
`~/dev.sh --magic-link {email}` from the Agents checkout when human web review
requires authentication. Do not provide magic links that were opened during
agent testing.

When committing or pushing changes, use `--no-verify`. Include a link to any PR
that you mention in chat. Do not include incrementing numbers in stacked PR
branch names. Do not automatically open stacked PRs on GitHub unless asked.

For each new construct (function, class, type, etc.) that you create or update,
include or edit a one or two sentence inline comment that explains the intent
behind the code and how it fits within the system.
