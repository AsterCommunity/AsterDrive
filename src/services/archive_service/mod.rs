//! 归档文件服务：只读扫描和 manifest 生成的共享逻辑。

pub(crate) mod range_reader;
#[cfg(test)]
pub(crate) mod test_utils;
pub(crate) mod zip_scan;
