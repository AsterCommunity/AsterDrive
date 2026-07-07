//! 用户资料服务子模块：`avatar_image`。

use actix_multipart::Multipart;
use futures::StreamExt;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{
    AsterError, MapAsterErr, Result, file_upload_error_with_code, validation_error_with_code,
};

pub(super) struct AvatarUploadData {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

pub(super) async fn read_avatar_upload(
    payload: &mut Multipart,
    max_upload_size: usize,
) -> Result<AvatarUploadData> {
    let mut bytes = Vec::new();
    let mut saw_file = false;
    let mut file_name = None;

    while let Some(field) = payload.next().await {
        let mut field = field.map_aster_err(|message| {
            file_upload_error_with_code(ApiErrorCode::AvatarUploadReadFailed, message)
        })?;
        let Some(current_file_name) = field
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

        saw_file = true;
        file_name = Some(current_file_name);
        while let Some(chunk) = field.next().await {
            let chunk = chunk.map_aster_err(|message| {
                file_upload_error_with_code(ApiErrorCode::AvatarUploadReadFailed, message)
            })?;
            if bytes.len() + chunk.len() > max_upload_size {
                return Err(AsterError::file_too_large(format!(
                    "avatar upload exceeds {} bytes",
                    max_upload_size
                )));
            }
            bytes.extend_from_slice(&chunk);
        }
        break;
    }

    if !saw_file || bytes.is_empty() {
        return Err(validation_error_with_code(
            ApiErrorCode::AvatarFileRequired,
            "avatar file is required",
        ));
    }

    Ok(AvatarUploadData {
        file_name: file_name.unwrap_or_else(|| "avatar".to_string()),
        bytes,
    })
}
