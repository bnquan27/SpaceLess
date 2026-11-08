use sqlx::{FromRow, PgConnection};

use crate::{Actor, ApiError, Id, Result};

pub async fn lock_session(conn: &mut PgConnection, actor: &Actor) -> Result<()> {
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT is_available AND auth_version=$2 FROM users WHERE id=$1 FOR SHARE",
    )
    .bind(actor.user_id.0)
    .bind(actor.auth_version)
    .fetch_optional(&mut *conn)
    .await?;
    if active != Some(true) {
        return Err(ApiError::unauthorized());
    }
    let valid = sqlx::query_scalar::<_, bool>(
        "SELECT revoked_at IS NULL AND max_expires_at > clock_timestamp() AND to_timestamp($3) > clock_timestamp()
         FROM auth_sessions WHERE jti=$1 AND user_id=$2 FOR SHARE",
    ).bind(actor.jti).bind(actor.user_id.0).bind(actor.exp as f64).fetch_optional(conn).await?;
    if valid != Some(true) {
        return Err(ApiError::unauthorized());
    }
    Ok(())
}

pub async fn lock_pair(conn: &mut PgConnection, first: Id, second: Id) -> Result<()> {
    let (lo, hi) = (first.min(second), first.max(second));
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("social:{lo}:{hi}"))
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn lock_channel(conn: &mut PgConnection, channel: Id) -> Result<()> {
    sqlx::query_scalar::<_, i64>("SELECT id FROM channels WHERE id=$1 AND is_available FOR UPDATE")
        .bind(channel.0)
        .fetch_one(conn)
        .await?;
    Ok(())
}

pub async fn lock_room(conn: &mut PgConnection, room: Id) -> Result<()> {
    sqlx::query_scalar::<_, i64>("SELECT id FROM rooms WHERE id=$1 AND is_available FOR UPDATE")
        .bind(room.0)
        .fetch_one(conn)
        .await?;
    Ok(())
}

#[derive(FromRow)]
pub struct RoomAccess {
    pub kind: String,
    pub channel_id: Option<Id>,
    pub writable: bool,
    pub moderate: bool,
    pub last_sequence: i64,
}

pub async fn room_access(conn: &mut PgConnection, user: Id, room: Id) -> Result<RoomAccess> {
    let result = sqlx::query_as::<_, RoomAccess>(
        "SELECT r.kind, r.channel_id, r.last_sequence,
            CASE WHEN r.kind='DM' THEN EXISTS(SELECT 1 FROM relations x WHERE x.lo=p.lo AND x.hi=p.hi AND x.status='FRIEND')
              AND NOT EXISTS(SELECT 1 FROM user_blocks b WHERE (b.blocker_id,b.blocked_id) IN ((p.lo,p.hi),(p.hi,p.lo)))
              ELSE true END AS writable,
            COALESCE(cm.role IN ('CREATOR','MODERATOR') OR (r.private AND rm.is_room_mod),false) AS moderate
         FROM rooms r LEFT JOIN channels c ON c.id=r.channel_id
         LEFT JOIN channel_members cm ON cm.channel_id=c.id AND cm.user_id=$1
         LEFT JOIN room_members rm ON rm.room_id=r.id AND rm.user_id=$1
         LEFT JOIN dm_pairs p ON p.room_id=r.id
         WHERE r.id=$2 AND r.is_available AND
           ((r.kind='DM' AND $1 IN (p.lo,p.hi)) OR
            (c.is_available AND cm.user_id IS NOT NULL AND (NOT r.private OR cm.role IN ('CREATOR','MODERATOR') OR rm.user_id IS NOT NULL)))",
    ).bind(user.0).bind(room.0).fetch_optional(conn).await?;
    result.ok_or_else(ApiError::not_found)
}

pub async fn room_viewers(conn: &mut PgConnection, room: Id) -> Result<Vec<Id>> {
    let ids = sqlx::query_scalar::<_, Id>(
        "SELECT cm.user_id FROM rooms r JOIN channels c ON c.id=r.channel_id AND c.is_available
         JOIN channel_members cm ON cm.channel_id=c.id JOIN users u ON u.id=cm.user_id AND u.is_available
         LEFT JOIN room_members rm ON rm.room_id=r.id AND rm.user_id=cm.user_id
         WHERE r.id=$1 AND r.is_available AND (NOT r.private OR cm.role IN ('CREATOR','MODERATOR') OR rm.user_id IS NOT NULL)
         UNION ALL SELECT u.id FROM rooms r JOIN dm_pairs p ON p.room_id=r.id
         JOIN users u ON u.id IN (p.lo,p.hi) AND u.is_available WHERE r.id=$1 AND r.is_available ORDER BY 1",
    ).bind(room.0).fetch_all(conn).await?;
    Ok(ids)
}

pub async fn channel_visible(conn: &mut PgConnection, user: Id, channel: Id) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM channels c JOIN channel_members cm ON cm.channel_id=c.id
         WHERE c.id=$1 AND c.is_available AND cm.user_id=$2)",
    )
    .bind(channel.0)
    .bind(user.0)
    .fetch_one(conn)
    .await?)
}

pub async fn lock_room_write(
    conn: &mut PgConnection,
    actor: &Actor,
    room: Id,
) -> Result<RoomAccess> {
    lock_session(conn, actor).await?;
    let access = room_access(conn, actor.user_id, room).await?;
    if let Some(channel) = access.channel_id {
        sqlx::query_scalar::<_, i64>(
            "SELECT id FROM channels WHERE id=$1 AND is_available FOR SHARE",
        )
        .bind(channel.0)
        .fetch_one(&mut *conn)
        .await?;
    } else {
        let (lo, hi) = sqlx::query_as::<_, (Id, Id)>("SELECT lo,hi FROM dm_pairs WHERE room_id=$1")
            .bind(room.0)
            .fetch_one(&mut *conn)
            .await?;
        lock_pair(conn, lo, hi).await?;
    }
    lock_room(conn, room).await?;
    room_access(conn, actor.user_id, room).await
}
