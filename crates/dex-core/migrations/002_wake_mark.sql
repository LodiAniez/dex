-- What an agent was last woken to read (issue #58): the newest message it was
-- woken for, and the status time it had then. An agent that ignored its nudge
-- is not woken again for the same message, nor given a second nudge before it
-- has had a turn of its own.
ALTER TABLE context_cursor ADD COLUMN woken_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE context_cursor ADD COLUMN woken_idle_at INTEGER NOT NULL DEFAULT 0;
