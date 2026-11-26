ALTER TABLE messages ADD COLUMN reply_context jsonb;
ALTER TABLE messages ADD COLUMN forward_context jsonb;
