use tokio_util::io::ReaderStream;

use super::range::ResolvedDownloadRange;

/// 服务层文件流下载数据。路由层负责把这些字段组装成 HttpResponse。
pub struct StreamedFile {
    pub content_type: String,
    pub content_length: i64,
    pub content_disposition: String,
    pub etag: String,
    pub cache_control: &'static str,
    /// 仅 inline 且需要沙盒隔离时不为 None。
    pub csp: Option<&'static str>,
    /// HTTP Range 响应元数据。None 表示完整文件 200 响应。
    pub(crate) range: Option<ResolvedDownloadRange>,
    pub body: ReaderStream<Box<dyn tokio::io::AsyncRead + Unpin + Send>>,
    /// 流未走到 EOF 就被 drop 时触发的 hook（客户端中途断连、actix 丢弃 response 等）。
    /// 分享下载用它在断连时回滚 `download_count`，避免"发起一次就计一次、哪怕没发完"的
    /// 虚增和提前触碰 `max_downloads`。
    pub on_abort: Option<Box<dyn FnOnce() + Send + 'static>>,
}

/// 服务层下载结果。路由层根据变体组装 HttpResponse，服务层不接触 actix_web::HttpResponse。
pub enum DownloadOutcome {
    /// 200 流式响应。
    Stream(StreamedFile),
    /// 304 Not Modified：客户端缓存命中。
    NotModified {
        etag: String,
        cache_control: &'static str,
        csp: Option<&'static str>,
    },
    /// 302 redirect to a provider-issued temporary download URL.
    PresignedRedirect { url: String },
}

impl DownloadOutcome {
    pub fn metrics_outcome(&self) -> &'static str {
        match self {
            Self::Stream(stream) => {
                if stream.range.is_some() {
                    "partial"
                } else {
                    "stream"
                }
            }
            Self::NotModified { .. } => "not_modified",
            Self::PresignedRedirect { .. } => "presigned_redirect",
        }
    }

    pub fn has_range(&self) -> bool {
        matches!(self, Self::Stream(stream) if stream.range.is_some())
    }
}

impl std::fmt::Debug for StreamedFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamedFile")
            .field("content_type", &self.content_type)
            .field("content_length", &self.content_length)
            .field("content_disposition", &self.content_disposition)
            .field("etag", &self.etag)
            .field("cache_control", &self.cache_control)
            .field("csp", &self.csp)
            .field("range", &self.range)
            .field("body", &"<stream>")
            .finish()
    }
}

impl std::fmt::Debug for DownloadOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stream(stream) => f.debug_tuple("Stream").field(stream).finish(),
            Self::NotModified {
                etag,
                cache_control,
                csp,
            } => f
                .debug_struct("NotModified")
                .field("etag", etag)
                .field("cache_control", cache_control)
                .field("csp", csp)
                .finish(),
            Self::PresignedRedirect { url } => f
                .debug_struct("PresignedRedirect")
                .field("url", url)
                .finish(),
        }
    }
}
