CREATE TABLE message_send_windows (
    sender_id bigint PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    sent_at timestamptz[] NOT NULL CHECK (cardinality(sent_at) BETWEEN 1 AND 5)
);
