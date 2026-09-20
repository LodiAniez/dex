-- The newest message an agent was last woken to read (issue #58): an agent
-- that ignored its nudge is not woken again for the same message, and one
-- that goes idle with messages waiting is.
ALTER TABLE context_cursor ADD COLUMN woken_at INTEGER NOT NULL DEFAULT 0;
