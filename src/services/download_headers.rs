//! 下载响应头构造工具。

use actix_web::http::header::{
    Charset, ContentDisposition, DispositionParam, DispositionType, ExtendedValue,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DownloadDisposition {
    Attachment,
    Inline,
}

impl DownloadDisposition {
    pub(crate) fn header_value(self, filename: &str) -> String {
        let safe_filename = filename
            .chars()
            .filter(|ch| !matches!(ch, '\r' | '\n' | '\0'))
            .collect::<String>();
        let disposition = match self {
            Self::Attachment => DispositionType::Attachment,
            Self::Inline => DispositionType::Inline,
        };

        ContentDisposition {
            disposition,
            parameters: vec![DispositionParam::FilenameExt(ExtendedValue {
                charset: Charset::Ext("UTF-8".to_string()),
                language_tag: None,
                value: safe_filename.into_bytes(),
            })],
        }
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::DownloadDisposition;

    #[test]
    fn download_disposition_uses_rfc5987_filename_star() {
        assert_eq!(
            DownloadDisposition::Attachment.header_value("report 1.pdf"),
            "attachment; filename*=UTF-8''report%201.pdf"
        );
        assert_eq!(
            DownloadDisposition::Inline.header_value("报告\"\\;1.pdf"),
            "inline; filename*=UTF-8''%E6%8A%A5%E5%91%8A%22%5C%3B1.pdf"
        );
    }

    #[test]
    fn download_disposition_strips_header_control_characters() {
        assert_eq!(
            DownloadDisposition::Attachment.header_value("a\r\n\0b.txt"),
            "attachment; filename*=UTF-8''ab.txt"
        );
    }
}
