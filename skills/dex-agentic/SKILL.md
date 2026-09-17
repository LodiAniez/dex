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

## When you need the owner

The owner watches the workspace from views that show who is waiting on them. A
decision that is theirs to make - overwrite this file, which of two designs,
whether a requirement relayed by another agent really came from them - is best
asked with the **AskUserQuestion** tool. Dex hears that as it happens: you are
shown as needing them, with the question beside you, until they answer.

A question that only ends your reply is a finished turn as far as anyone can
tell. Dex tries to notice ("Reply yes and I'll...", "Should I...?"), but it is
reading prose, and a missed one means you sit unanswered while the owner thinks
you are done. If you do end a turn on a question, make the question its last
sentence.

Do not ask the owner what another agent can answer - `message_send` them - and
say in a brief what a child should come back to you about rather than the owner.

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

So is ending one, because it removes a pane:

```
dex agent stop <label-or-id>
```

That ends the agent and closes the pane Dex made for it. Do it when you are
asked to stop, kill or dismiss agents you spawned, or when one is finished and
its seat is wanted - not to change its task, which is what `message_send` is
for. Its notes and its branch stay. It only closes panes Dex created: an agent
the owner started in a pane of their own is ended and the pane left alone.
You can stop yourself and the agents you spawned, and nobody else's: Dex
refuses the rest. If another agent's child should end, tell that agent.

Everything else is an MCP tool: `note_append`, `context_read`, `context_write`,
`context_search`, `context_list`, `agents_list`, `message_send`,
`message_inbox`. They are only available inside a Dex pane.
