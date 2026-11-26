use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_postgres::{GenericClient, types::Type};
use uuid::Uuid;

use crate::{
    auth::{Actor, ApiError},
    messages,
};

type Result<T> = std::result::Result<T, ApiError>;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct Id(pub(crate) i64);

impl From<Id> for String {
    fn from(id: Id) -> Self {
        id.0.to_string()
    }
}

impl TryFrom<String> for Id {
    type Error = &'static str;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        let id = value.parse::<i64>().map_err(|_| "Invalid Snowflake ID")?;
        if id <= 0 || id.to_string() != value {
            return Err("Invalid Snowflake ID");
        }
        Ok(Self(id))
    }
}

pub(crate) enum Scope {
    User(i64),
    Users(Vec<i64>),
    Channel(i64),
    Room(i64),
    RoomUsers(i64, Vec<i64>),
    Revocation(Vec<i64>),
    Session(Uuid),
}

#[derive(Deserialize, Serialize)]
pub(crate) enum EventScope {
    Users(Vec<Id>),
    Channel(Id),
    Room(Id),
    RoomUsers(Id, Vec<Id>),
    Revocation(Vec<Id>),
    Session(Uuid),
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Event {
    pub(crate) id: Id,
    pub(crate) stream: String,
    pub(crate) revision: Id,
    pub(crate) kind: String,
    pub(crate) scope: EventScope,
    pub(crate) data: Value,
    pub(crate) occurred_at: DateTime<Utc>,
}

pub(crate) async fn emit<C: GenericClient + Sync>(
    client: &C,
    scope: Scope,
    kind: &str,
    data: Value,
) -> Result<()> {
    let data = json!({"cloudflare_staging": true, "payload": data});
    let revocation = matches!(scope, Scope::Revocation(_));
    let (stream, scope) = match scope {
        Scope::User(user) => (format!("user:{user}"), EventScope::Users(vec![Id(user)])),
        Scope::Users(mut users) | Scope::Revocation(mut users) => {
            users.sort_unstable();
            users.dedup();
            for user in users {
                let scope = if revocation {
                    EventScope::Revocation(vec![Id(user)])
                } else {
                    EventScope::Users(vec![Id(user)])
                };
                insert(client, &format!("user:{user}"), scope, kind, &data).await?;
            }
            return Ok(());
        }
        Scope::Channel(channel) => (
            format!("channel:{channel}"),
            EventScope::Channel(Id(channel)),
        ),
        Scope::Room(room) => (format!("room:{room}"), EventScope::Room(Id(room))),
        Scope::RoomUsers(room, users) => (
            format!("room:{room}"),
            EventScope::RoomUsers(Id(room), users.into_iter().map(Id).collect()),
        ),
        Scope::Session(sid) => {
            let id = data["payload"]["voice_session_id"]
                .as_str()
                .ok_or_else(ApiError::internal)?;
            (format!("voice-session:{id}"), EventScope::Session(sid))
        }
    };
    insert(client, &stream, scope, kind, &data).await
}

async fn insert<C: GenericClient + Sync>(
    client: &C,
    stream: &str,
    scope: EventScope,
    kind: &str,
    data: &Value,
) -> Result<()> {
    let scope = serde_json::to_value(scope).map_err(|_| ApiError::internal())?;
    client.query_typed(
        "WITH next AS (
            INSERT INTO stream_revisions(stream,revision) VALUES($1,1)
            ON CONFLICT(stream) DO UPDATE SET revision=stream_revisions.revision+1 RETURNING revision
        ) INSERT INTO outbox_events(stream,revision,scope,kind,data) SELECT $1,revision,$2,$3,$4 FROM next",
        &[(&stream, Type::TEXT), (&scope, Type::JSONB), (&kind, Type::TEXT), (&data, Type::JSONB)],
    ).await?;
    Ok(())
}

impl Event {
    pub(crate) async fn recipients<C: GenericClient + Sync>(&self, client: &C) -> Result<Vec<i64>> {
        let mut users = match &self.scope {
            EventScope::Users(users) | EventScope::Revocation(users) | EventScope::RoomUsers(_, users) => users.iter().map(|id| id.0).collect(),
            EventScope::Room(room) => messages::room_viewers(client, room.0).await?,
            EventScope::Channel(channel) => client.query_typed(
                "SELECT cm.user_id FROM channel_members cm JOIN users u ON u.id=cm.user_id AND u.is_available WHERE cm.channel_id=$1 ORDER BY cm.user_id",
                &[(&channel.0, Type::INT8)],
            ).await?.iter().map(|row| row.get(0)).collect(),
            EventScope::Session(sid) => client.query_typed(
                "SELECT user_id FROM auth_sessions WHERE session_id=$1", &[(sid, Type::UUID)],
            ).await?.iter().map(|row| row.get(0)).collect(),
        };
        users.sort_unstable();
        users.dedup();
        Ok(users)
    }

    pub(crate) async fn delivery<C: GenericClient + Sync>(
        &self,
        client: &C,
        actor: &Actor,
    ) -> Result<Option<Value>> {
        let permitted = match &self.scope {
            EventScope::Users(users) | EventScope::Revocation(users) => users.iter().any(|id| id.0 == actor.user_id),
            EventScope::Session(sid) => *sid == actor.sid,
            EventScope::Room(room) => messages::lock_read(client, actor.user_id, room.0).await?,
            EventScope::RoomUsers(room, users) => users.iter().any(|id| id.0 == actor.user_id) && messages::lock_read(client, actor.user_id, room.0).await?,
            EventScope::Channel(channel) => {
                !client.query_typed(
                    "SELECT c.id FROM channels c WHERE c.id=$1 AND c.is_available AND EXISTS
                     (SELECT FROM channel_members WHERE channel_id=c.id AND user_id=$2) FOR SHARE OF c",
                    &[(&channel.0, Type::INT8), (&actor.user_id, Type::INT8)],
                ).await?.is_empty()
            }
        };
        if !permitted {
            return Ok(None);
        }
        let mut data = self.data.clone();
        if self.kind == "MESSAGE_CREATE" {
            let room: Id = serde_json::from_value(data["room_id"].clone())
                .map_err(|_| ApiError::internal())?;
            let id: Id = serde_json::from_value(data["message_id"].clone())
                .map_err(|_| ApiError::internal())?;
            let Some(view) = messages::message_view(client, actor.user_id, room.0, id.0).await?
            else {
                return Ok(None);
            };
            data = view;
        }
        Ok(Some(json!({
            "event_id": self.id, "type": self.kind, "stream": self.stream, "revision": self.revision,
            "ordinal": 0, "occurred_at": self.occurred_at, "data": data,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_scopes_keep_native_wire_format() {
        let encoded = json!({"RoomUsers": ["9007199254740993", ["9007199254740994"]]});
        let scope: EventScope = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(serde_json::to_value(scope).unwrap(), encoded);
        assert!(
            serde_json::from_value::<EventScope>(json!({"Room": 9007199254740993_i64})).is_err()
        );
    }
}
