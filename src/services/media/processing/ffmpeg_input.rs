//! FFmpeg may seek within its supplied object, but cannot open any other source.

use std::path::Path;

use crate::errors::{AsterError, MapAsterErr, Result};

// Covers standalone video/audio containers. Reference/playlist demuxers are
// excluded independently of protocols, including manifests disguised as video.
pub(crate) const INPUT_OPTIONS: &[&str] = &[
    "-protocol_whitelist",
    "fd",
    "-format_whitelist",
    "mov,matroska,webm,avi,asf,flv,mpeg,mpegts,mpegvideo,ogg,mp3,wav,flac,aac,aiff,ape,mpc,mpc8,wv,ac3,eac3,dts,dtshd,amr,au,rm,nut,ivf,hevc,h264,av1,obu,vc1,mjpeg,gif,png_pipe,jpeg_pipe,webp_pipe,bmp_pipe,tiff_pipe",
    "-fd",
    "0",
    "-i",
    "fd:",
];

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
mod tests {
    use super::*;
    use crate::services::media::processing::shared::run_cli_command_with_timeout;

    #[test]
    fn seekable_video_works_and_reference_manifests_cannot_open_secondary_sources() {
        let ffmpeg = "ffmpeg";
        let ffprobe = "ffprobe";
        if !crate::config::media_processing::command_is_available(ffmpeg)
            || !crate::config::media_processing::command_is_available(ffprobe)
        {
            eprintln!("FFmpeg/ffprobe unavailable; real media confinement test skipped");
            return;
        }
        let root =
            std::env::temp_dir().join(format!("aster-ffmpeg-input-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let guard = aster_forge_utils::raii::TempDirGuard::new(root, "ffmpeg confinement test");
        let video = guard.path().join("tail-metadata.mp4");
        let status = std::process::Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=size=16x16:rate=1",
                "-t",
                "1",
                "-c:v",
                "mpeg4",
                "-threads",
                "1",
                "-y",
            ])
            .arg(&video)
            .status()
            .unwrap();
        assert!(status.success());
        // Default MP4 places moov after mdat, requiring a seekable input.
        let bytes = std::fs::read(&video).unwrap();
        let find = |tag: &[u8]| bytes.windows(tag.len()).position(|v| v == tag).unwrap();
        assert!(find(b"moov") > find(b"mdat"));
        for command in [ffmpeg, ffprobe] {
            let mut args = vec!["-v", "error"];
            args.extend_from_slice(INPUT_OPTIONS);
            match command {
                "ffmpeg" => args.extend_from_slice(&["-frames:v", "1", "-f", "null", "-"]),
                _ => args.extend_from_slice(&["-show_streams", "-of", "json"]),
            }
            let output = run_cli_command_with_timeout(
                command,
                &args,
                Some(open_source(&video).unwrap()),
                AsterError::validation_error,
            )
            .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/private-resource", listener.local_addr().unwrap());
        let canary = guard.path().join("private.mp4");
        std::fs::copy(&video, &canary).unwrap();
        let payloads = [
            format!("#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\n{url}\n#EXT-X-ENDLIST\n"),
            format!("ffconcat version 1.0\nfile '{}'\n", canary.display()),
            format!("ffconcat version 1.0\nfile '{url}'\n"),
            format!(
                "<?xml version=\"1.0\"?><MPD xmlns=\"urn:mpeg:dash:schema:mpd:2011\"><Period><AdaptationSet><Representation><BaseURL>{url}</BaseURL></Representation></AdaptationSet></Period></MPD>"
            ),
        ];
        for payload in payloads {
            std::fs::write(&video, payload).unwrap();
            for command in [ffmpeg, ffprobe] {
                let mut args = vec!["-v", "error"];
                args.extend_from_slice(INPUT_OPTIONS);
                match command {
                    "ffmpeg" => args.extend_from_slice(&["-frames:v", "1", "-f", "null", "-"]),
                    _ => args.extend_from_slice(&["-show_streams", "-of", "json"]),
                }
                let output = run_cli_command_with_timeout(
                    command,
                    &args,
                    Some(open_source(&video).unwrap()),
                    AsterError::validation_error,
                )
                .unwrap();
                assert!(!output.status.success(), "reference manifest was accepted");
                assert_eq!(
                    listener.accept().unwrap_err().kind(),
                    std::io::ErrorKind::WouldBlock
                );
            }
        }
    }
}
