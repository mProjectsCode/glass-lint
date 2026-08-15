#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum BundleProfile {
    Web,
    Obsidian,
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum BundleTransformer {
    Vite,
    Esbuild,
}

impl BundleTransformer {
    pub const fn all() -> [Self; 2] {
        [Self::Vite, Self::Esbuild]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vite => "vite",
            Self::Esbuild => "esbuild",
        }
    }
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum BundleTarget {
    #[serde(rename = "ES5")]
    Es5,
    #[serde(rename = "ES6")]
    Es6,
    #[serde(rename = "ES2017")]
    Es2017,
    #[serde(rename = "ES2022")]
    Es2022,
    #[serde(rename = "ESNEXT")]
    Esnext,
}

impl BundleTarget {
    pub const fn all() -> [Self; 5] {
        [
            Self::Es5,
            Self::Es6,
            Self::Es2017,
            Self::Es2022,
            Self::Esnext,
        ]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Es5 => "ES5",
            Self::Es6 => "ES6",
            Self::Es2017 => "ES2017",
            Self::Es2022 => "ES2022",
            Self::Esnext => "ESNEXT",
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize)]
pub struct BundleKey {
    pub profile: BundleProfile,
    pub transformer: BundleTransformer,
    pub minified: bool,
    pub target: BundleTarget,
}

impl BundleKey {
    pub fn label(&self) -> String {
        format!(
            "{}/{}/minified={}/target={}",
            self.profile.as_str(),
            self.transformer.as_str(),
            self.minified,
            self.target.as_str()
        )
    }
}

impl BundleProfile {
    pub fn parse(value: &str) -> Result<Self, BundleProfileError> {
        match value {
            "web" => Ok(Self::Web),
            "obsidian" => Ok(Self::Obsidian),
            _ => Err(BundleProfileError::Unknown(value.to_owned())),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Obsidian => "obsidian",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BundleProfileError {
    Empty,
    Unknown(String),
    Duplicate(BundleProfile),
}

impl std::fmt::Display for BundleProfileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("@bundle must specify at least one profile"),
            Self::Unknown(profile) => write!(formatter, "unknown bundle profile `{profile}`"),
            Self::Duplicate(profile) => {
                write!(formatter, "duplicate bundle profile `{}`", profile.as_str())
            }
        }
    }
}

impl std::error::Error for BundleProfileError {}

pub fn normalize_bundle_profiles(
    values: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<Vec<BundleProfile>, BundleProfileError> {
    let mut profiles = Vec::new();
    for value in values {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(BundleProfileError::Empty);
        }
        let profile = BundleProfile::parse(value)?;
        if profiles.contains(&profile) {
            return Err(BundleProfileError::Duplicate(profile));
        }
        profiles.push(profile);
    }
    profiles.sort_unstable();
    Ok(profiles)
}
