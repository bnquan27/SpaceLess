SELECT EXISTS (SELECT 1 FROM auth_sessions s JOIN users u ON u.id = s.user_id
         WHERE s.session_id = $1 AND u.id = $2 AND u.auth_version = $3 AND s.auth_version = $3
           AND u.is_available AND s.revoked_at IS NULL AND s.expires_at >= $4 AND $4 > clock_timestamp()
           AND NOT EXISTS (SELECT 1 FROM revoked_tokens WHERE token_jti = $5))