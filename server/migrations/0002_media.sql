CREATE TABLE file_uploads (
    id BIGINT PRIMARY KEY,
    owner_id BIGINT NOT NULL REFERENCES users(id),
    room_id BIGINT REFERENCES rooms(id),
    channel_id BIGINT REFERENCES channels(id),
    purpose TEXT NOT NULL CHECK (purpose IN ('MESSAGE','AVATAR','ICON','BACKGROUND')),
    storage_key TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'STAGED' CHECK (status IN ('STAGED','ATTACHED','DELETE_PENDING')),
    file_name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL CHECK (size_bytes BETWEEN 1 AND 25000000),
    expires_at TIMESTAMPTZ NOT NULL,
    put_expires_at TIMESTAMPTZ NOT NULL,
    object_version TEXT,
    etag TEXT,
    CHECK ((purpose = 'MESSAGE' AND room_id IS NOT NULL AND channel_id IS NULL)
        OR (purpose = 'AVATAR' AND room_id IS NULL AND channel_id IS NULL)
        OR (purpose IN ('ICON','BACKGROUND') AND room_id IS NULL AND channel_id IS NOT NULL))
);
CREATE INDEX file_uploads_cleanup ON file_uploads(status, expires_at);
CREATE TABLE message_attachments (
    id BIGINT PRIMARY KEY,
    message_id BIGINT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    upload_id BIGINT NOT NULL UNIQUE REFERENCES file_uploads(id)
);
ALTER TABLE channels ADD CONSTRAINT channel_icon_upload FOREIGN KEY (icon_upload_id) REFERENCES file_uploads(id);
ALTER TABLE channels ADD CONSTRAINT channel_background_upload FOREIGN KEY (background_upload_id) REFERENCES file_uploads(id);
ALTER TABLE user_details ADD CONSTRAINT user_avatar_upload FOREIGN KEY (avatar_upload_id) REFERENCES file_uploads(id);

CREATE TABLE voice_sessions (
    id BIGINT PRIMARY KEY,
    user_id BIGINT NOT NULL UNIQUE REFERENCES users(id),
    room_id BIGINT NOT NULL REFERENCES rooms(id),
    session_jti UUID NOT NULL,
    identity TEXT NOT NULL UNIQUE,
    participant_sid TEXT,
    state TEXT NOT NULL CHECK (state IN ('PENDING','CONNECTED','REVOKING')),
    screen_video_sid TEXT,
    screen_audio_sid TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE livekit_webhook_receipts (
    event_id TEXT PRIMARY KEY,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE media_jobs (
    id BIGSERIAL PRIMARY KEY,
    voice_session_id BIGINT NOT NULL UNIQUE,
    room_id BIGINT NOT NULL,
    identity TEXT NOT NULL,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
