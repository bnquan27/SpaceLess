BEGIN;

GRANT USAGE ON SCHEMA public TO :"worker_role";
GRANT EXECUTE ON FUNCTION public.identity_key(text) TO :"worker_role";

GRANT SELECT (id, username, full_name, password_hash, auth_version, is_available),
      INSERT (id, username, full_name), UPDATE (full_name)
    ON public.users TO :"worker_role";
GRANT SELECT (user_id, email, date_of_birth, phone, address, avatar_upload_id),
      INSERT (user_id, email), UPDATE (email, date_of_birth, phone, address, avatar_upload_id)
    ON public.user_details TO :"worker_role";
GRANT SELECT (session_id, user_id, expires_at, auth_version, revoked_at),
      INSERT (session_id, user_id, expires_at, auth_version), UPDATE (expires_at, revoked_at)
    ON public.auth_sessions TO :"worker_role";
GRANT SELECT (token_jti), INSERT (token_jti, user_id, expires_at)
    ON public.revoked_tokens TO :"worker_role";
GRANT SELECT (user_id, is_used), UPDATE (is_used)
    ON public.password_reset_tokens TO :"worker_role";
GRANT SELECT (subject, user_id), INSERT (subject, user_id)
    ON public.google_identities TO :"worker_role";

GRANT SELECT, INSERT, UPDATE ON public.channels, public.channel_invitations, public.rooms,
    public.message_reads, public.snowflake_workers, public.stream_revisions TO :"worker_role";
GRANT SELECT, INSERT, UPDATE, DELETE ON public.channel_members, public.room_categories,
    public.room_members, public.room_moderator_denials, public.relations, public.voice_sessions,
    public.file_uploads TO :"worker_role";
GRANT SELECT, INSERT, DELETE ON public.user_blocks TO :"worker_role";
GRANT SELECT, INSERT ON public.dm_pairs, public.messages, public.message_send_receipts,
    public.message_attachments, public.livekit_webhook_receipts TO :"worker_role";
GRANT SELECT ON public.role_definitions, public.channel_room_access,
    public.authorized_voice_sessions, public.message_reactions TO :"worker_role";
GRANT SELECT, INSERT, UPDATE (attempts, next_attempt_at), DELETE
    ON public.media_jobs TO :"worker_role";
GRANT SELECT, INSERT (stream, revision, scope, kind, data), UPDATE (dispatched_at)
    ON public.outbox_events TO :"worker_role";
GRANT USAGE ON SEQUENCE public.media_jobs_id_seq, public.outbox_events_id_seq TO :"worker_role";

COMMIT;
