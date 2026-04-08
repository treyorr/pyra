//! Version and filename helpers used by the resolver.

use std::str::FromStr;

use pep440_rs::{Operator, Version, VersionSpecifier, VersionSpecifiers};
use pubgrub::Ranges;

use crate::{ResolverEnvironment, ResolverError, simple::SimpleFile};

pub fn version_from_filename(
    package: &str,
    filename: &str,
) -> Result<Option<Version>, ResolverError> {
    if let Some(version) = wheel_version(filename) {
        return Version::from_str(version)
            .map(Some)
            .map_err(|_| ResolverError::ParseVersion {
                package: package.to_string(),
                value: version.to_string(),
            });
    }

    if let Some(version) = sdist_version(package, filename) {
        return Version::from_str(version)
            .map(Some)
            .map_err(|_| ResolverError::ParseVersion {
                package: package.to_string(),
                value: version.to_string(),
            });
    }

    Ok(None)
}

pub fn requirement_to_range(
    package: &str,
    specifiers: &VersionSpecifiers,
) -> Result<Ranges<Version>, ResolverError> {
    let mut ranges = Ranges::full();
    for specifier in specifiers.iter() {
        let next = specifier_to_range(package, specifier)?;
        ranges = ranges.intersection(&next);
    }
    Ok(ranges)
}

pub fn artifact_compatible(file: &SimpleFile, env: &ResolverEnvironment) -> bool {
    if file.filename.ends_with(".whl") {
        return wheel_compatible(&file.filename, env);
    }
    file.filename.ends_with(".tar.gz") || file.filename.ends_with(".zip")
}

fn specifier_to_range(
    package: &str,
    specifier: &VersionSpecifier,
) -> Result<Ranges<Version>, ResolverError> {
    let version = specifier.version().clone();
    let range = match specifier.operator() {
        Operator::Equal | Operator::ExactEqual => Ranges::singleton(version),
        Operator::NotEqual => Ranges::singleton(version).complement(),
        Operator::GreaterThan => Ranges::strictly_higher_than(version),
        Operator::GreaterThanEqual => Ranges::higher_than(version),
        Operator::LessThan => Ranges::strictly_lower_than(version),
        Operator::LessThanEqual => Ranges::lower_than(version),
        Operator::EqualStar => wildcard_range(specifier.version()).map_err(|specifier| {
            ResolverError::UnsupportedVersionSpecifier {
                package: package.to_string(),
                specifier,
            }
        })?,
        Operator::NotEqualStar => wildcard_range(specifier.version())
            .map_err(|specifier| ResolverError::UnsupportedVersionSpecifier {
                package: package.to_string(),
                specifier,
            })?
            .complement(),
        Operator::TildeEqual => compatible_range(specifier.version()).map_err(|specifier| {
            ResolverError::UnsupportedVersionSpecifier {
                package: package.to_string(),
                specifier,
            }
        })?,
    };
    Ok(range)
}

fn wildcard_range(version: &Version) -> Result<Ranges<Version>, String> {
    let upper =
        next_release_bound(version, version.release().len()).ok_or_else(|| version.to_string())?;
    Ok(Ranges::between(version.clone(), upper))
}

fn compatible_range(version: &Version) -> Result<Ranges<Version>, String> {
    let release_len = version.release().len();
    if release_len < 2 {
        return Err(version.to_string());
    }
    let upper = next_release_bound(version, release_len - 1).ok_or_else(|| version.to_string())?;
    Ok(Ranges::between(version.clone(), upper))
}

fn next_release_bound(version: &Version, prefix_len: usize) -> Option<Version> {
    let release = version.release();
    if prefix_len == 0 || release.len() < prefix_len {
        return None;
    }
    let mut prefix = release[..prefix_len].to_vec();
    let last = prefix.last_mut()?;
    *last += 1;
    let value = prefix
        .into_iter()
        .map(|segment| segment.to_string())
        .collect::<Vec<_>>()
        .join(".");
    Version::from_str(&value).ok()
}

fn wheel_version(filename: &str) -> Option<&str> {
    let name = filename.strip_suffix(".whl")?;
    let parts = name.split('-').collect::<Vec<_>>();
    if parts.len() < 5 {
        return None;
    }
    parts.get(1).copied()
}

fn sdist_version<'a>(package: &str, filename: &'a str) -> Option<&'a str> {
    let stem = filename
        .strip_suffix(".tar.gz")
        .or_else(|| filename.strip_suffix(".zip"))?;
    let candidates = [
        package.to_string(),
        package.replace('-', "_"),
        package.replace('-', "."),
    ];
    candidates.iter().find_map(|prefix| {
        stem.strip_prefix(prefix.as_str())
            .and_then(|rest| rest.strip_prefix('-'))
    })
}

fn wheel_compatible(filename: &str, env: &ResolverEnvironment) -> bool {
    let Some(stem) = filename.strip_suffix(".whl") else {
        return false;
    };
    let parts = stem.split('-').collect::<Vec<_>>();
    if parts.len() < 5 {
        return false;
    }
    let py_tag = parts[parts.len() - 3];
    let abi_tag = parts[parts.len() - 2];
    let platform_tag = parts[parts.len() - 1];

    python_tag_compatible(py_tag, env)
        && abi_tag_compatible(abi_tag, env)
        && platform_tag_compatible(platform_tag, env)
}

fn python_tag_compatible(tag: &str, env: &ResolverEnvironment) -> bool {
    let major = env.python_version.release()[0];
    let minor = *env.python_version.release().get(1).unwrap_or(&0);
    let exact = format!("cp{major}{minor}");

    tag.split('.').any(|candidate| {
        candidate == "py3"
            || candidate == "py2.py3"
            || candidate == "py3-none"
            || candidate == exact
    })
}

fn abi_tag_compatible(tag: &str, env: &ResolverEnvironment) -> bool {
    let major = env.python_version.release()[0];
    let minor = *env.python_version.release().get(1).unwrap_or(&0);
    let exact = format!("cp{major}{minor}");
    tag.split('.')
        .any(|candidate| candidate == "none" || candidate == "abi3" || candidate == exact)
}

fn platform_tag_compatible(tag: &str, env: &ResolverEnvironment) -> bool {
    if tag == "any" {
        return true;
    }
    match env.target_triple.as_str() {
        "aarch64-apple-darwin" => tag.contains("macosx") && tag.contains("arm64"),
        "x86_64-apple-darwin" => tag.contains("macosx") && tag.contains("x86_64"),
        "x86_64-unknown-linux-gnu" => {
            (tag.contains("manylinux") || tag.contains("linux")) && tag.contains("x86_64")
        }
        "aarch64-unknown-linux-gnu" => {
            (tag.contains("manylinux") || tag.contains("linux")) && tag.contains("aarch64")
        }
        _ => false,
    }
}
