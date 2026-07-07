use actix_web::HttpResponse;
use actix_web::http::{StatusCode, header};

use super::streaming::AbortAwareStream;
use super::types::DownloadOutcome;

// DownloadOutcome 到 HttpResponse 的路由层组装函数，仅在路由/中间件层使用。
//
// 这些函数在 api 层调用，把 DownloadOutcome 组装成 actix_web::HttpResponse。
// 服务层（download.rs）本身不调用它们；它们存放在此处是为了避免跨文件重复。
pub(crate) fn outcome_to_response(outcome: DownloadOutcome) -> HttpResponse {
    match outcome {
        DownloadOutcome::NotModified {
            etag,
            cache_control,
            csp,
        } => {
            let mut response = HttpResponse::NotModified();
            response.insert_header(("ETag", etag));
            response.insert_header(("Cache-Control", cache_control));
            if let Some(csp_value) = csp {
                response.insert_header(("Content-Security-Policy", csp_value));
                response.insert_header(("X-Content-Type-Options", "nosniff"));
            }
            response.finish()
        }
        DownloadOutcome::PresignedRedirect { url } => HttpResponse::Found()
            .insert_header((header::LOCATION, url))
            .insert_header((header::CACHE_CONTROL, "no-store"))
            .finish(),
        DownloadOutcome::Stream(streamed) => {
            let mut response = match streamed.range {
                Some(_) => HttpResponse::build(StatusCode::PARTIAL_CONTENT),
                None => HttpResponse::Ok(),
            };
            response.content_type(streamed.content_type);
            response.insert_header(("Content-Length", streamed.content_length.to_string()));
            response.insert_header(("Content-Disposition", streamed.content_disposition));
            response.insert_header(("ETag", streamed.etag));
            response.insert_header(("Cache-Control", streamed.cache_control));
            response.insert_header(("Accept-Ranges", "bytes"));
            if let Some(range) = streamed.range {
                response.insert_header(("Content-Range", range.content_range_header()));
            }
            if let Some(csp_value) = streamed.csp {
                response.insert_header(("Content-Security-Policy", csp_value));
                response.insert_header(("X-Content-Type-Options", "nosniff"));
            }
            // 跳过全局 Compress 中间件，避免压缩编码器为了攒出更大的压缩块而额外缓存，
            // 让大文件下载从"稳定流式"退化成高内存占用。
            response.insert_header(("Content-Encoding", "identity"));
            match streamed.on_abort {
                Some(hook) => response.streaming(AbortAwareStream {
                    inner: streamed.body,
                    hook: Some(hook),
                }),
                None => response.streaming(streamed.body),
            }
        }
    }
}
