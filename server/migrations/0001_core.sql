CREATE TABLE snowflake_workers (
    worker_id integer PRIMARY KEY CHECK (worker_id BETWEEN 0 AND 1023),
    last_ms bigint NOT NULL,
    sequence integer NOT NULL CHECK (sequence BETWEEN 0 AND 4095)
);

CREATE TABLE users (
    id bigint PRIMARY KEY CHECK (id > 0),
    username text NOT NULL,
    email text NOT NULL,
    password_hash text NOT NULL,
    is_available boolean NOT NULL DEFAULT true,
    auth_version bigint NOT NULL DEFAULT 0
);
CREATE FUNCTION identity_key(text) RETURNS text LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
    RETURN lower($1 COLLATE "und-x-icu");
CREATE UNIQUE INDEX users_username_key ON users (identity_key(username));
CREATE UNIQUE INDEX users_email_key ON users (identity_key(email));

CREATE TABLE user_details (
    user_id bigint PRIMARY KEY REFERENCES users(id),
    full_name text NOT NULL,
    date_of_birth date NOT NULL,
    phone text NOT NULL,
    address text,
    avatar_upload_id bigint
);

CREATE TABLE auth_sessions (
    jti uuid PRIMARY KEY,
    user_id bigint NOT NULL REFERENCES users(id),
    max_expires_at timestamptz NOT NULL,
    revoked_at timestamptz
);
CREATE INDEX auth_sessions_user ON auth_sessions (user_id);

CREATE TABLE password_reset_tokens (
    token_hash bytea PRIMARY KEY,
    user_id bigint NOT NULL REFERENCES users(id),
    expires_at timestamptz NOT NULL,
    used_at timestamptz
);
CREATE INDEX reset_tokens_user ON password_reset_tokens (user_id) WHERE used_at IS NULL;

CREATE TABLE forgot_limits (
    key text NOT NULL,
    bucket bigint NOT NULL,
    count integer NOT NULL CHECK (count > 0),
    PRIMARY KEY (key, bucket)
);

CREATE TABLE relations (
    id bigint PRIMARY KEY,
    lo bigint NOT NULL REFERENCES users(id),
    hi bigint NOT NULL REFERENCES users(id),
    requester_id bigint NOT NULL REFERENCES users(id),
    status text NOT NULL CHECK (status IN ('PENDING','FRIEND')),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (lo, hi),
    CHECK (lo < hi AND requester_id IN (lo, hi))
);
CREATE INDEX relations_hi ON relations (hi);

CREATE TABLE user_blocks (
    blocker_id bigint REFERENCES users(id),
    blocked_id bigint REFERENCES users(id),
    PRIMARY KEY (blocker_id, blocked_id),
    CHECK (blocker_id <> blocked_id)
);

CREATE TABLE channels (
    id bigint PRIMARY KEY,
    creator_id bigint NOT NULL REFERENCES users(id),
    name text NOT NULL,
    description text,
    join_mode text NOT NULL DEFAULT 'INVITE_ONLY' CHECK (join_mode IN ('INVITE_ONLY','PUBLIC_LINK')),
    invite_code text NOT NULL UNIQUE,
    is_available boolean NOT NULL DEFAULT true,
    icon_upload_id bigint,
    background_upload_id bigint
);

CREATE TABLE channel_members (
    channel_id bigint REFERENCES channels(id),
    user_id bigint REFERENCES users(id),
    role text NOT NULL CHECK (role IN ('CREATOR','MODERATOR','MEMBER')),
    PRIMARY KEY (channel_id, user_id)
);
CREATE UNIQUE INDEX channel_single_creator ON channel_members (channel_id) WHERE role = 'CREATOR';
CREATE INDEX channel_members_user ON channel_members (user_id);

CREATE FUNCTION check_channel_creator() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    target bigint;
    expected bigint;
BEGIN
    IF TG_TABLE_NAME = 'channels' THEN
        target := NEW.id;
    ELSIF TG_OP = 'DELETE' THEN
        target := OLD.channel_id;
    ELSE
        target := NEW.channel_id;
    END IF;
    SELECT creator_id INTO expected FROM channels WHERE id = target;
    IF FOUND AND NOT EXISTS (
        SELECT 1 FROM channel_members WHERE channel_id = target AND user_id = expected AND role = 'CREATOR'
    ) THEN
        RAISE EXCEPTION 'Channel must have its designated creator' USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER creator_channel_check AFTER INSERT OR UPDATE ON channels
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION check_channel_creator();
CREATE CONSTRAINT TRIGGER creator_membership_check AFTER INSERT OR UPDATE OR DELETE ON channel_members
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION check_channel_creator();

CREATE TABLE channel_invitations (
    id bigint PRIMARY KEY,
    channel_id bigint NOT NULL REFERENCES channels(id),
    inviter_id bigint NOT NULL REFERENCES users(id),
    invitee_id bigint NOT NULL REFERENCES users(id),
    status text NOT NULL DEFAULT 'PENDING' CHECK (status IN ('PENDING','ACCEPTED','DECLINED','CANCELLED')),
    created_at timestamptz NOT NULL DEFAULT now(),
    resolved_at timestamptz,
    CHECK (inviter_id <> invitee_id)
);
CREATE UNIQUE INDEX pending_channel_invitation ON channel_invitations (channel_id, invitee_id) WHERE status = 'PENDING';
CREATE INDEX invitations_inviter ON channel_invitations (inviter_id);
CREATE INDEX invitations_invitee ON channel_invitations (invitee_id);

CREATE TABLE room_categories (
    id bigint PRIMARY KEY,
    channel_id bigint NOT NULL REFERENCES channels(id),
    name text NOT NULL,
    position integer NOT NULL CHECK (position >= 0),
    UNIQUE (channel_id, id)
);
CREATE UNIQUE INDEX categories_name_key ON room_categories (channel_id, identity_key(name));

CREATE TABLE rooms (
    id bigint PRIMARY KEY,
    channel_id bigint REFERENCES channels(id),
    category_id bigint,
    name text NOT NULL DEFAULT '',
    kind text NOT NULL CHECK (kind IN ('TEXT','VOICE','DM')),
    private boolean NOT NULL,
    position integer NOT NULL DEFAULT 0 CHECK (position >= 0),
    is_available boolean NOT NULL DEFAULT true,
    last_sequence bigint NOT NULL DEFAULT 0 CHECK (last_sequence >= 0),
    FOREIGN KEY (channel_id, category_id) REFERENCES room_categories(channel_id, id),
    CHECK ((kind = 'DM' AND channel_id IS NULL AND category_id IS NULL AND private) OR
           (kind IN ('TEXT','VOICE') AND channel_id IS NOT NULL))
);
CREATE UNIQUE INDEX available_room_name ON rooms (channel_id, identity_key(name)) WHERE is_available AND channel_id IS NOT NULL;
CREATE INDEX rooms_channel ON rooms (channel_id);

CREATE TABLE room_members (
    room_id bigint REFERENCES rooms(id),
    user_id bigint REFERENCES users(id),
    is_room_mod boolean NOT NULL DEFAULT false,
    PRIMARY KEY (room_id, user_id)
);
CREATE INDEX room_members_user ON room_members (user_id);

CREATE TABLE dm_pairs (
    lo bigint REFERENCES users(id),
    hi bigint REFERENCES users(id),
    room_id bigint NOT NULL UNIQUE REFERENCES rooms(id),
    PRIMARY KEY (lo, hi),
    CHECK (lo < hi)
);
CREATE INDEX dm_pairs_hi ON dm_pairs (hi);

CREATE TABLE messages (
    id bigint PRIMARY KEY,
    room_id bigint NOT NULL REFERENCES rooms(id),
    sender_id bigint NOT NULL REFERENCES users(id),
    client_id text NOT NULL,
    sequence bigint NOT NULL CHECK (sequence > 0),
    content text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    edited_at timestamptz,
    UNIQUE (room_id, sequence),
    UNIQUE (room_id, sender_id, client_id)
);

CREATE TABLE deleted_message_keys (
    room_id bigint REFERENCES rooms(id),
    sender_id bigint REFERENCES users(id),
    client_id text NOT NULL,
    expires_at timestamptz NOT NULL,
    PRIMARY KEY (room_id, sender_id, client_id)
);
CREATE INDEX deleted_message_keys_expiry ON deleted_message_keys (expires_at);

CREATE TABLE message_reactions (
    message_id bigint REFERENCES messages(id) ON DELETE CASCADE,
    user_id bigint REFERENCES users(id),
    reaction_type text NOT NULL,
    PRIMARY KEY (message_id, user_id)
);

CREATE TABLE message_reads (
    room_id bigint REFERENCES rooms(id),
    user_id bigint REFERENCES users(id),
    last_sequence bigint NOT NULL DEFAULT 0 CHECK (last_sequence >= 0),
    read_at timestamptz,
    PRIMARY KEY (room_id, user_id)
);

CREATE TABLE stream_revisions (
    stream text PRIMARY KEY,
    revision bigint NOT NULL CHECK (revision > 0)
);

CREATE TABLE outbox_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    stream text NOT NULL,
    revision bigint NOT NULL,
    kind text NOT NULL,
    scope jsonb NOT NULL,
    data jsonb NOT NULL,
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    dispatched_at timestamptz,
    UNIQUE (stream, revision)
);
CREATE INDEX pending_outbox_events ON outbox_events (id) WHERE dispatched_at IS NULL;
