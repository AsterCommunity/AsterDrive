//! 归档文件服务：只读扫描和 manifest 生成的共享逻辑。

pub(crate) mod format;
pub(crate) mod io;
pub(crate) mod path;
pub(crate) mod range_reader;
pub(crate) mod scan;
pub(crate) mod seven_zip_scan;
#[cfg(test)]
pub(crate) mod test_utils;
pub(crate) mod zip_scan;
