//! Inputs for FFmpeg: local descriptors or a mandatory object-scoped Range proxy.

use std::path::Path;
use std::sync::Arc;

use super::range_input::ObjectRangeInput;
use super::shared::{CliInput, TempDirGuard, run_cli_command_with_timeout};
use aster_drive_model::entities::file_blob;
use aster_drive_storage::StorageDriver;

use crate::errors::{AsterError, MapAsterErr, Result};

// Covers standalone video/audio containers. Reference/playlist demuxers are
// excluded independently of protocols, including manifests disguised as video.
const FORMATS: &str = "mov,matroska,webm,avi,asf,flv,mpeg,mpegts,mpegvideo,ogg,mp3,wav,flac,aac,aiff,ape,mpc,mpc8,wv,ac3,eac3,dts,dtshd,amr,au,rm,nut,ivf,hevc,h264,av1,obu,vc1,mjpeg,gif,png_pipe,jpeg_pipe,webp_pipe,bmp_pipe,tiff_pipe";

pub(crate) enum FfmpegInput {
    File {
        file: std::fs::File,
        _staging: Option<TempDirGuard>,
    },
    Range(ObjectRangeInput),
}

impl FfmpegInput {
    pub(crate) async fn prepare(
        driver: Arc<dyn StorageDriver>,
        blob: &file_blob::Model,
        name: &str,
        mime: &str,
        temp_root: &str,
    ) -> Result<Self> {
        let key = blob
            .storage_path_for_connector()
            .ok_or_else(|| AsterError::validation_error("media object has no stored input"))?;
        if let Some(local) = driver.extensions().local_path {
            return Ok(Self::File {
                file: open_source(&local.resolve_local_path(key)?)?,
                _staging: None,
            });
        }
        if driver.supports_efficient_range() {
            let size = aster_forge_utils::numbers::i64_to_u64(blob.size, "media source size")?;
            return Ok(Self::Range(
                ObjectRangeInput::start(driver, key, size).await?,
            ));
        }
        // Backends without efficient ranges retain their existing bounded disk
        // staging path. Do not repeatedly discard a full stream to emulate seek.
        let root = aster_forge_utils::paths::runtime_temp_dir(temp_root);
        let path =
            std::path::PathBuf::from(root).join(format!("media-input-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&path).await.map_aster_err_ctx(
            "create media input directory",
            AsterError::storage_driver_error,
        )?;
        let staging = TempDirGuard::new(path, "media input staging");
        let prepared = super::cli_input::prepare_cli_source(
            driver.as_ref(),
            key,
            name,
            mime,
            staging.path(),
            blob.size,
        )
        .await?;
        Ok(Self::File {
            file: open_source(Path::new(prepared.input_arg()))?,
            _staging: Some(staging),
        })
    }

    pub(crate) fn run(
        self,
        command: &str,
        before: &[&str],
        after: &[&str],
        error: impl Fn(String) -> AsterError,
    ) -> Result<std::process::Output> {
        let mut args = before.to_vec();
        args.extend_from_slice(&[
            "-format_whitelist",
            FORMATS,
            "-enable_drefs",
            "0",
            "-use_absolute_path",
            "0",
        ]);
        match self {
            Self::File { file, _staging } => {
                args.extend_from_slice(&["-protocol_whitelist", "fd", "-fd", "0", "-i", "fd:"]);
                args.extend_from_slice(after);
                run_cli_command_with_timeout(command, &args, Some(CliInput::File(file)), error)
            }
            Self::Range(source) => {
                args.extend_from_slice(&[
                    "-protocol_whitelist",
                    "http,tcp",
                    "-http_proxy",
                    &source.proxy,
                    "-rw_timeout",
                    "10000000",
                    "-seekable",
                    "1",
                    "-i",
                    &source.url,
                ]);
                args.extend_from_slice(after);
                run_cli_command_with_timeout(
                    command,
                    &args,
                    Some(CliInput::ObjectProxy(source.proxy.clone())),
                    error,
                )
            }
        }
    }
}

pub(crate) fn open_source(path: &Path) -> Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    let file = options
        .open(path)
        .map_aster_err_ctx("open media input", AsterError::storage_driver_error)?;
    if !file
        .metadata()
        .map_aster_err_ctx("stat media input", AsterError::storage_driver_error)?
        .is_file()
    {
        return Err(AsterError::validation_error(
            "media input must be a regular seekable file",
        ));
    }
    Ok(file)
}

#[cfg(test)]
mod tests;
