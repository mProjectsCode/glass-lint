use std::{ops::Deref, path::Path};

pub struct TempDir(tempfile::TempDir);

impl TempDir {
    pub fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }

    pub fn path(&self) -> &Path {
        self.0.path()
    }
}

impl Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.0.path()
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        self.0.path()
    }
}
