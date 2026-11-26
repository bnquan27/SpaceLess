use std::collections::BTreeMap;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio_postgres::{Client, GenericClient, Row, Transaction, types::Type};
use uuid::Uuid;
use worker::{Bucket, Conditional, Env, HttpMetadata, Method, Object, Range, Request, Response};

use crate::{
    auth::{self, Actor, ApiError},
    db,
    events::Id,
    messages,
};

#[path = "../../src/file_content.rs"]
mod file_content;

type Result<T> = std::result::Result<T, ApiError>;
const STAGED: &str = "uploads/worker/staging/";
const FINAL: &str = "uploads/worker/final/";
const PUT_SECONDS: i64 = 300;
const STAGED_SECONDS: i64 = 3600;

pub(crate) enum Route {
    PresignAttachment(i64),
    PresignAvatar,
    PresignChannel(i64),
    AttachmentUrl(i64),
    ImageUrl(i64),
    Download(i64),
}

pub(crate) fn route(req: &Request) -> Result<Option<Route>> {
    let path = req.path();
    let path = path.split('/').collect::<Vec<_>>();
    let id = |value: &str| {
        Id::try_from(value.to_owned())
            .map(|id| id.0)
            .map_err(|_| bad("INVALID_PATH", "Đường dẫn không hợp lệ."))
    };
    let route = match (req.method(), path.as_slice()) {
        (Method::Post, ["", "api", "v1", "rooms", room, "uploads", "presign"]) => {
            Route::PresignAttachment(id(room)?)
        }
        (
            Method::Post,
            [
                "",
                "api",
                "v1",
                "users",
                "me",
                "avatar",
                "uploads",
                "presign",
            ],
        ) => Route::PresignAvatar,
        (
            Method::Post,
            [
                "",
                "api",
                "v1",
                "channels",
                channel,
                "images",
                "uploads",
                "presign",
            ],
        ) => Route::PresignChannel(id(channel)?),
        (Method::Get, ["", "api", "v1", "attachments", attachment, "url"]) => {
            Route::AttachmentUrl(id(attachment)?)
        }
        (Method::Get, ["", "api", "v1", "images", upload, "url"]) => Route::ImageUrl(id(upload)?),
        (Method::Get, ["", "files", upload]) => Route::Download(id(upload)?),
        _ => return Ok(None),
    };
    Ok(Some(route))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Upload {
    file_name: String,
    mime_type: String,
    file_size_bytes: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelUpload {
    file_name: String,
    mime_type: String,
    file_size_bytes: i64,
    purpose: ChannelImage,
}

#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ChannelImage {
    Icon,
    Background,
}

enum Destination {
    Attachment(i64),
    Avatar,
    Channel(i64, ChannelImage),
}

impl Destination {
    fn purpose(&self) -> &'static str {
        match self {
            Self::Attachment(_) => "ATTACHMENT",
            Self::Avatar => "AVATAR",
            Self::Channel(_, ChannelImage::Icon) => "CHANNEL_ICON",
            Self::Channel(_, ChannelImage::Background) => "CHANNEL_BACKGROUND",
        }
    }

    fn ids(&self) -> (Option<i64>, Option<i64>) {
        match self {
            Self::Attachment(room) => (Some(*room), None),
            Self::Channel(channel, _) => (None, Some(*channel)),
            Self::Avatar => (None, None),
        }
    }
}

fn bad(code: &'static str, message: &'static str) -> ApiError {
    ApiError::new(400, code, message)
}
fn missing() -> ApiError {
    ApiError::new(404, "NOT_FOUND", "Không tìm thấy tài nguyên.")
}
fn conflict(message: &'static str) -> ApiError {
    ApiError::new(409, "CONFLICT", message)
}

fn validate_metadata(upload: &Upload, purpose: &str) -> Result<()> {
    let limit = match purpose {
        "AVATAR" | "CHANNEL_ICON" => 2_000_000,
        "CHANNEL_BACKGROUND" => 5_000_000,
        _ => 25_000_000,
    };
    if !(1..=limit).contains(&upload.file_size_bytes) {
        return Err(bad("BAD_REQUEST", "Kích thước tệp vượt giới hạn."));
    }
    if upload.file_name.trim().is_empty()
        || upload.file_name.chars().count() > 255
        || upload
            .file_name
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err(bad("BAD_REQUEST", "Tên tệp không hợp lệ."));
    }
    let mime = upload.mime_type.as_str();
    let image = matches!(mime, "image/png" | "image/jpeg" | "image/webp");
    let document = matches!(
        mime,
        "application/pdf"
            | "text/plain"
            | "text/csv"
            | "application/zip"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
    );
    if !image && (purpose != "ATTACHMENT" || !document) {
        return Err(bad("BAD_REQUEST", "Định dạng tệp không được hỗ trợ."));
    }
    Ok(())
}

fn bucket(env: &Env) -> Result<Bucket> {
    env.bucket("FILES").map_err(|_| ApiError::unavailable())
}

fn gateway_key(env: &Env) -> Result<String> {
    let key = env
        .secret("FILE_GATEWAY_SECRET")
        .map_err(|_| ApiError::unavailable())?
        .to_string();
    if key.len() < 32 {
        return Err(ApiError::unavailable());
    }
    Ok(key)
}

pub(crate) async fn handle(
    route: Route,
    req: Request,
    client: &mut Client,
    jwt_key: &[u8],
    env: &Env,
) -> Result<Response> {
    let actor = auth::actor(&req, jwt_key)?;
    match route {
        Route::PresignAttachment(room) => {
            presign(
                client,
                env,
                &actor,
                Destination::Attachment(room),
                auth::json_body(req).await?,
            )
            .await
        }
        Route::PresignAvatar => {
            presign(
                client,
                env,
                &actor,
                Destination::Avatar,
                auth::json_body(req).await?,
            )
            .await
        }
        Route::PresignChannel(channel) => {
            let body: ChannelUpload = auth::json_body(req).await?;
            let upload = Upload {
                file_name: body.file_name,
                mime_type: body.mime_type,
                file_size_bytes: body.file_size_bytes,
            };
            presign(
                client,
                env,
                &actor,
                Destination::Channel(channel, body.purpose),
                upload,
            )
            .await
        }
        Route::AttachmentUrl(attachment) => {
            signed_download(client, env, &req, &actor, FileLink::Attachment(attachment)).await
        }
        Route::ImageUrl(upload) => {
            signed_download(client, env, &req, &actor, FileLink::Image(upload)).await
        }
        Route::Download(upload) => download(client, env, &req, &actor, upload).await,
    }
}

async fn presign(
    client: &mut Client,
    env: &Env,
    actor: &Actor,
    destination: Destination,
    upload: Upload,
) -> Result<Response> {
    let purpose = destination.purpose();
    validate_metadata(&upload, purpose)?;
    let config = SigningConfig::from_env(env)?;
    let tx = client.transaction().await?;
    match destination {
        Destination::Attachment(room) => {
            if tx
                .query_typed_opt(
                    "SELECT r.id FROM rooms r JOIN channel_room_access a ON a.room_id=r.id
                 WHERE r.id=$1 AND r.kind='VOICE' AND r.is_available AND a.user_id=$2",
                    &[(&room, Type::INT8), (&actor.user_id, Type::INT8)],
                )
                .await?
                .is_some()
            {
                auth::lock_session(&tx, actor, false).await?;
                return Err(ApiError::new(
                    403,
                    "FORBIDDEN",
                    "Bạn không có quyền thực hiện thao tác này.",
                ));
            }
            messages::lock_write(&tx, actor, room).await?;
        }
        Destination::Avatar => auth::lock_session(&tx, actor, false).await?,
        Destination::Channel(channel, _) => {
            auth::lock_session(&tx, actor, false).await?;
            let visible = tx.query_typed_opt(
                "SELECT c.id FROM channels c JOIN channel_members m ON m.channel_id=c.id
                 WHERE c.id=$1 AND c.is_available AND m.user_id=$2 AND m.role_id IN (1,2) FOR SHARE OF c",
                &[(&channel, Type::INT8), (&actor.user_id, Type::INT8)],
            ).await?.is_some();
            if !visible {
                return Err(missing());
            }
        }
    }
    let id = db::snowflake(&tx).await?;
    let suffix = format!("{id}/{}", Uuid::new_v4());
    let staging_key = format!("{STAGED}{suffix}");
    let storage_key = format!("{FINAL}{suffix}");
    let (room, channel) = destination.ids();
    let now = Utc::now();
    let put_expires_at = now + chrono::Duration::seconds(PUT_SECONDS);
    let expires_at = now + chrono::Duration::seconds(STAGED_SECONDS);
    let headers = BTreeMap::from([
        ("content-length", upload.file_size_bytes.to_string()),
        ("content-type", upload.mime_type.clone()),
        ("if-none-match", "*".into()),
    ]);
    let signed = config.put_url(&staging_key, &headers, now)?;
    tx.query_typed(
        "INSERT INTO file_uploads(id,owner_id,room_id,channel_id,purpose,staging_key,storage_key,file_name,mime_type,size_bytes,created_at,expires_at,put_expires_at)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
        &[(&id, Type::INT8), (&actor.user_id, Type::INT8), (&room, Type::INT8), (&channel, Type::INT8),
          (&purpose, Type::TEXT), (&staging_key, Type::TEXT), (&storage_key, Type::TEXT), (&upload.file_name, Type::TEXT),
          (&upload.mime_type, Type::TEXT), (&upload.file_size_bytes, Type::INT8), (&now, Type::TIMESTAMPTZ),
          (&expires_at, Type::TIMESTAMPTZ), (&put_expires_at, Type::TIMESTAMPTZ)],
    ).await?;
    tx.commit().await?;
    Response::from_json(
        &json!({"upload_id": Id(id), "put_url": signed, "required_headers": headers,
        "expires_at": put_expires_at, "staged_expires_at": expires_at}),
    )
    .map_err(|_| ApiError::internal())
}

struct SigningConfig {
    host: String,
    bucket: String,
    access: String,
    key: String,
}

impl SigningConfig {
    fn from_env(env: &Env) -> Result<Self> {
        let account = env
            .var("R2_ACCOUNT_ID")
            .map_err(|_| ApiError::unavailable())?
            .to_string();
        let bucket = env
            .var("R2_BUCKET")
            .map_err(|_| ApiError::unavailable())?
            .to_string();
        let access = env
            .secret("R2_ACCESS_KEY_ID")
            .map_err(|_| ApiError::unavailable())?
            .to_string();
        let key = env
            .secret("R2_SECRET_ACCESS_KEY")
            .map_err(|_| ApiError::unavailable())?
            .to_string();
        if account.len() != 32
            || !account.bytes().all(|c| c.is_ascii_hexdigit())
            || bucket.is_empty()
            || !bucket
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            || access.is_empty()
            || key.is_empty()
        {
            return Err(ApiError::unavailable());
        }
        Ok(Self {
            host: format!("{account}.r2.cloudflarestorage.com"),
            bucket,
            access,
            key,
        })
    }

    fn put_url(
        &self,
        object: &str,
        input: &BTreeMap<&str, String>,
        now: DateTime<Utc>,
    ) -> Result<String> {
        let path = format!(
            "/{}/{}",
            self.bucket,
            object
                .split('/')
                .map(encode_component)
                .collect::<Vec<_>>()
                .join("/")
        );
        let timestamp = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date = &timestamp[..8];
        let scope = format!("{date}/auto/s3/aws4_request");
        let mut headers = input.clone();
        headers.insert("host", self.host.clone());
        let signed_headers = headers.keys().copied().collect::<Vec<_>>().join(";");
        let canonical_headers = headers
            .iter()
            .map(|(k, v)| format!("{k}:{v}\n"))
            .collect::<String>();
        let query = BTreeMap::from([
            ("X-Amz-Algorithm", "AWS4-HMAC-SHA256".into()),
            ("X-Amz-Credential", format!("{}/{scope}", self.access)),
            ("X-Amz-Date", timestamp.clone()),
            ("X-Amz-Expires", PUT_SECONDS.to_string()),
            ("X-Amz-SignedHeaders", signed_headers.clone()),
        ])
        .into_iter()
        .map(|(k, v)| format!("{k}={}", encode_component(&v)))
        .collect::<Vec<_>>()
        .join("&");
        let canonical = format!(
            "PUT\n{path}\n{query}\n{canonical_headers}\n{signed_headers}\nUNSIGNED-PAYLOAD"
        );
        let hash = Sha256::digest(canonical.as_bytes());
        let string_to_sign = format!("AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{hash:x}");
        let key = mac(format!("AWS4{}", self.key).as_bytes(), date.as_bytes())?;
        let key = mac(&key, b"auto")?;
        let key = mac(&key, b"s3")?;
        let key = mac(&key, b"aws4_request")?;
        let signature = mac(&key, string_to_sign.as_bytes())?;
        let signature = signature
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Ok(format!(
            "https://{}{path}?{query}&X-Amz-Signature={signature}",
            self.host
        ))
    }
}

fn encode_component(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn mac(key: &[u8], bytes: &[u8]) -> Result<[u8; 32]> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| ApiError::internal())?;
    mac.update(bytes);
    Ok(mac.finalize().into_bytes().into())
}

pub(crate) struct VerifiedUpload {
    id: i64,
    version: String,
    etag: String,
}

pub(crate) async fn verify_uploads(
    env: &Env,
    client: &mut Client,
    actor: i64,
    room: i64,
    uploads: &[i64],
) -> Result<Vec<VerifiedUpload>> {
    if uploads.is_empty() {
        return Ok(Vec::new());
    }
    let bucket = bucket(env)?;
    let mut verified = Vec::with_capacity(uploads.len());
    for id in uploads {
        let tx = client.transaction().await?;
        let row = tx
            .query_typed_opt(
                "SELECT id,staging_key,storage_key,size_bytes,mime_type FROM file_uploads
             WHERE id=$1 AND owner_id=$2 AND room_id=$3 AND purpose='ATTACHMENT'
               AND status='STAGED' AND expires_at>clock_timestamp() FOR UPDATE",
                &[(id, Type::INT8), (&actor, Type::INT8), (&room, Type::INT8)],
            )
            .await?
            .ok_or_else(|| conflict("Tệp không còn khả dụng."))?;
        verified.push(verify(&bucket, row).await?);
        tx.commit().await?;
    }
    Ok(verified)
}

pub(crate) async fn attach_message(
    tx: &Transaction<'_>,
    actor: i64,
    room: i64,
    message: i64,
    uploads: Vec<VerifiedUpload>,
) -> Result<()> {
    for upload in uploads {
        let row = tx.query_typed_opt(
            "SELECT id FROM file_uploads WHERE id=$1 AND owner_id=$2 AND room_id=$3
             AND purpose='ATTACHMENT' AND status='STAGED' AND expires_at>clock_timestamp() FOR UPDATE",
            &[(&upload.id, Type::INT8), (&actor, Type::INT8), (&room, Type::INT8)],
        ).await?;
        if row.is_none() {
            return Err(conflict("Tệp đã hết hạn hoặc đã được sử dụng."));
        }
        let id = upload.id;
        mark_attached(tx, upload).await?;
        let attachment = db::snowflake(tx).await?;
        tx.query_typed(
            "INSERT INTO message_attachments(id,message_id,upload_id) VALUES($1,$2,$3)",
            &[
                (&attachment, Type::INT8),
                (&message, Type::INT8),
                (&id, Type::INT8),
            ],
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn attach_image(
    env: &Env,
    tx: &Transaction<'_>,
    actor: i64,
    channel: Option<i64>,
    upload: i64,
    purpose: &str,
) -> Result<()> {
    let row = tx.query_typed_opt(
        "SELECT id,staging_key,storage_key,size_bytes,mime_type FROM file_uploads WHERE id=$1 AND owner_id=$2
         AND channel_id IS NOT DISTINCT FROM $3 AND purpose=$4 AND status='STAGED' AND expires_at>clock_timestamp() FOR UPDATE",
         &[(&upload, Type::INT8), (&actor, Type::INT8), (&channel, Type::INT8), (&purpose, Type::TEXT)],
    ).await?.ok_or_else(|| conflict("Ảnh không còn khả dụng."))?;
    let verified = verify(&bucket(env)?, row).await?;
    mark_attached(tx, verified).await
}

pub(crate) async fn discard_image(tx: &Transaction<'_>, upload: i64) -> Result<()> {
    tx.query_typed(
        "UPDATE file_uploads SET status='DELETE_PENDING' WHERE id=$1",
        &[(&upload, Type::INT8)],
    )
    .await?;
    Ok(())
}

async fn mark_attached(tx: &Transaction<'_>, upload: VerifiedUpload) -> Result<()> {
    let changed = tx
        .query_typed_opt(
            "UPDATE file_uploads SET status='ATTACHED',object_version=$2,etag=$3
         WHERE id=$1 AND status='STAGED' AND expires_at>clock_timestamp() RETURNING id",
            &[
                (&upload.id, Type::INT8),
                (&upload.version, Type::TEXT),
                (&upload.etag, Type::TEXT),
            ],
        )
        .await?;
    if changed.is_none() {
        return Err(conflict("Tệp đã hết hạn hoặc đã được sử dụng."));
    }
    Ok(())
}

fn object_key(key: &str, prefix: &str) -> bool {
    let Some((id, suffix)) = key
        .strip_prefix(prefix)
        .and_then(|value| value.split_once('/'))
    else {
        return false;
    };
    Id::try_from(id.to_owned()).is_ok()
        && Uuid::parse_str(suffix).is_ok_and(|uuid| uuid.to_string() == suffix)
}

async fn read_bytes(object: &Object, size: u64, mime: &str) -> Result<Vec<u8>> {
    if object.size() != size || object.http_metadata().content_type.as_deref() != Some(mime) {
        return Err(bad("BAD_REQUEST", "Dữ liệu tệp không khớp khai báo."));
    }
    let mut stream = object
        .body()
        .ok_or_else(ApiError::unavailable)?
        .stream()
        .map_err(|_| ApiError::unavailable())?;
    let mut bytes = Vec::with_capacity(size as usize);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ApiError::unavailable())?;
        if chunk.len() > size as usize - bytes.len() {
            return Err(bad("BAD_REQUEST", "Kích thước tệp không hợp lệ."));
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() != size as usize {
        return Err(bad("BAD_REQUEST", "Kích thước tệp không hợp lệ."));
    }
    Ok(bytes)
}

fn validate_bytes(bytes: &[u8], mime: &str) -> Result<()> {
    file_content::validate_bytes(bytes, mime, 64 * 1024 * 1024).map_err(|_| {
        bad(
            "BAD_REQUEST",
            "Nội dung tệp không đúng định dạng đã khai báo.",
        )
    })
}

async fn verify(bucket: &Bucket, row: Row) -> Result<VerifiedUpload> {
    let id: i64 = row.try_get("id")?;
    let staging: String = row.try_get("staging_key")?;
    let storage: String = row.try_get("storage_key")?;
    let size: i64 = row.try_get("size_bytes")?;
    let mime: String = row.try_get("mime_type")?;
    if !object_key(&staging, STAGED)
        || !object_key(&storage, FINAL)
        || !(1..=25_000_000).contains(&size)
    {
        return Err(conflict("Tệp không còn khả dụng."));
    }
    let object = bucket
        .get(staging)
        .execute()
        .await
        .map_err(|_| ApiError::unavailable())?
        .ok_or_else(|| conflict("Tệp chưa được tải lên thành công."))?;
    let bytes = read_bytes(&object, size as u64, &mime).await?;
    validate_bytes(&bytes, &mime)?;
    let digest = Sha256::digest(&bytes);
    let conditional = Conditional {
        etag_does_not_match: Some("*".into()),
        ..Conditional::default()
    };
    let final_object = bucket
        .put(&storage, bytes)
        .http_metadata(HttpMetadata {
            content_type: Some(mime.clone()),
            ..HttpMetadata::default()
        })
        .only_if(conditional)
        .sha256(digest.to_vec())
        .execute()
        .await
        .map_err(|_| ApiError::unavailable())?;
    let object = if let Some(object) = final_object {
        object
    } else {
        let existing = bucket
            .get(storage)
            .execute()
            .await
            .map_err(|_| ApiError::unavailable())?
            .ok_or_else(ApiError::unavailable)?;
        if Sha256::digest(read_bytes(&existing, size as u64, &mime).await?) != digest {
            return Err(conflict("Nội dung tệp đã thay đổi."));
        }
        existing
    };
    Ok(VerifiedUpload {
        id,
        version: object.version(),
        etag: object.http_etag(),
    })
}

#[derive(Deserialize, Serialize)]
struct DownloadToken {
    sub: Id,
    jti: Uuid,
    upload_id: Id,
    exp: i64,
    aud: String,
}

const JWT_HEADER: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9";

fn sign_download(claims: &DownloadToken, key: &[u8]) -> Result<String> {
    let body = serde_json::to_vec(claims).map_err(|_| ApiError::internal())?;
    let signed = format!("{JWT_HEADER}.{}", URL_SAFE_NO_PAD.encode(body));
    let signature = mac(key, signed.as_bytes())?;
    Ok(format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature)))
}

fn verify_download(token: &str, key: &[u8], actor: &Actor, upload: i64, now: i64) -> Result<()> {
    let invalid = ApiError::unauthorized;
    if token.len() > 1024 {
        return Err(invalid());
    }
    let (signed, signature) = token.rsplit_once('.').ok_or_else(invalid)?;
    let (header, data) = signed.split_once('.').ok_or_else(invalid)?;
    if header != JWT_HEADER {
        return Err(invalid());
    }
    let bytes = URL_SAFE_NO_PAD.decode(signature).map_err(|_| invalid())?;
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| ApiError::internal())?;
    mac.update(signed.as_bytes());
    mac.verify_slice(&bytes).map_err(|_| invalid())?;
    let data = URL_SAFE_NO_PAD.decode(data).map_err(|_| invalid())?;
    let claims: DownloadToken = serde_json::from_slice(&data).map_err(|_| invalid())?;
    if claims.exp <= now
        || claims.aud != "spaceless-files"
        || claims.sub.0 != actor.user_id
        || claims.jti != actor.sid
        || claims.upload_id.0 != upload
    {
        return Err(invalid());
    }
    Ok(())
}

enum FileLink {
    Attachment(i64),
    Image(i64),
}

async fn authorize<C: GenericClient + Sync>(tx: &C, user: i64, id: i64) -> Result<Row> {
    let row = tx.query_typed_opt(
        "SELECT owner_id,room_id,channel_id,purpose,storage_key,object_version,etag,mime_type,size_bytes FROM file_uploads
         WHERE id=$1 AND status='ATTACHED'", &[(&id, Type::INT8)],
    ).await?.ok_or_else(missing)?;
    let purpose: String = row.try_get("purpose")?;
    let permitted = match purpose.as_str() {
        "ATTACHMENT" => {
            let room: i64 = row.try_get("room_id")?;
            messages::lock_read(tx, user, room).await?
                && tx
                    .query_typed_opt(
                        "SELECT upload_id FROM message_attachments WHERE upload_id=$1",
                        &[(&id, Type::INT8)],
                    )
                    .await?
                    .is_some()
        }
        "AVATAR" => {
            let owner: i64 = row.try_get("owner_id")?;
            tx.query_typed_opt(
                "SELECT u.id FROM users u JOIN user_details d ON d.user_id=u.id WHERE u.id=$1 AND u.is_available AND d.avatar_upload_id=$2",
                &[(&owner, Type::INT8), (&id, Type::INT8)],
            ).await?.is_some()
        }
        "CHANNEL_ICON" | "CHANNEL_BACKGROUND" => {
            let channel: i64 = row.try_get("channel_id")?;
            tx.query_typed_opt(
               "SELECT c.id FROM channels c JOIN channel_members m ON m.channel_id=c.id WHERE c.id=$1 AND c.is_available
                AND m.user_id=$2 AND (c.icon_upload_id=$3 OR c.background_upload_id=$3) FOR SHARE OF c",
               &[(&channel, Type::INT8), (&user, Type::INT8), (&id, Type::INT8)],
            ).await?.is_some()
        }
        _ => false,
    };
    if !permitted {
        return Err(missing());
    }
    Ok(row)
}

async fn signed_download(
    client: &mut Client,
    env: &Env,
    req: &Request,
    actor: &Actor,
    link: FileLink,
) -> Result<Response> {
    let key = gateway_key(env)?;
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    let id = match link {
        FileLink::Attachment(id) => tx
            .query_typed_opt(
                "SELECT upload_id FROM message_attachments WHERE id=$1",
                &[(&id, Type::INT8)],
            )
            .await?
            .ok_or_else(missing)?
            .try_get(0)?,
        FileLink::Image(id) => id,
    };
    let row = authorize(&tx, actor.user_id, id).await?;
    if (row.try_get::<_, String>("purpose")? == "ATTACHMENT")
        != matches!(link, FileLink::Attachment(_))
    {
        return Err(missing());
    }
    let url = req
        .url()
        .map_err(|_| ApiError::internal())?
        .origin()
        .ascii_serialization();
    let exp = (Utc::now().timestamp() + 120).min(actor.exp);
    let token = sign_download(
        &DownloadToken {
            sub: Id(actor.user_id),
            jti: actor.sid,
            upload_id: Id(id),
            exp,
            aud: "spaceless-files".into(),
        },
        key.as_bytes(),
    )?;
    tx.commit().await?;
    Response::from_json(&json!({"url":format!("{url}/files/{id}?token={token}"), "expires_at":DateTime::from_timestamp(exp, 0)}))
        .map_err(|_| ApiError::internal())
}

fn requested_range(header: &str, size: u64) -> Result<(u64, u64)> {
    let invalid = || bad("BAD_REQUEST", "Range không hợp lệ.");
    if header.len() > 100 || header.contains(',') || size == 0 {
        return Err(invalid());
    }
    let (start, end) = header
        .strip_prefix("bytes=")
        .and_then(|range| range.split_once('-'))
        .ok_or_else(invalid)?;
    let parse = |value: &str| {
        if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
            value.parse::<u64>().map_err(|_| invalid())
        } else {
            Err(invalid())
        }
    };
    let (offset, end) = if start.is_empty() {
        let suffix = parse(end)?;
        (size.saturating_sub(suffix), size - 1)
    } else {
        (
            parse(start)?,
            if end.is_empty() {
                size - 1
            } else {
                parse(end)?.min(size - 1)
            },
        )
    };
    if offset > end {
        return Err(invalid());
    }
    Ok((offset, end + 1 - offset))
}

async fn download(
    client: &mut Client,
    env: &Env,
    req: &Request,
    actor: &Actor,
    id: i64,
) -> Result<Response> {
    let key = gateway_key(env)?;
    let url = req.url().map_err(|_| ApiError::unauthorized())?;
    let mut params = url.query_pairs();
    let (name, token) = params.next().ok_or_else(ApiError::unauthorized)?;
    if name != "token" || params.next().is_some() {
        return Err(ApiError::unauthorized());
    }
    verify_download(&token, key.as_bytes(), actor, id, Utc::now().timestamp())?;
    let tx = client.transaction().await?;
    auth::lock_session(&tx, actor, false).await?;
    let upload = authorize(&tx, actor.user_id, id).await?;
    let bucket = bucket(env)?;
    let storage: String = upload.try_get("storage_key")?;
    if !object_key(&storage, FINAL) {
        return Err(missing());
    }
    let expected_version: Option<String> = upload.try_get("object_version")?;
    let expected_etag: String = upload.try_get("etag")?;
    let size: i64 = upload.try_get("size_bytes")?;
    if !(1..=25_000_000).contains(&size) {
        return Err(ApiError::unavailable());
    }
    let size = size as u64;
    let range = req
        .headers()
        .get("Range")
        .map_err(|_| bad("BAD_REQUEST", "Range không hợp lệ."))?
        .as_deref()
        .map(|header| requested_range(header, size))
        .transpose()?;
    let mut get = bucket.get(storage).only_if(Conditional {
        etag_matches: Some(expected_etag.trim_matches('"').to_owned()),
        ..Conditional::default()
    });
    if let Some((offset, length)) = range {
        get = get.range(Range::OffsetWithLength { offset, length });
    }
    let object = get
        .execute()
        .await
        .map_err(|_| ApiError::unavailable())?
        .ok_or_else(missing)?;
    let mime: String = upload.try_get("mime_type")?;
    if object.size() != size
        || object.http_metadata().content_type.as_deref() != Some(mime.as_str())
        || expected_version.as_deref() != Some(&object.version())
        || object.http_etag() != expected_etag
    {
        return Err(ApiError::unavailable());
    }
    let stream = object
        .body()
        .ok_or_else(ApiError::unavailable)?
        .stream()
        .map_err(|_| ApiError::unavailable())?;
    tx.commit().await?;
    let mut response = Response::from_stream(stream).map_err(|_| ApiError::internal())?;
    let headers = response.headers_mut();
    let length = range.map_or(size, |(_, length)| length).to_string();
    for (key, value) in [
        ("Content-Type", mime.as_str()),
        ("Content-Length", length.as_str()),
        ("Content-Disposition", "attachment"),
        ("Accept-Ranges", "bytes"),
        ("Cache-Control", "private, no-store"),
        ("X-Content-Type-Options", "nosniff"),
    ] {
        headers.set(key, value).map_err(|_| ApiError::internal())?;
    }
    if let Some((offset, length)) = range {
        headers
            .set(
                "Content-Range",
                &format!("bytes {offset}-{}/{size}", offset + length - 1),
            )
            .map_err(|_| ApiError::internal())?;
        response = response.with_status(206);
    }
    Ok(response)
}

pub(crate) async fn cleanup(env: &Env, client: &mut Client) -> Result<()> {
    let Ok(bucket) = env.bucket("FILES") else {
        return Ok(());
    };
    for _ in 0..10 {
        let tx = client.transaction().await?;
        let row = tx.query_typed_opt(
            "SELECT id,staging_key,storage_key,status FROM file_uploads WHERE storage_key LIKE 'uploads/worker/final/%'
             AND put_expires_at<=now() AND (status='DELETE_PENDING' OR (status='STAGED' AND expires_at<=now())
             OR (status='ATTACHED' AND staging_key IS NOT NULL)) ORDER BY id FOR UPDATE SKIP LOCKED LIMIT 1", &[],
        ).await?;
        let Some(row) = row else {
            break;
        };
        let staging: Option<String> = row.try_get("staging_key")?;
        let storage: String = row.try_get("storage_key")?;
        let status: String = row.try_get("status")?;
        let id: i64 = row.try_get("id")?;
        if let Some(key) = staging {
            if !object_key(&key, STAGED) {
                return Err(ApiError::internal());
            }
            bucket
                .delete(key)
                .await
                .map_err(|_| ApiError::unavailable())?;
        }
        if status == "ATTACHED" {
            tx.query_typed(
                "UPDATE file_uploads SET staging_key=NULL WHERE id=$1",
                &[(&id, Type::INT8)],
            )
            .await?;
        } else {
            if !object_key(&storage, FINAL) {
                return Err(ApiError::internal());
            }
            bucket
                .delete(storage)
                .await
                .map_err(|_| ApiError::unavailable())?;
            tx.query_typed("DELETE FROM file_uploads WHERE id=$1", &[(&id, Type::INT8)])
                .await?;
        }
        tx.commit().await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presigned_put_matches_independent_hmac_vector() {
        let config = SigningConfig {
            host: "0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com".into(),
            bucket: "spaceless-fixture".into(),
            access: "fixture-access".into(),
            key: "fixture-secret".into(),
        };
        let headers = BTreeMap::from([
            ("content-length", "3".into()),
            ("content-type", "text/plain".into()),
            ("if-none-match", "*".into()),
        ]);
        let now = DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
            .unwrap()
            .to_utc();
        let key = "uploads/worker/staging/9007199254740993/03f6504d-6845-4303-a07b-e9a588a59f6f";
        let signed = config.put_url(key, &headers, now).unwrap();
        assert!(signed.ends_with(
            "X-Amz-Signature=72d1f20a96303ee2ab69cbec2c200e91ce5882b514075d700a6ff3baf9d50c8b"
        ));
        assert!(signed.contains("content-length%3Bcontent-type%3Bhost%3Bif-none-match"));
    }

    #[test]
    fn ranges_resolve_with_no_wrap_or_overread() {
        assert_eq!(requested_range("bytes=2-6", 8).unwrap(), (2, 5));
        assert_eq!(requested_range("bytes=2-", 8).unwrap(), (2, 6));
        assert_eq!(requested_range("bytes=-3", 8).unwrap(), (5, 3));
        assert_eq!(
            requested_range("bytes=0-18446744073709551615", 8).unwrap(),
            (0, 8)
        );
        for header in [
            "bytes=3-2",
            "bytes=8-",
            "bytes=0-2,4-6",
            "bytes=-0",
            "bytes=+1-3",
        ] {
            assert!(requested_range(header, 8).is_err(), "{header}");
        }
    }

    #[test]
    fn download_is_bound_to_session_resource_and_expiration() {
        let mut actor = Actor {
            user_id: 9_007_199_254_740_993,
            sid: Uuid::new_v4(),
            jti: Uuid::new_v4(),
            exp: 200,
            auth_version: 1,
        };
        let key = b"independent-fixture-download-signing-key";
        let claims = DownloadToken {
            sub: Id(actor.user_id),
            jti: actor.sid,
            upload_id: Id(7),
            exp: 120,
            aud: "spaceless-files".into(),
        };
        let token = sign_download(&claims, key).unwrap();
        assert!(verify_download(&token, key, &actor, 7, 119).is_ok());
        assert!(verify_download(&token, key, &actor, 7, 120).is_err());
        assert!(verify_download(&token, key, &actor, 8, 119).is_err());
        actor.sid = Uuid::new_v4();
        assert!(verify_download(&token, key, &actor, 7, 119).is_err());
    }

    #[test]
    fn content_and_storage_namespace_are_checked() {
        assert!(validate_bytes(b"UTF-8 fixture", "text/plain").is_ok());
        assert!(validate_bytes(b"UTF-8 fixture\0", "text/plain").is_err());
        assert!(validate_bytes(b"%PDF-1.7\n%%EOF", "application/pdf").is_ok());
        assert!(validate_bytes(b"%PDF-1.7 only header", "application/pdf").is_err());
        assert!(validate_bytes(b"GIF89a", "image/png").is_err());
        assert!(object_key(
            "uploads/worker/final/9007199254740993/03f6504d-6845-4303-a07b-e9a588a59f6f",
            FINAL
        ));
        assert!(!object_key(
            "uploads/worker/final/../03f6504d-6845-4303-a07b-e9a588a59f6f",
            FINAL
        ));
        assert!(!object_key(
            "uploads/final/9007199254740993/03f6504d-6845-4303-a07b-e9a588a59f6f",
            FINAL
        ));
    }
}
