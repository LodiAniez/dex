---
name: dex-agentic
description: Working alongside other agents in a Dex workspace - when to start another agent on a task, how to brief one, and how to share findings so two agents never solve the same problem twice. Use when a task looks splittable, when other agents are already working in this workspace, or when deciding whether to spawn.
---

# Working with other agents in Dex

You are running in a Dex pane, which means you may not be the only agent in this
workspace. A short digest of what the others have done arrives at the start of
your turns; you do not have to ask for it.

This skill is about judgment: when another agent helps, and when it costs more
than it saves. The mechanics are one command and eight tools, listed at the end.

## Before you spawn anything, look

Call `agents_list`. If someone is already on the work, you are about to duplicate
it. Call `context_search` for the topic — another agent may have settled it hours
ago and written it down.

## When spawning is worth it

All three have to be true:

- **Independent.** The work does not depend on anything you are about to learn.
- **Big enough.** Roughly fifteen minutes of real work or more.
- **Checkable without you.** You can say what "done" looks like, now, in writing.

Good candidates:

- "Port the remaining 40 call sites to the new client API" while you design the
  next piece of it.
- "Write integration tests for the six endpoints that already exist."
- "Update the docs to match the schema change in `db/schema.sql`."

## When it is not

**The usual mistake is spawning too much.** A child starts cold: it has not read
this conversation, has not seen the files you have open, does not know what you
already tried and ruled out. You pay that cost every single time.

Do it yourself when the work is:

- **Sequential** - the second half needs the first half's answer.
- **Exploratory debugging** - you do not yet know what you are looking for. If
  you cannot write the brief, the task is not ready to delegate.
- **Small** - under about fifteen minutes.
- **Dependent on your reasoning in flight** - the half-formed plan you have not
  written down yet is not something the child can read.

The test that settles most cases: **if writing the brief would take longer than
doing the work, do the work.**

## Writing the brief

The child cannot see this conversation. The brief plus the shared context is
everything it will ever know about why it exists. Four parts:

1. **What to do.**
2. **What done looks like** - something it can check, like "`cargo test` passes"
   or "every call site uses the new signature".
3. **Which files or directories** it should be working in.
4. **What not to touch.**

The fourth is not politeness. **A spawned agent runs without stopping for
approval** - it will not ask you before editing something. Anything you do not
want changed has to be named in the brief, because the brief is the only place
the boundary exists.

Too thin: *"fix the auth tests"*

Enough: *"The tests in `tests/auth/` fail since the JWT change. Make them pass by
updating the tests to the new token shape - the implementation in `src/auth/` is
correct and must not change. Done when `cargo test -p auth` is green. Do not
touch anything outside `tests/auth/`."*

## Give it a worktree

A child with a worktree gets its own checkout on its own branch, so its edits
cannot collide with yours and nothing it does reaches your files until you merge.
Give one to any child that writes code. Without it the child works in the same
checkout you are in, and two agents editing one working tree will lose work.

## The limits

- **Depth 2.** You may spawn; your children may not. A child that needs help
  should report back instead.
- **Six live agents per workspace.**

Both are refused immediately, with the reason. A refusal is not something to
retry or work around - it means do the work yourself.

## Share as you go

- `note_append` **while you work, not at the end.** A note written when you
  finish is a note nobody could act on. One line each: a decision, a discovery, a
  rename, a broken assumption.
- `context_write` for facts that stay true, under a stable key. If another agent
  might be editing the same key, `context_read` it first and pass the version
  back, so a conflict fails loudly instead of overwriting them.
- `message_send` when it matters to one agent and nobody else. This is also
  how you change a child's task after it has started: send it the new
  requirement. A child that is busy sees it at its next tool call; one that
  has finished and is sitting idle is woken to read it. You do not need to
  stop it and start another.
- `message_inbox` the moment your digest says messages are waiting - the digest
  tells you the count, never the contents.

Do this whether or not you spawned anything. It is what makes the next agent's
digest worth reading.

## Collecting results

`agents_list` shows what each agent is doing and whether it is running, idle, or
waiting on a human. A child's notes reach you in your digest on their own.

When a child finishes, its work is on its branch, not yours - review that branch
before merging. A child that goes idle without notes usually got stuck: read its
pane rather than assuming it finished.

## The mechanics

Spawning is a command, because it creates a pane:

```
dex agent spawn --task "<the brief>" --repo <name> --worktree <branch> --label <short-name>
```

The brief can be as long as it needs to be; it reaches the child through the
shared context, not through a command line.

Everything else is an MCP tool: `note_append`, `context_read`, `context_write`,
`context_search`, `context_list`, `agents_list`, `message_send`,
`message_inbox`. They are only available inside a Dex pane.
