use smol_str::SmolStr;

/// Shared package-root grammar used by project inputs and rule declarations.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageName(SmolStr);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidPackageName;

impl PackageName {
    pub fn parse(value: &str) -> Result<Self, InvalidPackageName> {
        let value = value.trim();
        if value.is_empty()
            || value.contains('\0')
            || value.contains(char::is_whitespace)
            || value.starts_with('.')
            || value.starts_with('/')
            || value.starts_with('\\')
        {
            return Err(InvalidPackageName);
        }

        if let Some(scoped) = value.strip_prefix('@') {
            let mut segments = scoped.split('/');
            if segments.next().is_none_or(str::is_empty)
                || segments.next().is_none_or(str::is_empty)
                || segments.next().is_some()
            {
                return Err(InvalidPackageName);
            }
        } else if value.contains('/') {
            return Err(InvalidPackageName);
        }

        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> SmolStr {
        self.0
    }
}
