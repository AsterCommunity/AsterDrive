//! WebDAV XML nesting-depth regression coverage.

use std::io::Cursor;

use aster_forge_xml::{XmlElement as WebDavXmlFragment, XmlTreeError as WebDavXmlError};
use aster_forge_xml::{XmlSafetyError, XmlSafetyPolicy, validate_xml_input};

const XML_DEPTH: usize = 30_000;
const XML_BODY_LIMIT: usize = 1_048_576;

fn nested_propfind(depth: usize) -> Vec<u8> {
    let mut body = String::with_capacity(64 + depth * 7);
    body.push_str(r#"<D:propfind xmlns:D="DAV:"><D:prop>"#);
    for _ in 0..depth {
        body.push_str("<x>");
    }
    for _ in 0..depth {
        body.push_str("</x>");
    }
    body.push_str("</D:prop></D:propfind>");
    body.into_bytes()
}

#[test]
fn webdav_xml_deep_nesting_is_rejected_before_fragment_construction() {
    let body = nested_propfind(XML_DEPTH);
    assert!(
        body.len() < XML_BODY_LIMIT,
        "deep XML regression fixture must remain below the WebDAV body limit"
    );
    assert_eq!(
        validate_xml_input(&body, XmlSafetyPolicy::untrusted()),
        Err(XmlSafetyError::TooDeep)
    );
    assert_eq!(
        WebDavXmlFragment::parse(Cursor::new(body)),
        Err(WebDavXmlError::TooDeep)
    );
}
