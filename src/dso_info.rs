/// The file path and the build ID of a DSO.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DsoInfo {
    /// The file path. Can be an absolute path or a special string
    /// of various forms, e.g. `[vdso]`.
    pub path: Vec<u8>,
    /// The build ID.
    pub build_id: Vec<u8>,
}
