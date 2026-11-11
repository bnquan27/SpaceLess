use std::{collections::BTreeMap, io::Cursor, time::Duration};

use aws_sdk_s3::{
    Client, operation::get_object::GetObjectOutput, presigning::PresigningConfig,
    primitives::ByteStream,
};
use axum::{
    Json, Router,
    body::Body as HttpBody,
    http::{HeaderMap, StatusCode, header},
    response::Response,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgConnection, Row};
use tokio::{io::AsyncReadExt, sync::Semaphore};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::{Actor, ApiError, AppState, Body, Id, Path, Query, Result, db};

static ATTACHMENT_VERIFICATION: Semaphore = Semaphore::const_new(4);

#[derive(Clone)]
pub struct Config {
    pub s3: Client,
    pub bucket: String,
    pub gateway_base_url: String,
    gateway_secret: String,
}

impl Config {
    pub async fn from_env() -> Result<Option<Self>> {
        let Ok(bucket) = std::env::var("S3_BUCKET") else {
            return Ok(None);
        };
        let gateway_base_url =
            std::env::var("FILE_GATEWAY_PUBLIC_URL").map_err(|_| ApiError::unavailable())?;
        let gateway_secret =
            std::env::var("FILE_GATEWAY_SECRET").map_err(|_| ApiError::unavailable())?;
        if gateway_secret.len() < 32 || !gateway_base_url.starts_with("http") {
            return Err(ApiError::bad("Cấu hình cổng tải tệp không hợp lệ."));
        }
        let sdk = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .load()
            .await;
        let mut config = aws_sdk_s3::config::Builder::from(&sdk);
        if let Ok(endpoint) = std::env::var("S3_ENDPOINT") {
            config = config.endpoint_url(endpoint).force_path_style(true);
        }
        Ok(Some(Self {
            s3: Client::from_conf(config.build()),
            bucket,
            gateway_base_url: gateway_base_url.trim_end_matches('/').to_owned(),
            gateway_secret,
        }))
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/rooms/{room_id}/uploads/presign", post(presign_message))
        .route("/users/me/avatar/uploads/presign", post(presign_avatar))
        .route(
            "/channels/{channel_id}/images/uploads/presign",
            post(presign_channel),
        )
        .route("/attachments/{attachment_id}/url", get(attachment_url))
        .route("/images/{upload_id}/url", get(image_download_url))
}

pub fn gateway_routes() -> Router<AppState> {
    Router::new().route("/files/{upload_id}", get(download))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UploadInput {
    file_name: String,
    mime_type: String,
    file_size_bytes: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelUploadInput {
    file_name: String,
    mime_type: String,
    file_size_bytes: i64,
    purpose: ImagePurpose,
}

#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ImagePurpose {
    Icon,
    Background,
}

fn validate_metadata(input: &UploadInput, purpose: &str) -> Result<()> {
    let limit = match purpose {
        "AVATAR" | "CHANNEL_ICON" => 2_000_000,
        "CHANNEL_BACKGROUND" => 5_000_000,
        _ => 25_000_000,
    };
    if input.file_size_bytes < 1 || input.file_size_bytes > limit {
        return Err(ApiError::bad("Kích thước tệp vượt giới hạn."));
    }
    if input.file_name.trim().is_empty()
        || input.file_name.chars().count() > 255
        || input
            .file_name
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err(ApiError::bad("Tên tệp không hợp lệ."));
    }
    let image = matches!(
        input.mime_type.as_str(),
        "image/png" | "image/jpeg" | "image/webp"
    );
    let document = matches!(
        input.mime_type.as_str(),
        "application/pdf"
            | "text/plain"
            | "text/csv"
            | "application/zip"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
    );
    if !image && (purpose != "ATTACHMENT" || !document) {
        return Err(ApiError::bad("Định dạng tệp không được hỗ trợ."));
    }
    Ok(())
}

async fn presign_message(
    axum::extract::State(state): axum::extract::State<AppState>,
    actor: Actor,
    Path(room): Path<Id>,
    Body(input): Body<UploadInput>,
) -> Result<Json<serde_json::Value>> {
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    let access = db::room_access(&mut tx, actor.user_id, room).await?;
    if let Some(channel) = access.channel_id {
        db::lock_channel(&mut tx, channel).await?;
    }
    if access.kind == "DM" {
        let pair = sqlx::query("SELECT lo,hi FROM dm_pairs WHERE room_id=$1")
            .bind(room.0)
            .fetch_one(&mut *tx)
            .await?;
        db::lock_pair(&mut tx, Id(pair.get("lo")), Id(pair.get("hi"))).await?;
    }
    db::lock_room(&mut tx, room).await?;
    let access = db::room_access(&mut tx, actor.user_id, room).await?;
    if !access.writable || access.kind == "VOICE" {
        return Err(ApiError::forbidden());
    }
    let response = presign(
        &state,
        &mut tx,
        actor.user_id,
        Some(room),
        None,
        "ATTACHMENT",
        input,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(response))
}

async fn presign_avatar(
    axum::extract::State(state): axum::extract::State<AppState>,
    actor: Actor,
    Body(input): Body<UploadInput>,
) -> Result<Json<serde_json::Value>> {
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    let response = presign(&state, &mut tx, actor.user_id, None, None, "AVATAR", input).await?;
    tx.commit().await?;
    Ok(Json(response))
}

async fn presign_channel(
    axum::extract::State(state): axum::extract::State<AppState>,
    actor: Actor,
    Path(channel): Path<Id>,
    Body(input): Body<ChannelUploadInput>,
) -> Result<Json<serde_json::Value>> {
    let mut tx = state.pool.begin().await?;
    db::lock_session(&mut tx, &actor).await?;
    db::lock_channel(&mut tx, channel).await?;
    let allowed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM channels c JOIN channel_members m ON m.channel_id=c.id WHERE c.id=$1 AND c.is_available AND m.user_id=$2 AND m.role_id IN (1,2))").bind(channel.0).bind(actor.user_id.0).fetch_one(&mut *tx).await?;
    if !allowed {
        return Err(ApiError::not_found());
    }
    let purpose = match input.purpose {
        ImagePurpose::Icon => "CHANNEL_ICON",
        ImagePurpose::Background => "CHANNEL_BACKGROUND",
    };
    let response = presign(
        &state,
        &mut tx,
        actor.user_id,
        None,
        Some(channel),
        purpose,
        UploadInput {
            file_name: input.file_name,
            mime_type: input.mime_type,
            file_size_bytes: input.file_size_bytes,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(response))
}

async fn presign(
    state: &AppState,
    conn: &mut PgConnection,
    owner: Id,
    room: Option<Id>,
    channel: Option<Id>,
    purpose: &str,
    input: UploadInput,
) -> Result<serde_json::Value> {
    validate_metadata(&input, purpose)?;
    let config = state.files.as_ref().ok_or_else(ApiError::unavailable)?;
    let id = state.id().await?;
    let suffix = format!("{}/{}", id.0, Uuid::new_v4());
    let staging_key = format!("uploads/staging/{suffix}");
    let storage_key = format!("uploads/final/{suffix}");
    let now = Utc::now();
    let put_expires_at = now + chrono::Duration::seconds(300);
    let expires_at = now + chrono::Duration::seconds(3600);
    let signed = config
        .s3
        .put_object()
        .bucket(&config.bucket)
        .key(&staging_key)
        .content_type(&input.mime_type)
        .content_length(input.file_size_bytes)
        .if_none_match("*")
        .presigned(
            PresigningConfig::builder()
                .start_time(now.into())
                .expires_in(Duration::from_secs(300))
                .build()
                .map_err(|_| ApiError::internal())?,
        )
        .await
        .map_err(|_| ApiError::unavailable())?;
    let mut headers: BTreeMap<String, String> = signed
        .headers()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    headers.insert("content-length".into(), input.file_size_bytes.to_string());
    headers.insert("content-type".into(), input.mime_type.clone());
    headers.insert("if-none-match".into(), "*".into());
    sqlx::query("INSERT INTO file_uploads(id,owner_id,room_id,channel_id,purpose,staging_key,storage_key,file_name,mime_type,size_bytes,created_at,expires_at,put_expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
        .bind(id.0).bind(owner.0).bind(room.map(|x| x.0)).bind(channel.map(|x| x.0)).bind(purpose).bind(staging_key).bind(storage_key).bind(input.file_name).bind(input.mime_type).bind(input.file_size_bytes).bind(now).bind(expires_at).bind(put_expires_at).execute(conn).await?;
    Ok(
        json!({"upload_id":id,"put_url":signed.uri(),"required_headers":headers,"expires_at":put_expires_at,"staged_expires_at":expires_at}),
    )
}

pub struct VerifiedUpload {
    id: Id,
    version: Option<String>,
    etag: String,
}

pub async fn verify_uploads(
    state: &AppState,
    actor: Id,
    room: Id,
    ids: &[Id],
) -> Result<Vec<VerifiedUpload>> {
    let mut result = Vec::with_capacity(ids.len());
    for id in ids {
        let _permit = ATTACHMENT_VERIFICATION
            .acquire()
            .await
            .map_err(|_| ApiError::unavailable())?;
        let mut tx = state.pool.begin().await?;
        let row = sqlx::query("SELECT * FROM file_uploads WHERE id=$1 AND owner_id=$2 AND room_id=$3 AND purpose='ATTACHMENT' AND status='STAGED' AND expires_at>clock_timestamp() FOR UPDATE")
            .bind(id.0).bind(actor.0).bind(room.0).fetch_optional(&mut *tx).await?.ok_or_else(|| ApiError::conflict("Tệp không còn khả dụng."))?;
        result.push(verify_object(state, &row).await?);
        tx.commit().await?;
    }
    Ok(result)
}

async fn verify_object(state: &AppState, row: &sqlx::postgres::PgRow) -> Result<VerifiedUpload> {
    let config = state.files.as_ref().ok_or_else(ApiError::unavailable)?;
    let staging_key: String = row.get("staging_key");
    let key: String = row.get("storage_key");
    let expected: i64 = row.get("size_bytes");
    let mime: String = row.get("mime_type");
    let staged = config
        .s3
        .get_object()
        .bucket(&config.bucket)
        .key(staging_key)
        .send()
        .await
        .map_err(|_| ApiError::conflict("Tệp chưa được tải lên thành công."))?;
    let bytes = read_upload_bytes(staged, expected, &mime).await?;
    // The upload row lock coordinates cleanup. A previous attempt can already have written the final key.
    let (version, etag) = match config
        .s3
        .put_object()
        .bucket(&config.bucket)
        .key(&key)
        .content_type(&mime)
        .content_length(expected)
        .if_none_match("*")
        .body(ByteStream::from(bytes))
        .send()
        .await
    {
        Ok(object) => (
            object.version_id().map(str::to_owned),
            object.e_tag().map(str::to_owned),
        ),
        Err(error)
            if error
                .raw_response()
                .is_some_and(|response| response.status().as_u16() == 412) =>
        {
            (None, None)
        }
        Err(_) => return Err(ApiError::unavailable()),
    };
    let response = config
        .s3
        .get_object()
        .bucket(&config.bucket)
        .key(&key)
        .set_version_id(version)
        .set_if_match(etag)
        .send()
        .await
        .map_err(|_| ApiError::unavailable())?;
    let version = response.version_id().map(str::to_owned);
    let etag = response
        .e_tag()
        .ok_or_else(ApiError::unavailable)?
        .to_owned();
    let bytes = read_upload_bytes(response, expected, &mime).await?;
    tokio::task::spawn_blocking(move || validate_bytes(&bytes, &mime))
        .await
        .map_err(|_| ApiError::internal())??;
    Ok(VerifiedUpload {
        id: Id(row.get("id")),
        version,
        etag,
    })
}

async fn read_upload_bytes(
    response: GetObjectOutput,
    expected: i64,
    mime: &str,
) -> Result<Vec<u8>> {
    if response.content_length() != Some(expected) || response.content_type() != Some(mime) {
        return Err(ApiError::bad("Dữ liệu tệp không khớp khai báo."));
    }
    let mut bytes = Vec::with_capacity(expected as usize);
    response
        .body
        .into_async_read()
        .take(expected as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| ApiError::unavailable())?;
    if bytes.len() as i64 != expected {
        return Err(ApiError::bad("Kích thước tệp không hợp lệ."));
    }
    Ok(bytes)
}

fn validate_bytes(bytes: &[u8], mime: &str) -> Result<()> {
    let invalid = || ApiError::bad("Nội dung tệp không đúng định dạng đã khai báo.");
    if mime.starts_with("image/") {
        let format = match mime {
            "image/png" => image::ImageFormat::Png,
            "image/jpeg" => image::ImageFormat::Jpeg,
            "image/webp" => image::ImageFormat::WebP,
            _ => return Err(invalid()),
        };
        if image::guess_format(bytes).map_err(|_| invalid())? != format {
            return Err(invalid());
        }
        let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(16_384);
        limits.max_image_height = Some(16_384);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        reader.decode().map_err(|_| invalid())?;
    } else if mime == "application/pdf" {
        if !bytes.starts_with(b"%PDF-")
            || !bytes[bytes.len().saturating_sub(1024)..]
                .windows(5)
                .any(|w| w == b"%%EOF")
        {
            return Err(invalid());
        }
    } else if mime.starts_with("text/") {
        if std::str::from_utf8(bytes).is_err() || bytes.contains(&0) {
            return Err(invalid());
        }
    } else {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid())?;
        if zip.len() > 10_000 {
            return Err(invalid());
        }
        let required = match mime {
            "application/zip" => None,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                Some("word/document.xml")
            }
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => {
                Some("xl/workbook.xml")
            }
            "application/vnd.openxmlformats-officedocument.presentationml.presentation" => {
                Some("ppt/presentation.xml")
            }
            _ => return Err(invalid()),
        };
        if let Some(name) = required {
            zip.by_name("[Content_Types].xml").map_err(|_| invalid())?;
            zip.by_name(name).map_err(|_| invalid())?;
        }
    }
    Ok(())
}

pub async fn attach_message(
    state: &AppState,
    conn: &mut PgConnection,
    actor: Id,
    room: Id,
    message: Id,
    verified: Vec<VerifiedUpload>,
) -> Result<()> {
    for upload in verified {
        let locked = sqlx::query_scalar::<_, i64>("SELECT id FROM file_uploads WHERE id=$1 AND owner_id=$2 AND room_id=$3 AND purpose='ATTACHMENT' AND status='STAGED' FOR UPDATE")
            .bind(upload.id.0).bind(actor.0).bind(room.0).fetch_optional(&mut *conn).await?;
        let id =
            locked.ok_or_else(|| ApiError::conflict("Tệp đã hết hạn hoặc đã được sử dụng."))?;
        mark_attached(conn, upload).await?;
        sqlx::query("INSERT INTO message_attachments(id,message_id,upload_id) VALUES($1,$2,$3)")
            .bind(state.id().await?.0)
            .bind(message.0)
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

pub async fn attach_image(
    state: &AppState,
    conn: &mut PgConnection,
    actor: Id,
    channel: Option<Id>,
    upload: Id,
    purpose: &str,
) -> Result<()> {
    let row = sqlx::query("SELECT * FROM file_uploads WHERE id=$1 AND owner_id=$2 AND channel_id IS NOT DISTINCT FROM $3 AND purpose=$4 AND status='STAGED' AND expires_at>clock_timestamp() FOR UPDATE")
        .bind(upload.0).bind(actor.0).bind(channel.map(|id| id.0)).bind(purpose).fetch_optional(&mut *conn).await?.ok_or_else(|| ApiError::conflict("Ảnh không còn khả dụng."))?;
    let verified = verify_object(state, &row).await?;
    mark_attached(conn, verified).await
}

async fn mark_attached(conn: &mut PgConnection, upload: VerifiedUpload) -> Result<()> {
    let changed = sqlx::query("UPDATE file_uploads SET status='ATTACHED',object_version=$2,etag=$3 WHERE id=$1 AND expires_at>clock_timestamp()")
        .bind(upload.id.0)
        .bind(upload.version)
        .bind(upload.etag)
        .execute(conn)
        .await?
        .rows_affected();
    if changed != 1 {
        return Err(ApiError::conflict("Tệp đã hết hạn hoặc đã được sử dụng."));
    }
    Ok(())
}

pub async fn discard_image(conn: &mut PgConnection, id: Id) -> Result<()> {
    sqlx::query("UPDATE file_uploads SET status='DELETE_PENDING' WHERE id=$1")
        .bind(id.0)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn discard_message(conn: &mut PgConnection, message: Id) -> Result<()> {
    sqlx::query("UPDATE file_uploads SET status='DELETE_PENDING' WHERE id IN (SELECT upload_id FROM message_attachments WHERE message_id=$1)").bind(message.0).execute(conn).await?;
    Ok(())
}

pub fn image_url(id: Id) -> String {
    format!("/api/v1/images/{}/url", id.0)
}

#[derive(Serialize, Deserialize)]
struct DownloadClaims {
    sub: String,
    jti: Uuid,
    upload_id: Id,
    exp: i64,
    aud: String,
}

async fn authorize_upload(
    conn: &mut PgConnection,
    actor: Id,
    upload: Id,
) -> Result<sqlx::postgres::PgRow> {
    let row = sqlx::query("SELECT * FROM file_uploads WHERE id=$1 AND status='ATTACHED'")
        .bind(upload.0)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let purpose: String = row.get("purpose");
    match purpose.as_str() {
        "ATTACHMENT" => {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM message_attachments WHERE upload_id=$1)",
            )
            .bind(upload.0)
            .fetch_one(&mut *conn)
            .await?;
            if !exists {
                return Err(ApiError::not_found());
            }
            db::room_access(conn, actor, Id(row.get("room_id"))).await?;
        }
        "AVATAR" => {
            let visible: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users u JOIN user_details d ON d.user_id=u.id WHERE u.id=$1 AND u.is_available AND d.avatar_upload_id=$2)").bind(row.get::<i64,_>("owner_id")).bind(upload.0).fetch_one(&mut *conn).await?;
            if !visible {
                return Err(ApiError::not_found());
            }
        }
        _ => {
            let visible: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM channels c JOIN channel_members m ON c.id=m.channel_id WHERE c.id=$1 AND c.is_available AND m.user_id=$2 AND (c.icon_upload_id=$3 OR c.background_upload_id=$3))")
                .bind(row.get::<i64,_>("channel_id")).bind(actor.0).bind(upload.0).fetch_one(&mut *conn).await?;
            if !visible {
                return Err(ApiError::not_found());
            }
        }
    }
    Ok(row)
}

async fn signed_download(
    state: &AppState,
    actor: &Actor,
    upload: Id,
    expected_image: bool,
) -> Result<Json<serde_json::Value>> {
    let config = state.files.as_ref().ok_or_else(ApiError::unavailable)?;
    let mut conn = state.pool.acquire().await?;
    let row = authorize_upload(&mut conn, actor.user_id, upload).await?;
    let is_image = row.get::<String, _>("purpose") != "ATTACHMENT";
    if expected_image != is_image {
        return Err(ApiError::not_found());
    }
    let expires_at = Utc::now() + chrono::Duration::seconds(120);
    let claims = DownloadClaims {
        sub: actor.user_id.0.to_string(),
        jti: actor.sid,
        upload_id: upload,
        exp: expires_at.timestamp().min(actor.exp),
        aud: "spaceless-files".into(),
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(config.gateway_secret.as_bytes()),
    )
    .map_err(|_| ApiError::internal())?;
    Ok(Json(
        json!({"url":format!("{}/files/{}?token={}",config.gateway_base_url,upload.0,token),"expires_at":DateTime::<Utc>::from_timestamp(claims.exp,0)}),
    ))
}

async fn attachment_url(
    axum::extract::State(state): axum::extract::State<AppState>,
    actor: Actor,
    Path(attachment): Path<Id>,
) -> Result<Json<serde_json::Value>> {
    let upload: i64 = sqlx::query_scalar("SELECT upload_id FROM message_attachments WHERE id=$1")
        .bind(attachment.0)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    signed_download(&state, &actor, Id(upload), false).await
}

async fn image_download_url(
    axum::extract::State(state): axum::extract::State<AppState>,
    actor: Actor,
    Path(upload): Path<Id>,
) -> Result<Json<serde_json::Value>> {
    signed_download(&state, &actor, upload, true).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DownloadQuery {
    token: String,
}

async fn download(
    axum::extract::State(state): axum::extract::State<AppState>,
    actor: Actor,
    Path(upload): Path<Id>,
    Query(query): Query<DownloadQuery>,
    headers: HeaderMap,
) -> Result<Response> {
    let config = state.files.as_ref().ok_or_else(ApiError::unavailable)?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["spaceless-files"]);
    validation.leeway = 0;
    let claims = decode::<DownloadClaims>(
        &query.token,
        &DecodingKey::from_secret(config.gateway_secret.as_bytes()),
        &validation,
    )
    .map_err(|_| ApiError::unauthorized())?
    .claims;
    if claims.upload_id.0 != upload.0
        || claims.jti != actor.sid
        || claims.sub != actor.user_id.0.to_string()
    {
        return Err(ApiError::unauthorized());
    }
    let mut conn = state.pool.acquire().await?;
    let row = authorize_upload(&mut conn, actor.user_id, upload).await?;
    drop(conn);
    let mut request = config
        .s3
        .get_object()
        .bucket(&config.bucket)
        .key(row.get::<String, _>("storage_key"))
        .set_version_id(row.get("object_version"))
        .set_if_match(row.get("etag"));
    if let Some(range) = headers.get(header::RANGE) {
        let range = range
            .to_str()
            .map_err(|_| ApiError::bad("Range không hợp lệ."))?;
        if !range.starts_with("bytes=") || range.contains(',') || range.len() > 100 {
            return Err(ApiError::bad("Range không hợp lệ."));
        }
        request = request.range(range);
    }
    let object = request.send().await.map_err(|_| ApiError::unavailable())?;
    let mut response = Response::builder()
        .status(if object.content_range().is_some() {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, row.get::<String, _>("mime_type"))
        .header(header::CACHE_CONTROL, "private, no-store")
        .header("x-content-type-options", "nosniff")
        .header(header::CONTENT_DISPOSITION, "attachment")
        .header(header::ACCEPT_RANGES, "bytes");
    if let Some(length) = object.content_length() {
        response = response.header(header::CONTENT_LENGTH, length);
    }
    if let Some(range) = object.content_range() {
        response = response.header(header::CONTENT_RANGE, range);
    }
    response
        .body(HttpBody::from_stream(ReaderStream::new(
            object.body.into_async_read(),
        )))
        .map_err(|_| ApiError::internal())
}

pub async fn run_cleanup(state: &AppState) -> Result<()> {
    let Some(config) = state.files.as_ref() else {
        return Ok(());
    };
    loop {
        let mut tx = state.pool.begin().await?;
        let row = sqlx::query(
            "SELECT id, staging_key, storage_key, object_version, status FROM file_uploads
            WHERE put_expires_at <= now() AND (status='DELETE_PENDING'
                OR (status='STAGED' AND expires_at <= now())
                OR (status='ATTACHED' AND staging_key IS NOT NULL))
            ORDER BY id FOR UPDATE SKIP LOCKED LIMIT 1",
        )
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else { return Ok(()) };
        let id: i64 = row.get("id");
        if let Some(key) = row.get::<Option<String>, _>("staging_key") {
            delete_object(config, &key, None).await?;
        }
        if row.get::<String, _>("status") == "ATTACHED" {
            sqlx::query("UPDATE file_uploads SET staging_key=NULL WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        } else {
            let key: String = row.get("storage_key");
            delete_object(config, &key, row.get("object_version")).await?;
            sqlx::query("DELETE FROM file_uploads WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
    }
}

async fn head_object(
    config: &Config,
    key: &str,
) -> Result<Option<aws_sdk_s3::operation::head_object::HeadObjectOutput>> {
    match config
        .s3
        .head_object()
        .bucket(&config.bucket)
        .key(key)
        .send()
        .await
    {
        Ok(object) => Ok(Some(object)),
        Err(error)
            if error
                .as_service_error()
                .is_some_and(|error| error.is_not_found()) =>
        {
            Ok(None)
        }
        Err(_) => Err(ApiError::unavailable()),
    }
}

async fn delete_object(config: &Config, key: &str, version: Option<String>) -> Result<()> {
    let version = match version {
        Some(version) => Some(version),
        None => {
            let Some(object) = head_object(config, key).await? else {
                return Ok(());
            };
            object.version_id().map(str::to_owned)
        }
    };
    config
        .s3
        .delete_object()
        .bucket(&config.bucket)
        .key(key)
        .set_version_id(version)
        .send()
        .await
        .map_err(|_| ApiError::unavailable())?;
    if head_object(config, key).await?.is_some() {
        return Err(ApiError::unavailable());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_type_must_match_bytes() {
        assert!(validate_bytes(b"hello", "image/png").is_err());
        assert!(validate_bytes(b"hello", "text/plain").is_ok());
        assert!(validate_bytes(b"hello\0world", "text/plain").is_err());
        assert!(validate_bytes(b"%PDF-1.7\nbody\n%%EOF", "application/pdf").is_ok());
        assert!(validate_bytes(b"%PDF-1.7 truncated", "application/pdf").is_err());
        assert!(validate_bytes(b"PK\x03\x04", "application/zip").is_err());
    }

    #[test]
    fn image_metadata_cannot_accept_documents_or_oversize() {
        let mut input = UploadInput {
            file_name: "a.pdf".into(),
            mime_type: "application/pdf".into(),
            file_size_bytes: 10,
        };
        assert!(validate_metadata(&input, "AVATAR").is_err());
        assert!(validate_metadata(&input, "ATTACHMENT").is_ok());
        input.mime_type = "image/png".into();
        input.file_size_bytes = 2_000_001;
        assert!(validate_metadata(&input, "CHANNEL_ICON").is_err());
        assert!(validate_metadata(&input, "CHANNEL_BACKGROUND").is_ok());
        input.file_name = "../a.png".into();
        assert!(validate_metadata(&input, "ATTACHMENT").is_err());
    }
}
