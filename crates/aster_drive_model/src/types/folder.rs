//! Supported logical workspace hierarchy depth.

/// The virtual workspace root has depth zero; its immediate folders have depth one.
/// This is the product hierarchy boundary, independent of synchronous operation budgets.
pub const MAX_FOLDER_DEPTH: usize = 4096;
