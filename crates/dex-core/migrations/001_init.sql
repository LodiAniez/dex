-- Initial schema. Source of truth: docs/prd.md §5.
-- Never edit a migration that has shipped; add a new numbered file instead.

CREATE TABLE workspace (
  id            TEXT PRIMARY KEY,          -- uuid v4
  name          TEXT NOT NULL,
  root_path     TEXT NOT NULL,             -- absolute; hosts .dex/. Need not be a repo.
  color         TEXT,                      -- hex, nullable
  sort_index    INTEGER NOT NULL,
  layout_json   TEXT NOT NULL,             -- serialized pane tree
  active_pane   TEXT,                      -- pane id
  created_at    INTEGER NOT NULL,          -- unix millis
  updated_at    INTEGER NOT NULL
);

CREATE TABLE repo (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,             -- display name, defaults to dir name
  path          TEXT NOT NULL UNIQUE,      -- absolute, normalized to forward slashes
  created_at    INTEGER NOT NULL
);

CREATE TABLE workspace_repo (
  workspace_id  TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
  repo_id       TEXT NOT NULL REFERENCES repo(id) ON DELETE CASCADE,
  worktree_path TEXT,                      -- null = uses the main checkout
  branch        TEXT,
  PRIMARY KEY (workspace_id, repo_id)
);

CREATE TABLE pane (
  id            TEXT PRIMARY KEY,
  workspace_id  TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
  label         TEXT,                      -- user or agent assigned, unique within workspace
  cwd           TEXT NOT NULL,
  kind          TEXT NOT NULL,             -- 'terminal' | 'markdown' | 'diff' | 'activity'
  runtime       TEXT NOT NULL DEFAULT 'windows', -- 'windows' | 'wsl:<distro>'
  created_at    INTEGER NOT NULL
);
CREATE UNIQUE INDEX pane_label_unique ON pane(workspace_id, label) WHERE label IS NOT NULL;

-- Agent rows are history: they outlive their panes and are never deleted,
-- except when their whole workspace is deleted.
CREATE TABLE agent (
  id              TEXT PRIMARY KEY,        -- uuid, set as DEX_AGENT_ID when spawned
  pane_id         TEXT REFERENCES pane(id) ON DELETE SET NULL,
  workspace_id    TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
  parent_id       TEXT REFERENCES agent(id) ON DELETE SET NULL, -- null for human-started agents
  label           TEXT,                    -- defaults to the pane label
  backend         TEXT NOT NULL,           -- 'claude'
  session_id      TEXT,                    -- backend's own session id, from hook input
  status          TEXT NOT NULL,           -- idle|running|waiting|error|unknown|dead
  status_detail   TEXT,                    -- e.g. StopFailure type: 'rate_limit'
  status_at       INTEGER NOT NULL,        -- hook-stamped time of the transition applied
  permission_mode TEXT,                    -- from hook input
  task_brief      TEXT,
  depth           INTEGER NOT NULL DEFAULT 0,
  started_at      INTEGER NOT NULL,
  last_event_at   INTEGER NOT NULL,
  ended_at        INTEGER
);
CREATE UNIQUE INDEX agent_pane_session ON agent(pane_id, session_id)
  WHERE pane_id IS NOT NULL AND session_id IS NOT NULL;

-- Cross-agent context. Key-value with optimistic concurrency.
-- Explicit integer id: FTS5 external content needs a rowid that VACUUM cannot renumber.
CREATE TABLE context_entry (
  id            INTEGER PRIMARY KEY,
  workspace_id  TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
  key           TEXT NOT NULL,
  value         TEXT NOT NULL,
  version       INTEGER NOT NULL DEFAULT 1,
  author_agent  TEXT,                      -- null if written by the human via CLI
  tags          TEXT,                      -- comma-separated, for filtering
  updated_at    INTEGER NOT NULL,
  UNIQUE (workspace_id, key)
);

CREATE VIRTUAL TABLE context_entry_fts USING fts5(
  key, value, tags, content='context_entry', content_rowid='id'
);
CREATE TRIGGER context_entry_ai AFTER INSERT ON context_entry BEGIN
  INSERT INTO context_entry_fts(rowid, key, value, tags)
  VALUES (new.id, new.key, new.value, new.tags);
END;
CREATE TRIGGER context_entry_ad AFTER DELETE ON context_entry BEGIN
  INSERT INTO context_entry_fts(context_entry_fts, rowid, key, value, tags)
  VALUES ('delete', old.id, old.key, old.value, old.tags);
END;
CREATE TRIGGER context_entry_au AFTER UPDATE ON context_entry BEGIN
  INSERT INTO context_entry_fts(context_entry_fts, rowid, key, value, tags)
  VALUES ('delete', old.id, old.key, old.value, old.tags);
  INSERT INTO context_entry_fts(rowid, key, value, tags)
  VALUES (new.id, new.key, new.value, new.tags);
END;

-- Append-only log. Drives digests, the activity pane, and debugging.
CREATE TABLE context_event (
  seq           INTEGER PRIMARY KEY AUTOINCREMENT,
  workspace_id  TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
  agent_id      TEXT,
  kind          TEXT NOT NULL,             -- 'note' | 'write' | 'delete' | 'message' | 'spawn' | 'status'
  key           TEXT,
  body          TEXT NOT NULL,
  target_agent  TEXT,                      -- set for 'message'
  read_at       INTEGER,                   -- set for 'message' when delivered by message_inbox
  created_at    INTEGER NOT NULL
);
CREATE INDEX context_event_ws_seq ON context_event(workspace_id, seq);
CREATE INDEX context_event_inbox ON context_event(target_agent, read_at) WHERE kind = 'message';

-- Per-agent read cursor into context_event, for delta digests.
CREATE TABLE context_cursor (
  agent_id      TEXT PRIMARY KEY REFERENCES agent(id) ON DELETE CASCADE,
  last_seq      INTEGER NOT NULL DEFAULT 0,
  last_delta_at INTEGER NOT NULL DEFAULT 0 -- for delta rate limiting
);

CREATE TABLE app_state (
  key           TEXT PRIMARY KEY,
  value         TEXT NOT NULL
);
