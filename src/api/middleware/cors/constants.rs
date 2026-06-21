//! CORS 中间件子模块：`constants`。

use std::sync::LazyLock;

pub(super) const ALLOWED_METHODS: &[&str] = &[
    "GET",
    "POST",
    "PUT",
    "PATCH",
    "DELETE",
    "OPTIONS",
    "PROPFIND",
    "PROPPATCH",
    "MKCOL",
    "COPY",
    "MOVE",
    "LOCK",
    "UNLOCK",
];

pub(super) const ALLOWED_HEADERS: &[&str] = &[
    "authorization",
    "accept",
    "content-type",
    "depth",
    "destination",
    "if",
    "lock-token",
    "overwrite",
    "range",
    "timeout",
    "x-csrf-token",
    "x-wopi-lock",
    "x-wopi-oldlock",
    "x-wopi-override",
    "x-wopi-overwriterelativetarget",
    "x-wopi-requestedname",
    "x-wopi-relativetarget",
    "x-wopi-size",
    "x-wopi-suggestedtarget",
];

pub(super) static ALLOWED_HEADERS_VALUE: LazyLock<String> =
    LazyLock::new(|| ALLOWED_HEADERS.join(", "));

pub(super) static ALLOWED_METHODS_VALUE: LazyLock<String> =
    LazyLock::new(|| ALLOWED_METHODS.join(", "));

pub(super) const EXPOSE_HEADERS: &[&str] = &[
    "accept-ranges",
    "content-length",
    "content-range",
    "dav",
    "etag",
    "lock-token",
    "x-wopi-itemversion",
    "x-wopi-invalidfilenameerror",
    "x-wopi-lock",
    "x-wopi-lockfailurereason",
    "x-wopi-validrelativetarget",
];

pub(super) static EXPOSE_HEADERS_VALUE: LazyLock<String> =
    LazyLock::new(|| EXPOSE_HEADERS.join(", "));
