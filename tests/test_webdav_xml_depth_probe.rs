//! WebDAV XML 深度探针。
//!
//! 这是一个故意放在 ignored 下的崩溃复现测试：
//! - 父测试进程拉起同一个测试二进制里的子测试；
//! - 子测试按 `xmltree::Element::parse` 直接调用，证明底层递归风险仍可复现。
//! - 如果子进程因栈溢出中止，父测试把它视为“风险已复现”。

use std::io::Cursor;
use std::process::Command;

const PROBE_TEST_NAME: &str = "webdav_xml_deep_nesting_child_probe";
const PROBE_ENV: &str = "ASTER_WEBDAV_XML_DEPTH_CRASH_PROBE_CHILD";
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
#[ignore = "crash probe: spawns a child process that currently stack-overflows in xmltree; run with -- --ignored"]
fn webdav_xml_deep_nesting_crashes_parser_process() {
    let executable = std::env::current_exe().expect("current test executable path");
    let output = Command::new(executable)
        .args(["--exact", PROBE_TEST_NAME, "--ignored", "--nocapture"])
        .env(PROBE_ENV, "1")
        .output()
        .expect("spawn child probe");

    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!("child status: {}", output.status);
    eprintln!("child stderr:\n{stderr}");

    assert!(
        !output.status.success(),
        "child parser process unexpectedly survived deep WebDAV XML"
    );
    assert!(
        stderr.contains("overflowed its stack") || stderr.contains("stack overflow"),
        "child process failed, but stderr did not show a stack overflow"
    );
}

#[test]
#[ignore = "helper test executed only by webdav_xml_deep_nesting_crashes_parser_process"]
fn webdav_xml_deep_nesting_child_probe() {
    if std::env::var(PROBE_ENV).as_deref() != Ok("1") {
        eprintln!("skipping crash probe child without parent-provided gate");
        return;
    }

    let body = nested_propfind(XML_DEPTH);
    assert!(
        body.len() < XML_BODY_LIMIT,
        "probe body must stay below configured WebDAV XML limit"
    );
    let root = xmltree::Element::parse(Cursor::new(body))
        .expect("well-formed deep WebDAV XML should parse structurally");
    assert_eq!(root.name, "propfind");
}
