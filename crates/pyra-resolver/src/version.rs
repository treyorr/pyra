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

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, str::FromStr};

    use pep440_rs::{Version, VersionSpecifiers};
    use pep508_rs::MarkerEnvironmentBuilder;

    use super::{artifact_compatible, requirement_to_range};
    use crate::{ResolverEnvironment, simple::SimpleFile};

    #[test]
    fn equal_operator_matches_only_the_exact_version() {
        assert_range("==1.2.3", &["1.2.3"], &["1.2.2", "1.2.4"]);
    }

    #[test]
    fn not_equal_operator_excludes_only_the_exact_version() {
        assert_range("!=1.2.3", &["1.2.2", "1.2.4"], &["1.2.3"]);
    }

    #[test]
    fn less_than_operator_excludes_the_boundary() {
        assert_range("<1.2.3", &["1.2.2"], &["1.2.3", "1.2.4"]);
    }

    #[test]
    fn less_than_equal_operator_includes_the_boundary() {
        assert_range("<=1.2.3", &["1.2.2", "1.2.3"], &["1.2.4"]);
    }

    #[test]
    fn greater_than_operator_excludes_the_boundary() {
        assert_range(">1.2.3", &["1.2.4"], &["1.2.2", "1.2.3"]);
    }

    #[test]
    fn greater_than_equal_operator_includes_the_boundary() {
        assert_range(">=1.2.3", &["1.2.3", "1.2.4"], &["1.2.2"]);
    }

    #[test]
    fn wildcard_equal_operator_stops_before_the_next_release_prefix() {
        assert_range("==1.2.*", &["1.2.0", "1.2.9"], &["1.1.9", "1.3.0"]);
    }

    #[test]
    fn wildcard_not_equal_operator_excludes_the_matching_prefix() {
        assert_range("!=1.2.*", &["1.1.9", "1.3.0"], &["1.2.0", "1.2.9"]);
    }

    #[test]
    fn compatible_release_with_two_release_segments_caps_at_the_next_major() {
        assert_range("~=1.2", &["1.2", "1.9.9"], &["1.1.9", "2.0.0"]);
    }

    #[test]
    fn compatible_release_with_three_release_segments_caps_at_the_next_minor() {
        assert_range("~=1.2.3", &["1.2.3", "1.2.9"], &["1.2.2", "1.3.0"]);
    }

    #[test]
    fn py3_none_any_wheels_are_accepted_for_every_supported_target() {
        for triple in supported_target_triples() {
            assert!(
                artifact_compatible(
                    &simple_file("demo-1.0.0-py3-none-any.whl"),
                    &resolver_environment(triple),
                ),
                "expected py3-none-any to match {triple}"
            );
        }
    }

    #[test]
    fn abi3_wheels_follow_the_current_supported_host_matrix() {
        let cases = [
            (
                "aarch64-apple-darwin",
                "demo-1.0.0-cp313-abi3-macosx_11_0_arm64.whl",
                "demo-1.0.0-cp313-abi3-macosx_11_0_x86_64.whl",
            ),
            (
                "x86_64-apple-darwin",
                "demo-1.0.0-cp313-abi3-macosx_11_0_x86_64.whl",
                "demo-1.0.0-cp313-abi3-macosx_11_0_arm64.whl",
            ),
            (
                "x86_64-unknown-linux-gnu",
                "demo-1.0.0-cp313-abi3-manylinux_2_17_x86_64.whl",
                "demo-1.0.0-cp313-abi3-manylinux_2_17_aarch64.whl",
            ),
            (
                "aarch64-unknown-linux-gnu",
                "demo-1.0.0-cp313-abi3-manylinux_2_17_aarch64.whl",
                "demo-1.0.0-cp313-abi3-manylinux_2_17_x86_64.whl",
            ),
        ];

        for (triple, matching, mismatching) in cases {
            let env = resolver_environment(triple);
            assert!(
                artifact_compatible(&simple_file(matching), &env),
                "expected {matching} to match {triple}"
            );
            assert!(
                !artifact_compatible(&simple_file(mismatching), &env),
                "expected {mismatching} to be rejected for {triple}"
            );
        }
    }

    #[test]
    fn exact_python_tags_reject_other_python_versions() {
        let env = resolver_environment("x86_64-unknown-linux-gnu");
        assert!(!artifact_compatible(
            &simple_file("demo-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl"),
            &env,
        ));
    }

    #[test]
    fn source_distributions_remain_installable_candidates() {
        let env = resolver_environment("x86_64-unknown-linux-gnu");
        assert!(artifact_compatible(&simple_file("demo-1.0.0.tar.gz"), &env));
        assert!(artifact_compatible(&simple_file("demo-1.0.0.zip"), &env));
    }

    fn assert_range(specifier: &str, included: &[&str], excluded: &[&str]) {
        let specifiers = VersionSpecifiers::from_str(specifier).expect("valid version specifier");
        let range = requirement_to_range("demo", &specifiers).expect("supported range");

        for version in included {
            assert!(
                range.contains(&parse_version(version)),
                "expected {specifier} to contain {version}"
            );
        }

        for version in excluded {
            assert!(
                !range.contains(&parse_version(version)),
                "expected {specifier} to exclude {version}"
            );
        }
    }

    fn parse_version(version: &str) -> Version {
        Version::from_str(version).expect("valid version")
    }

    fn supported_target_triples() -> [&'static str; 4] {
        [
            "aarch64-apple-darwin",
            "x86_64-apple-darwin",
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
        ]
    }

    fn resolver_environment(target_triple: &str) -> ResolverEnvironment {
        // These tests intentionally lock the current supported host matrix from
        // `docs/resolution-scope.md`. If Pyra expands wheel support later, the
        // docs and this matrix should change together.
        let (platform_machine, platform_system, sys_platform) = match target_triple {
            "aarch64-apple-darwin" => ("arm64", "Darwin", "darwin"),
            "x86_64-apple-darwin" => ("x86_64", "Darwin", "darwin"),
            "x86_64-unknown-linux-gnu" => ("x86_64", "Linux", "linux"),
            "aarch64-unknown-linux-gnu" => ("aarch64", "Linux", "linux"),
            other => panic!("unsupported test target triple: {other}"),
        };

        let python_full_version = "3.13.2";
        let markers = MarkerEnvironmentBuilder {
            implementation_name: "cpython",
            implementation_version: python_full_version,
            os_name: "posix",
            platform_machine,
            platform_python_implementation: "CPython",
            platform_release: "",
            platform_system,
            platform_version: "",
            python_full_version,
            python_version: "3.13",
            sys_platform,
        }
        .try_into()
        .expect("marker environment");

        ResolverEnvironment::new(markers, python_full_version, target_triple)
            .expect("resolver environment")
    }

    fn simple_file(filename: &str) -> SimpleFile {
        SimpleFile {
            filename: filename.to_string(),
            url: "file:///fixture".to_string(),
            hashes: BTreeMap::new(),
            requires_python: None,
            size: None,
            upload_time: None,
            core_metadata: None,
            yanked: None,
        }
    }
}
