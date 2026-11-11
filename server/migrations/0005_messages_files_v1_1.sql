CREATE TABLE message_send_receipts (
    room_id bigint REFERENCES rooms(id),
    sender_id bigint REFERENCES users(id),
    client_id text,
    message_id bigint CHECK (message_id > 0),
    request_hash char(64) CHECK (request_hash IS NULL OR
        (request_hash ~ '^[0-9a-f]{64}$' AND message_id IS NOT NULL)),
    PRIMARY KEY (room_id, sender_id, client_id)
);

-- Earlier versions kept only the current message text and the keys of recent deletions.
INSERT INTO message_send_receipts (room_id, sender_id, client_id, message_id, request_hash)
SELECT m.room_id, m.sender_id, m.client_id, m.id,
       CASE WHEN m.edited_at IS NULL THEN encode(sha256(
           int8send(octet_length(convert_to(m.content, 'UTF8'))::bigint) || convert_to(m.content, 'UTF8') ||
           COALESCE((SELECT string_agg(int8send(a.upload_id), ''::bytea ORDER BY a.upload_id)
                     FROM message_attachments a WHERE a.message_id = m.id), ''::bytea)
       ), 'hex') END
FROM messages m;
INSERT INTO message_send_receipts (room_id, sender_id, client_id)
SELECT room_id, sender_id, client_id FROM deleted_message_keys WHERE expires_at > now()
ON CONFLICT DO NOTHING;
DROP TABLE deleted_message_keys;
ALTER TABLE message_send_receipts ADD CONSTRAINT message_receipts_require_payload
    CHECK (message_id IS NOT NULL AND request_hash IS NOT NULL
           AND char_length(client_id) BETWEEN 1 AND 64) NOT VALID;

ALTER TABLE messages ALTER COLUMN content DROP NOT NULL;
UPDATE messages SET content = NULL WHERE content = '';
ALTER TABLE messages ADD CONSTRAINT message_content_length CHECK (char_length(content) BETWEEN 1 AND 4000);

ALTER TABLE message_reactions
    ALTER COLUMN reaction_type TYPE varchar(50),
    ADD COLUMN time_reacted timestamptz;
ALTER TABLE message_reactions ALTER COLUMN time_reacted SET DEFAULT now();
ALTER TABLE message_reactions ADD CONSTRAINT message_reactions_require_time
    CHECK (time_reacted IS NOT NULL) NOT VALID;

ALTER TABLE file_uploads
    DROP CONSTRAINT file_uploads_purpose_check,
    DROP CONSTRAINT file_uploads_check,
    ADD COLUMN staging_key varchar(512) UNIQUE,
    ADD COLUMN created_at timestamptz;
UPDATE file_uploads SET
    staging_key = CASE WHEN status = 'STAGED' THEN storage_key END,
    storage_key = CASE WHEN status = 'STAGED' THEN 'uploads/final/' || storage_key ELSE storage_key END,
    created_at = expires_at - interval '1 hour',
    purpose = CASE purpose WHEN 'MESSAGE' THEN 'ATTACHMENT'
                           WHEN 'ICON' THEN 'CHANNEL_ICON'
                           WHEN 'BACKGROUND' THEN 'CHANNEL_BACKGROUND'
                           ELSE purpose END;
ALTER TABLE file_uploads
    ALTER COLUMN storage_key TYPE varchar(512),
    ALTER COLUMN file_name TYPE varchar(255),
    ALTER COLUMN mime_type TYPE varchar(100),
    ALTER COLUMN created_at SET NOT NULL,
    ALTER COLUMN created_at SET DEFAULT now(),
    ADD CONSTRAINT file_uploads_stage_required CHECK (status <> 'STAGED' OR staging_key IS NOT NULL),
    ADD CONSTRAINT file_uploads_distinct_keys CHECK (storage_key <> staging_key),
    ADD CONSTRAINT file_uploads_purpose CHECK (
        (purpose = 'ATTACHMENT' AND room_id IS NOT NULL AND channel_id IS NULL) OR
        (purpose = 'AVATAR' AND room_id IS NULL AND channel_id IS NULL AND size_bytes <= 2000000) OR
        (purpose = 'CHANNEL_ICON' AND room_id IS NULL AND channel_id IS NOT NULL AND size_bytes <= 2000000) OR
        (purpose = 'CHANNEL_BACKGROUND' AND room_id IS NULL AND channel_id IS NOT NULL AND size_bytes <= 5000000)
    );
CREATE INDEX file_uploads_attached_staging_cleanup ON file_uploads (put_expires_at)
    WHERE status = 'ATTACHED' AND staging_key IS NOT NULL;

ALTER TABLE channels
    ADD CONSTRAINT channels_icon_upload_unique UNIQUE (icon_upload_id),
    ADD CONSTRAINT channels_background_upload_unique UNIQUE (background_upload_id);
