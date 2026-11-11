ALTER TABLE users ADD COLUMN full_name varchar(100);
ALTER TABLE user_details ADD COLUMN email varchar(100);
UPDATE users u SET full_name = d.full_name FROM user_details d WHERE d.user_id = u.id;
UPDATE user_details d SET email = u.email FROM users u WHERE u.id = d.user_id;
ALTER TABLE users ALTER COLUMN full_name SET NOT NULL;
ALTER TABLE user_details ALTER COLUMN email SET NOT NULL;
ALTER TABLE user_details ADD CONSTRAINT user_details_avatar_key UNIQUE (avatar_upload_id);
ALTER TABLE users ADD COLUMN time_created timestamptz;
UPDATE users SET time_created = to_timestamp(1704067200 + (id >> 22) / 1000.0);
ALTER TABLE users ALTER COLUMN time_created SET DEFAULT now(), ALTER COLUMN time_created SET NOT NULL;
ALTER TABLE users DROP COLUMN email;
ALTER TABLE user_details DROP COLUMN full_name;
CREATE UNIQUE INDEX user_details_email_key ON user_details (identity_key(email));

CREATE FUNCTION invalidate_auth_on_account_lock() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.auth_version := greatest(NEW.auth_version, OLD.auth_version + 1);
    RETURN NEW;
END;
$$;
CREATE TRIGGER invalidate_auth_on_account_lock BEFORE UPDATE OF is_available ON users
    FOR EACH ROW WHEN (OLD.is_available AND NOT NEW.is_available)
    EXECUTE FUNCTION invalidate_auth_on_account_lock();

ALTER TABLE auth_sessions RENAME COLUMN jti TO session_id;
ALTER TABLE auth_sessions RENAME COLUMN max_expires_at TO expires_at;
ALTER TABLE auth_sessions ADD COLUMN auth_version bigint;
UPDATE auth_sessions s SET auth_version = u.auth_version FROM users u WHERE u.id = s.user_id;
ALTER TABLE auth_sessions ALTER COLUMN auth_version SET NOT NULL;

CREATE TABLE revoked_tokens (
    token_jti uuid PRIMARY KEY,
    user_id bigint NOT NULL REFERENCES users(id),
    expires_at timestamptz NOT NULL
);
CREATE INDEX revoked_tokens_expiry ON revoked_tokens (expires_at);

ALTER TABLE password_reset_tokens ADD COLUMN token_id integer GENERATED ALWAYS AS IDENTITY;
ALTER TABLE password_reset_tokens ADD COLUMN time_created timestamptz;
ALTER TABLE password_reset_tokens ADD COLUMN is_used boolean NOT NULL DEFAULT false;
UPDATE password_reset_tokens SET time_created = expires_at - interval '15 minutes',
    is_used = used_at IS NOT NULL;
ALTER TABLE password_reset_tokens
    ALTER COLUMN time_created SET DEFAULT now(),
    ALTER COLUMN time_created SET NOT NULL,
    DROP COLUMN used_at,
    DROP CONSTRAINT password_reset_tokens_pkey,
    ADD CONSTRAINT password_reset_tokens_pkey PRIMARY KEY (token_id),
    ADD CONSTRAINT password_reset_tokens_hash_key UNIQUE (token_hash);
CREATE INDEX reset_tokens_user ON password_reset_tokens (user_id) WHERE NOT is_used;

DROP TABLE forgot_limits;
CREATE TABLE recovery_rate_buckets (
    key_kind text NOT NULL CHECK (key_kind IN ('EMAIL', 'IP')),
    key_hash char(64) NOT NULL,
    window_start timestamptz NOT NULL,
    admitted_count integer NOT NULL CHECK (admitted_count BETWEEN 1 AND
        CASE key_kind WHEN 'EMAIL' THEN 3 ELSE 20 END),
    PRIMARY KEY (key_kind, key_hash, window_start)
);
CREATE INDEX recovery_rate_buckets_window ON recovery_rate_buckets (window_start);
