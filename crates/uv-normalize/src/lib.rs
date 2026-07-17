use std::error::Error;
use std::fmt::{Display, Formatter};

pub use dist_info_name::DistInfoName;
pub use extra_name::{DefaultExtras, ExtraName};
pub use group_name::{DEV_DEPENDENCIES, DefaultGroups, GroupName, PipGroupName};
pub use package_name::PackageName;

use uv_small_str::SmallString;

mod dist_info_name;
mod extra_name;
mod group_name;
mod package_name;

/// Validate and normalize an unowned package or extra name.
pub(crate) fn validate_and_normalize_ref(
    name: impl AsRef<str>,
) -> Result<SmallString, InvalidNameError> {
    let name = name.as_ref();
    match validate_normalization(name)? {
        Normalization::AlreadyNormalized => Ok(SmallString::from(name)),
        Normalization::Required { len } => normalize(name, len),
    }
}

/// Validate and normalize an owned package or extra name.
pub(crate) fn validate_and_normalize_owned(name: String) -> Result<SmallString, InvalidNameError> {
    let Normalization::Required { len } = validate_normalization(&name)? else {
        return Ok(SmallString::from(name));
    };
    let mut bytes = name.into_bytes();
    let mut index = 0;
    let mut previous_separator = true;
    for read in 0..bytes.len() {
        let byte = bytes[read];
        match byte {
            b'A'..=b'Z' => {
                bytes[index] = byte.to_ascii_lowercase();
                index += 1;
                previous_separator = false;
            }
            b'a'..=b'z' | b'0'..=b'9' => {
                bytes[index] = byte;
                index += 1;
                previous_separator = false;
            }
            b'-' | b'_' | b'.' if !previous_separator => {
                bytes[index] = b'-';
                index += 1;
                previous_separator = true;
            }
            b'-' | b'_' | b'.' => {}
            _ => {}
        }
    }
    bytes.truncate(index);
    debug_assert_eq!(index, len);

    let normalized = String::from_utf8(bytes)
        .map_err(|err| InvalidNameError(String::from_utf8_lossy(err.as_bytes()).into_owned()))?;
    Ok(SmallString::from(normalized))
}

/// Normalize an unowned package or extra name.
fn normalize(name: &str, len: usize) -> Result<SmallString, InvalidNameError> {
    let normalized = arcstr::ArcStr::init_with(len, |bytes| {
        write_normalized(name, bytes);
    })
    .map_err(|_| InvalidNameError(name.to_string()))?;

    Ok(SmallString::from(normalized))
}

enum Normalization {
    AlreadyNormalized,
    Required { len: usize },
}

fn validate_normalization(name: &str) -> Result<Normalization, InvalidNameError> {
    // An empty string is not a valid package, extra, or group name.
    if name.is_empty() {
        return Err(InvalidNameError(name.to_string()));
    }

    let mut previous_separator = true;
    for (index, byte) in name.bytes().enumerate() {
        match byte {
            b'A'..=b'Z' => {
                return required_normalized_len(name, index, index, previous_separator);
            }
            b'a'..=b'z' | b'0'..=b'9' => {
                previous_separator = false;
            }
            b'_' | b'.' => {
                if previous_separator {
                    // Names can't start with punctuation.
                    if index == 0 {
                        return Err(InvalidNameError(name.to_string()));
                    }
                }
                return required_normalized_len(name, index, index, previous_separator);
            }
            b'-' => {
                if previous_separator {
                    // Names can't start with punctuation.
                    if index == 0 {
                        return Err(InvalidNameError(name.to_string()));
                    }
                    return required_normalized_len(name, index, index, previous_separator);
                }
                previous_separator = true;
            }
            _ => return Err(InvalidNameError(name.to_string())),
        }
    }

    // Names can't end with punctuation.
    if previous_separator {
        return Err(InvalidNameError(name.to_string()));
    }

    Ok(Normalization::AlreadyNormalized)
}

fn required_normalized_len(
    name: &str,
    start: usize,
    mut len: usize,
    mut previous_separator: bool,
) -> Result<Normalization, InvalidNameError> {
    for byte in name[start..].bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => {
                len += 1;
                previous_separator = false;
            }
            b'-' | b'_' | b'.' => {
                if !previous_separator {
                    len += 1;
                }
                previous_separator = true;
            }
            _ => return Err(InvalidNameError(name.to_string())),
        }
    }

    // Names can't end with punctuation.
    if previous_separator {
        return Err(InvalidNameError(name.to_string()));
    }

    Ok(Normalization::Required { len })
}

fn write_normalized(name: &str, normalized: &mut [u8]) {
    let mut index = 0;
    let mut previous_separator = true;
    for byte in name.bytes() {
        match byte {
            b'A'..=b'Z' => {
                normalized[index] = byte.to_ascii_lowercase();
                index += 1;
                previous_separator = false;
            }
            b'a'..=b'z' | b'0'..=b'9' => {
                normalized[index] = byte;
                index += 1;
                previous_separator = false;
            }
            b'-' | b'_' | b'.' if !previous_separator => {
                normalized[index] = b'-';
                index += 1;
                previous_separator = true;
            }
            b'-' | b'_' | b'.' => {}
            _ => {}
        }
    }
    debug_assert_eq!(index, normalized.len());
}

/// Returns `true` if the name is already normalized.
#[cfg(test)]
fn is_normalized(name: impl AsRef<str>) -> Result<bool, InvalidNameError> {
    Ok(matches!(
        validate_normalization(name.as_ref())?,
        Normalization::AlreadyNormalized
    ))
}

/// Invalid [`PackageName`] or [`ExtraName`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidNameError(String);

impl Display for InvalidNameError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Not a valid package or extra name: \"{}\". Names must start and end with a letter or \
            digit and may only contain -, _, ., and alphanumeric characters.",
            self.0
        )
    }
}

impl Error for InvalidNameError {}

/// Path didn't end with `pyproject.toml`
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidPipGroupPathError(String);

impl Display for InvalidPipGroupPathError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "The `--group` path is required to end in 'pyproject.toml' for compatibility with pip; got: {}",
            self.0,
        )
    }
}
impl Error for InvalidPipGroupPathError {}

/// Possible errors from reading a [`PipGroupName`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvalidPipGroupError {
    Name(InvalidNameError),
    Path(InvalidPipGroupPathError),
}

impl Display for InvalidPipGroupError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name(e) => e.fmt(f),
            Self::Path(e) => e.fmt(f),
        }
    }
}
impl Error for InvalidPipGroupError {}
impl From<InvalidNameError> for InvalidPipGroupError {
    fn from(value: InvalidNameError) -> Self {
        Self::Name(value)
    }
}
impl From<InvalidPipGroupPathError> for InvalidPipGroupError {
    fn from(value: InvalidPipGroupPathError) -> Self {
        Self::Path(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        let inputs = [
            "friendly-bard",
            "Friendly-Bard",
            "FRIENDLY-BARD",
            "friendly.bard",
            "friendly_bard",
            "friendly--bard",
            "friendly-.bard",
            "FrIeNdLy-._.-bArD",
        ];
        for input in inputs {
            assert_eq!(
                validate_and_normalize_ref(input).unwrap().as_ref(),
                "friendly-bard"
            );
        }
    }

    #[test]
    fn check() {
        let inputs = ["friendly-bard", "friendlybard"];
        for input in inputs {
            assert!(is_normalized(input).unwrap(), "{input:?}");
        }

        let inputs = [
            "friendly.bard",
            "friendly.BARD",
            "friendly_bard",
            "friendly--bard",
            "friendly-.bard",
            "FrIeNdLy-._.-bArD",
        ];
        for input in inputs {
            assert!(!is_normalized(input).unwrap(), "{input:?}");
        }
    }

    #[test]
    fn unchanged() {
        // Unchanged
        let unchanged = ["friendly-bard", "1okay", "okay2"];
        for input in unchanged {
            assert_eq!(validate_and_normalize_ref(input).unwrap().as_ref(), input);
            assert!(is_normalized(input).unwrap());
        }
    }

    #[test]
    fn owned() {
        let inputs = [
            "friendly-bard",
            "friendly.bard",
            "friendly.BARD",
            "friendly_bard",
            "friendly--bard",
            "friendly-.bard",
            "FrIeNdLy-._.-bArD",
        ];
        for input in inputs {
            assert_eq!(
                PackageName::from_owned(input.to_string()).unwrap().as_ref(),
                "friendly-bard"
            );
            assert_eq!(
                ExtraName::from_owned(input.to_string()).unwrap().as_ref(),
                "friendly-bard"
            );
        }
    }

    #[test]
    fn failures() {
        let failures = [
            "",
            " starts-with-space",
            "-starts-with-dash",
            "ends-with-dash-",
            "ends-with-space ",
            "includes!invalid-char",
            "space in middle",
            "alpha-α",
        ];
        for input in failures {
            assert!(validate_and_normalize_ref(input).is_err());
            assert!(is_normalized(input).is_err());
            assert!(PackageName::from_owned(input.to_string()).is_err());
            assert!(ExtraName::from_owned(input.to_string()).is_err());
        }
    }
}
