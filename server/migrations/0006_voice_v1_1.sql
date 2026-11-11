ALTER TABLE auth_sessions ADD UNIQUE (session_id, user_id);
ALTER TABLE rooms ADD UNIQUE (id, kind);

ALTER TABLE voice_sessions RENAME COLUMN session_jti TO session_id;
ALTER TABLE voice_sessions RENAME COLUMN identity TO participant_identity;
ALTER TABLE voice_sessions RENAME COLUMN created_at TO time_issued;
ALTER TABLE voice_sessions RENAME COLUMN expires_at TO join_token_expires_at;

ALTER TABLE voice_sessions DROP CONSTRAINT voice_sessions_state_check;
UPDATE voice_sessions SET state = CASE state
    WHEN 'PENDING' THEN 'ISSUED'
    WHEN 'REVOKING' THEN 'REVOKE_PENDING'
    ELSE state
END;

ALTER TABLE voice_sessions
    ADD COLUMN room_kind text GENERATED ALWAYS AS ('VOICE'::text) STORED,
    ADD COLUMN service_room varchar(255) GENERATED ALWAYS AS ('room-' || room_id::text) STORED,
    ADD COLUMN is_screen_sharing boolean GENERATED ALWAYS AS (screen_video_sid IS NOT NULL) STORED,
    ADD CONSTRAINT voice_session_owner FOREIGN KEY (session_id, user_id)
        REFERENCES auth_sessions (session_id, user_id),
    ADD CONSTRAINT voice_session_room_kind FOREIGN KEY (room_id, room_kind)
        REFERENCES rooms (id, kind),
    ADD CONSTRAINT voice_session_state CHECK (state IN ('ISSUED', 'CONNECTED', 'REVOKE_PENDING')),
    ADD UNIQUE (service_room, participant_identity);

CREATE VIEW authorized_voice_sessions AS
SELECT v.id
FROM voice_sessions v
JOIN auth_sessions a ON a.session_id = v.session_id AND a.user_id = v.user_id
JOIN users u ON u.id = a.user_id AND u.auth_version = a.auth_version
JOIN channel_room_access access ON access.room_id = v.room_id AND access.user_id = v.user_id
WHERE a.revoked_at IS NULL AND a.expires_at > now();
