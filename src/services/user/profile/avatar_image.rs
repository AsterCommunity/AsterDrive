//! Streaming avatar upload staging.

use std::path::{Path, PathBuf};

use actix_multipart::Multipart;
use futures::StreamExt;
use tokio::io::{AsyncWriteExt, BufWriter};

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{
    AsterError, MapAsterErr, Result, file_upload_error_with_code, validation_error_with_code,
};

use super::avatar_storage::{
    avatar_staging_dir, avatar_staging_source_path, cleanup_avatar_staging,
};

pub(super) struct StagedAvatarUpload {
    pub submission_id: uuid::Uuid,
    pub file_name: String,
    pub source_path: PathBuf,
    pub source_size: u64,
}

pub(super) async fn stage_avatar_upload(
    payload: &mut Multipart,
    max_upload_size: usize,
    avatar_root: &Path,
) -> Result<StagedAvatarUpload> {
    while let Some(field) = payload.next().await {
        let mut field = field.map_aster_err(|message| {
            file_upload_error_with_code(ApiErrorCode::AvatarUploadReadFailed, message)
        })?;
        let Some(file_name) = field
            .content_disposition()
            .and_then(|cd| cd.get_filename())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
        else {
            while let Some(chunk) = field.next().await {
                chunk.map_aster_err(|message| {
                    file_upload_error_with_code(ApiErrorCode::AvatarUploadReadFailed, message)
                })?;
            }
            continue;
        };

        let submission_id = uuid::Uuid::new_v4();
        let staging_dir = avatar_staging_dir(avatar_root, submission_id);
        let source_path = avatar_staging_source_path(avatar_root, submission_id);
        let partial_path = staging_dir.join("source.part");
        tokio::fs::create_dir_all(&staging_dir)
            .await
            .map_aster_err_ctx(
                "create avatar staging directory",
                AsterError::storage_driver_error,
            )?;

        let result = async {
            let file = tokio::fs::File::create(&partial_path)
                .await
                .map_aster_err_ctx(
                    "create avatar staging source",
                    AsterError::storage_driver_error,
                )?;
            let mut writer = BufWriter::new(file);
            let mut source_size = 0usize;
            while let Some(chunk) = field.next().await {
                let chunk = chunk.map_aster_err(|message| {
                    file_upload_error_with_code(ApiErrorCode::AvatarUploadReadFailed, message)
                })?;
                source_size = source_size
                    .checked_add(chunk.len())
                    .ok_or_else(|| AsterError::file_too_large("avatar upload size overflow"))?;
                if source_size > max_upload_size {
                    return Err(AsterError::file_too_large(format!(
                        "avatar upload exceeds {max_upload_size} bytes"
                    )));
                }
                writer.write_all(&chunk).await.map_aster_err_ctx(
                    "write avatar staging source",
                    AsterError::storage_driver_error,
                )?;
            }
            if source_size == 0 {
                return Err(validation_error_with_code(
                    ApiErrorCode::AvatarFileRequired,
                    "avatar file is required",
                ));
            }
            writer.flush().await.map_aster_err_ctx(
                "flush avatar staging source",
                AsterError::storage_driver_error,
            )?;
            drop(writer);
            tokio::fs::rename(&partial_path, &source_path)
                .await
                .map_aster_err_ctx(
                    "publish avatar staging source",
                    AsterError::storage_driver_error,
                )?;
            Ok(StagedAvatarUpload {
                submission_id,
                file_name,
                source_path,
                source_size: u64::try_from(source_size)
                    .map_err(|_| AsterError::file_too_large("avatar upload size exceeds u64"))?,
            })
        }
        .await;
        if result.is_err() {
            cleanup_avatar_staging(avatar_root, submission_id).await;
        }
        return result;
    }

    Err(validation_error_with_code(
        ApiErrorCode::AvatarFileRequired,
        "avatar file is required",
    ))
}

#[cfg(test)]
mod tests {
    use actix_multipart::Multipart;
    use actix_web::error::PayloadError;
    use actix_web::http::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
    use actix_web::web::Bytes;
    use futures::stream;

    use super::stage_avatar_upload;
    use crate::services::user::profile::avatar_storage::{
        avatar_staging_dir, avatar_staging_source_path,
    };

    fn multipart_headers(boundary: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
                .expect("multipart content type should be valid"),
        );
        headers
    }

    fn multipart_body(boundary: &str, disposition: &str, bytes: &[u8]) -> Vec<u8> {
        let mut body = format!(
            "--{boundary}\r\nContent-Disposition: {disposition}\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        body
    }

    fn chunked_multipart(headers: &HeaderMap, body: &[u8], chunk_size: usize) -> Multipart {
        let chunks = body
            .chunks(chunk_size)
            .map(|chunk| Ok::<_, PayloadError>(Bytes::copy_from_slice(chunk)))
            .collect::<Vec<_>>();
        Multipart::new(headers, stream::iter(chunks))
    }

    fn test_root(fixture: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "asterdrive-avatar-stage-{fixture}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    fn assert_staging_empty(root: &std::path::Path) {
        let staging = root.join("staging");
        assert!(
            !staging.exists()
                || std::fs::read_dir(staging)
                    .expect("staging root should be readable")
                    .next()
                    .is_none()
        );
    }

    #[tokio::test]
    async fn stages_chunked_upload_at_exact_limit_without_partial_file() {
        let boundary = "aster-avatar-exact";
        let bytes = b"0123456789abcdef";
        let body = multipart_body(
            boundary,
            "form-data; name=\"file\"; filename=\"avatar.png\"",
            bytes,
        );
        let headers = multipart_headers(boundary);
        let mut multipart = chunked_multipart(&headers, &body, 3);
        let root = test_root("exact");

        let staged = stage_avatar_upload(&mut multipart, bytes.len(), &root)
            .await
            .expect("exact upload limit should be accepted");

        assert_eq!(staged.file_name, "avatar.png");
        assert_eq!(staged.source_size, bytes.len() as u64);
        assert_eq!(
            tokio::fs::read(avatar_staging_source_path(&root, staged.submission_id))
                .await
                .expect("staged source should be readable"),
            bytes
        );
        assert!(
            !avatar_staging_dir(&root, staged.submission_id)
                .join("source.part")
                .exists()
        );
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn rejects_one_byte_over_limit_and_cleans_partial_staging() {
        let boundary = "aster-avatar-over";
        let bytes = b"123456789";
        let body = multipart_body(
            boundary,
            "form-data; name=\"file\"; filename=\"avatar.png\"",
            bytes,
        );
        let headers = multipart_headers(boundary);
        let mut multipart = chunked_multipart(&headers, &body, 2);
        let root = test_root("over");

        assert!(
            stage_avatar_upload(&mut multipart, bytes.len() - 1, &root)
                .await
                .is_err()
        );
        assert_staging_empty(&root);
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn rejects_empty_or_missing_filename_without_staging_leaks() {
        for (fixture, disposition) in [
            ("empty", "form-data; name=\"file\"; filename=\"avatar.png\""),
            ("no-filename", "form-data; name=\"file\""),
        ] {
            let boundary = format!("aster-avatar-{fixture}");
            let body = multipart_body(&boundary, disposition, b"");
            let headers = multipart_headers(&boundary);
            let mut multipart = chunked_multipart(&headers, &body, 5);
            let root = test_root(fixture);

            assert!(stage_avatar_upload(&mut multipart, 8, &root).await.is_err());
            assert_staging_empty(&root);
            let _ = tokio::fs::remove_dir_all(root).await;
        }
    }

    #[tokio::test]
    async fn stream_error_cleans_partial_staging() {
        let boundary = "aster-avatar-read-error";
        let prefix = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"avatar.png\"\r\nContent-Type: image/png\r\n\r\npartial"
        );
        let headers = multipart_headers(boundary);
        let stream = stream::iter(vec![
            Ok::<_, PayloadError>(Bytes::from(prefix)),
            Err(PayloadError::Io(std::io::Error::other(
                "fixture read failure",
            ))),
        ]);
        let mut multipart = Multipart::new(&headers, stream);
        let root = test_root("read-error");

        assert!(
            stage_avatar_upload(&mut multipart, 1024, &root)
                .await
                .is_err()
        );
        assert_staging_empty(&root);
        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
