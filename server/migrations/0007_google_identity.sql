ALTER TABLE users ALTER COLUMN password_hash DROP NOT NULL;
ALTER TABLE user_details
    ALTER COLUMN date_of_birth DROP NOT NULL,
    ALTER COLUMN phone DROP NOT NULL;

CREATE TABLE google_identities (
    subject varchar(255) PRIMARY KEY,
    user_id bigint NOT NULL UNIQUE REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (char_length(subject) BETWEEN 1 AND 255)
);
