-- Keep the last five accepted sends, oldest first. The upsert serializes
-- concurrent sends from one account, including sends to different rooms.
INSERT INTO message_send_windows AS w (sender_id, sent_at)
VALUES ($1, ARRAY[clock_timestamp()])
ON CONFLICT (sender_id) DO UPDATE
SET sent_at = array_append(
    CASE WHEN cardinality(w.sent_at) = 5 THEN w.sent_at[2:5] ELSE w.sent_at END,
    clock_timestamp()
)
WHERE cardinality(w.sent_at) < 5 OR w.sent_at[1] <= clock_timestamp() - interval '2 seconds'
RETURNING sender_id
