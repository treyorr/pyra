//! In-memory PubGrub graph assembly.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt::Display;
use std::str::FromStr;

use pep440_rs::Version;
use pep508_rs::{ExtraName, Requirement, VerbatimUrl, VersionOrUrl};
use pubgrub::{
    DefaultStringReporter, DerivationTree, External, Map, OfflineDependencyProvider, PubGrubError,
    Ranges, Reporter, resolve,
};

use crate::{
    ResolutionRequest, ResolutionRootToken, ResolutionRootTokenKind, ResolvedPackage,
    ResolverConflict, ResolverEnvironment, ResolverError,
    simple::{SimpleCandidate, fetch_candidates},
    version::requirement_to_range,
};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum PackageKey {
    Root,
    Token(ResolutionRootToken),
    Base(String),
    Variant { name: String, extras: Vec<String> },
}

impl std::fmt::Display for PackageKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Root => write!(f, "__pyra_root__"),
            Self::Token(token) => write!(f, "{}:{}", token_kind_name(&token.kind), token.name),
            Self::Base(name) => write!(f, "{name}"),
            Self::Variant { name, extras } => write!(f, "{}[{}]", name, extras.join(",")),
        }
    }
}

pub async fn resolve_request(
    client: &reqwest::Client,
    request: ResolutionRequest,
) -> Result<Vec<ResolvedPackage>, ResolverError> {
    let catalog = build_catalog(client, &request).await?;
    let provider = build_provider(&request, &catalog)?;
    let root_version = Version::from_str("0").expect("synthetic root version");
    let solution = resolve(&provider, PackageKey::Root, root_version).map_err(resolve_error)?;
    collapse_solution(&request, &catalog, solution)
}

fn resolve_error(
    error: PubGrubError<OfflineDependencyProvider<PackageKey, Ranges<Version>>>,
) -> ResolverError {
    match error {
        PubGrubError::NoSolution(mut tree) => {
            // Pyra's fixture corpus uses a complete local index view, so merging
            // away no-version leaves keeps the user-facing explanation focused on
            // the incompatible constraints that actually matter.
            tree.collapse_no_versions();
            let report = DefaultStringReporter::report(&tree);
            let summary = summarize_conflict(&tree).unwrap_or_else(|| {
                report
                    .lines()
                    .next()
                    .unwrap_or("the selected requirements are incompatible")
                    .to_string()
            });
            ResolverError::Solve {
                detail: report.clone(),
                conflict: Some(ResolverConflict { summary, report }),
            }
        }
        other => ResolverError::Solve {
            detail: other.to_string(),
            conflict: None,
        },
    }
}

fn summarize_conflict(
    tree: &DerivationTree<PackageKey, Ranges<Version>, String>,
) -> Option<String> {
    let mut edges = Vec::new();
    collect_conflict_edges(tree, &mut edges);

    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for edge in edges {
        let key = (
            edge.depender.clone(),
            edge.dependency.clone(),
            edge.requirement.clone(),
        );
        if seen.insert(key) {
            deduped.push(edge);
        }
    }

    for dependency in deduped.iter().map(|edge| edge.dependency.clone()) {
        let mut matching = deduped
            .iter()
            .filter(|edge| edge.dependency == dependency)
            .collect::<Vec<_>>();
        if matching.len() >= 2 {
            matching.sort_by(|left, right| {
                left.depender
                    .cmp(&right.depender)
                    .then(left.requirement.cmp(&right.requirement))
            });
            return Some(format!(
                "{} requires {} {}, but {} requires {} {}.",
                matching[0].depender,
                matching[0].dependency,
                matching[0].requirement,
                matching[1].depender,
                matching[1].dependency,
                matching[1].requirement,
            ));
        }
    }

    deduped.first().map(|edge| {
        format!(
            "{} requires {} {}, but that requirement could not be satisfied.",
            edge.depender, edge.dependency, edge.requirement
        )
    })
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct ConflictEdge {
    depender: String,
    dependency: String,
    requirement: String,
}

fn collect_conflict_edges(
    tree: &DerivationTree<PackageKey, Ranges<Version>, String>,
    edges: &mut Vec<ConflictEdge>,
) {
    match tree {
        DerivationTree::External(External::FromDependencyOf(
            depender,
            _depender_versions,
            dependency,
            requirement,
        )) => {
            let Some(depender) = conflict_package_label(depender) else {
                return;
            };
            let Some(dependency) = conflict_package_label(dependency) else {
                return;
            };
            edges.push(ConflictEdge {
                depender,
                dependency,
                requirement: format_range(requirement),
            });
        }
        DerivationTree::Derived(derived) => {
            collect_conflict_edges(&derived.cause1, edges);
            collect_conflict_edges(&derived.cause2, edges);
        }
        DerivationTree::External(_) => {}
    }
}

fn conflict_package_label(package: &PackageKey) -> Option<String> {
    match package {
        PackageKey::Base(name) => Some(format!("`{name}`")),
        PackageKey::Variant { name, extras } => Some(format!("`{}[{}]`", name, extras.join(","))),
        PackageKey::Token(token) => Some(match token.kind {
            ResolutionRootTokenKind::DependencyGroup => {
                format!("dependency group `{}`", token.name)
            }
            ResolutionRootTokenKind::Extra => format!("extra `{}`", token.name),
        }),
        PackageKey::Root => None,
    }
}

fn format_range(range: &impl Display) -> String {
    format!("`{range}`")
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct CatalogRequest {
    // Catalog traversal has to remember requested dependency extras so we can
    // discover extra-activated edges without eagerly chasing every marker branch.
    package: String,
    extras: Vec<String>,
}

async fn build_catalog(
    client: &reqwest::Client,
    request: &ResolutionRequest,
) -> Result<HashMap<String, Vec<SimpleCandidate>>, ResolverError> {
    let mut queue = VecDeque::new();
    let mut seen = HashSet::new();
    for root in &request.roots {
        for requirement in &root.requirements {
            let requirement = Requirement::<VerbatimUrl>::from_str(requirement).map_err(|_| {
                ResolverError::ParseRequirement {
                    package: root.token.name.clone(),
                    value: requirement.clone(),
                }
            })?;
            queue.push_back(catalog_request(&requirement));
        }
    }

    let mut catalog = HashMap::new();
    while let Some(next) = queue.pop_front() {
        if !seen.insert(next.clone()) {
            continue;
        }

        if !catalog.contains_key(&next.package) {
            let candidates = fetch_candidates(
                client,
                &request.index_url,
                &next.package,
                &request.environment,
            )
            .await?;
            catalog.insert(next.package.clone(), candidates);
        }

        let candidates = catalog
            .get(&next.package)
            .expect("catalog entry inserted before traversal");
        for candidate in candidates {
            for dependency in active_catalog_requests(candidate, &request.environment, &next.extras)
            {
                queue.push_back(dependency);
            }
        }
    }
    Ok(catalog)
}

fn build_provider(
    request: &ResolutionRequest,
    catalog: &HashMap<String, Vec<SimpleCandidate>>,
) -> Result<OfflineDependencyProvider<PackageKey, Ranges<Version>>, ResolverError> {
    let mut provider = OfflineDependencyProvider::<PackageKey, Ranges<Version>>::new();
    let synthetic = Version::from_str("0").expect("synthetic version");

    let mut root_dependencies = Vec::new();
    for root in &request.roots {
        let token_key = PackageKey::Token(root.token.clone());
        root_dependencies.push((token_key.clone(), Ranges::singleton(synthetic.clone())));

        let mut token_dependencies = BTreeMap::new();
        for requirement_text in &root.requirements {
            let requirement =
                Requirement::<VerbatimUrl>::from_str(requirement_text).map_err(|_| {
                    ResolverError::ParseRequirement {
                        package: root.token.name.clone(),
                        value: requirement_text.clone(),
                    }
                })?;
            let dependency_key = dependency_key(&requirement);
            let dependency_range = requirement_range(&requirement)?;
            merge_dependency(&mut token_dependencies, dependency_key, dependency_range);
        }
        provider.add_dependencies(token_key, synthetic.clone(), token_dependencies);
    }
    provider.add_dependencies(PackageKey::Root, synthetic.clone(), root_dependencies);

    for (package, candidates) in catalog {
        for candidate in candidates {
            provider.add_dependencies(
                PackageKey::Base(package.clone()),
                candidate.version.clone(),
                dependency_constraints(candidate, &request.environment, &[])?,
            );
            let extras = collect_variant_extras(candidates);
            for variant in extras {
                let variant_names = variant.iter().cloned().collect::<Vec<_>>();
                provider.add_dependencies(
                    PackageKey::Variant {
                        name: package.clone(),
                        extras: variant_names.clone(),
                    },
                    candidate.version.clone(),
                    variant_constraints(package, candidate, &request.environment, &variant_names)?,
                );
            }
        }
    }

    Ok(provider)
}

fn collect_variant_extras(candidates: &[SimpleCandidate]) -> BTreeSet<BTreeSet<String>> {
    let mut variants = BTreeSet::new();
    for candidate in candidates {
        for dependency in &candidate.dependencies {
            for extra in &dependency.extras {
                variants.insert(BTreeSet::from([extra.to_string()]));
            }
        }
    }
    variants
}

fn dependency_constraints(
    candidate: &SimpleCandidate,
    env: &ResolverEnvironment,
    extras: &[String],
) -> Result<BTreeMap<PackageKey, Ranges<Version>>, ResolverError> {
    let active_extras = active_extra_names(extras);

    let mut dependencies = BTreeMap::new();
    for requirement in &candidate.dependencies {
        if !requirement.evaluate_markers(&env.markers, &active_extras) {
            continue;
        }
        let dependency_key = dependency_key(requirement);
        let range = requirement_range(requirement)?;
        merge_dependency(&mut dependencies, dependency_key, range);
    }
    Ok(dependencies)
}

fn active_catalog_requests(
    candidate: &SimpleCandidate,
    env: &ResolverEnvironment,
    extras: &[String],
) -> Vec<CatalogRequest> {
    let active_extras = active_extra_names(extras);
    candidate
        .dependencies
        .iter()
        .filter(|requirement| requirement.evaluate_markers(&env.markers, &active_extras))
        .map(catalog_request)
        .collect()
}

fn variant_constraints(
    package: &str,
    candidate: &SimpleCandidate,
    env: &ResolverEnvironment,
    extras: &[String],
) -> Result<BTreeMap<PackageKey, Ranges<Version>>, ResolverError> {
    let mut dependencies = dependency_constraints(candidate, env, extras)?;
    dependencies.insert(
        PackageKey::Base(package.to_string()),
        Ranges::singleton(candidate.version.clone()),
    );
    Ok(dependencies)
}

fn merge_dependency(
    dependencies: &mut BTreeMap<PackageKey, Ranges<Version>>,
    key: PackageKey,
    range: Ranges<Version>,
) {
    dependencies
        .entry(key)
        .and_modify(|existing| *existing = existing.intersection(&range))
        .or_insert(range);
}

fn active_extra_names(extras: &[String]) -> Vec<ExtraName> {
    extras
        .iter()
        .map(|extra| ExtraName::from_str(extra).expect("normalized extra"))
        .collect::<Vec<_>>()
}

fn catalog_request(requirement: &Requirement) -> CatalogRequest {
    let mut extras = requirement
        .extras
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    extras.sort();
    extras.dedup();
    CatalogRequest {
        package: requirement.name.to_string(),
        extras,
    }
}

fn dependency_key(requirement: &Requirement) -> PackageKey {
    let name = requirement.name.to_string();
    if requirement.extras.is_empty() {
        PackageKey::Base(name)
    } else {
        PackageKey::Variant {
            name,
            extras: requirement
                .extras
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        }
    }
}

fn requirement_range(requirement: &Requirement) -> Result<Ranges<Version>, ResolverError> {
    match requirement.version_or_url.as_ref() {
        None => Ok(Ranges::full()),
        Some(VersionOrUrl::VersionSpecifier(specifiers)) => {
            requirement_to_range(requirement.name.as_ref(), specifiers)
        }
        Some(VersionOrUrl::Url(_)) => Err(ResolverError::UnsupportedDirectUrlRequirement {
            package: requirement.name.to_string(),
        }),
    }
}

fn collapse_solution(
    request: &ResolutionRequest,
    catalog: &HashMap<String, Vec<SimpleCandidate>>,
    solution: Map<PackageKey, Version>,
) -> Result<Vec<ResolvedPackage>, ResolverError> {
    let token_roots = solution
        .keys()
        .filter_map(|key| match key {
            PackageKey::Token(token) => Some(token.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();

    let adjacency = build_solution_adjacency(request, catalog, &solution)?;
    let mut memberships = BTreeMap::<(String, String), BTreeSet<ResolutionRootToken>>::new();
    for token in &token_roots {
        walk_from_token(token, &adjacency, &solution, &mut memberships);
    }

    let mut packages = Vec::new();
    for (key, version) in &solution {
        let PackageKey::Base(name) = key else {
            continue;
        };
        let candidates = catalog
            .get(name)
            .and_then(|versions| {
                versions
                    .iter()
                    .find(|candidate| &candidate.version == version)
            })
            .ok_or_else(|| ResolverError::NoMatchingVersion {
                package: name.clone(),
                requirement: version.to_string(),
            })?;
        let roots = memberships
            .get(&(name.clone(), version.to_string()))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>();

        let dependencies = dependency_constraints(candidates, &request.environment, &[])?
            .into_iter()
            .filter_map(|(dependency, range)| match dependency {
                PackageKey::Base(dep) | PackageKey::Variant { name: dep, .. } => {
                    solution.iter().find_map(|(key, version)| match key {
                        PackageKey::Base(name) if name == &dep && range.contains(version) => {
                            Some(crate::PackageDependencyRecord {
                                name: dep.clone(),
                                version: version.to_string(),
                            })
                        }
                        _ => None,
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        packages.push(ResolvedPackage {
            name: name.clone(),
            version: version.to_string(),
            requires_python: candidates.requires_python.as_ref().map(ToString::to_string),
            dependencies,
            artifacts: candidates.artifacts.clone(),
            root_tokens: roots,
        });
    }
    packages.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.version.cmp(&right.version))
    });
    Ok(packages)
}

fn build_solution_adjacency(
    request: &ResolutionRequest,
    catalog: &HashMap<String, Vec<SimpleCandidate>>,
    solution: &Map<PackageKey, Version>,
) -> Result<HashMap<PackageKey, Vec<PackageKey>>, ResolverError> {
    let mut adjacency = HashMap::new();
    adjacency.insert(
        PackageKey::Root,
        solution
            .keys()
            .filter(|key| matches!(key, PackageKey::Token(_)))
            .cloned()
            .collect(),
    );
    for root in &request.roots {
        let mut edges = Vec::new();
        for requirement_text in &root.requirements {
            let requirement =
                Requirement::<VerbatimUrl>::from_str(requirement_text).map_err(|_| {
                    ResolverError::ParseRequirement {
                        package: root.token.name.clone(),
                        value: requirement_text.clone(),
                    }
                })?;
            edges.push(dependency_key(&requirement));
        }
        adjacency.insert(PackageKey::Token(root.token.clone()), edges);
    }
    for (key, version) in solution {
        let edges = match key {
            PackageKey::Root | PackageKey::Token(_) => continue,
            PackageKey::Base(name) => {
                let candidate = catalog
                    .get(name)
                    .and_then(|versions| {
                        versions
                            .iter()
                            .find(|candidate| &candidate.version == version)
                    })
                    .expect("candidate");
                dependency_constraints(candidate, &request.environment, &[])?
                    .into_keys()
                    .collect()
            }
            PackageKey::Variant { name, extras } => {
                let candidate = catalog
                    .get(name)
                    .and_then(|versions| {
                        versions
                            .iter()
                            .find(|candidate| &candidate.version == version)
                    })
                    .expect("candidate");
                variant_constraints(name, candidate, &request.environment, extras)?
                    .into_keys()
                    .collect()
            }
        };
        adjacency.insert(key.clone(), edges);
    }
    Ok(adjacency)
}

fn walk_from_token(
    token: &ResolutionRootToken,
    adjacency: &HashMap<PackageKey, Vec<PackageKey>>,
    solution: &Map<PackageKey, Version>,
    memberships: &mut BTreeMap<(String, String), BTreeSet<ResolutionRootToken>>,
) {
    let mut queue = VecDeque::from([PackageKey::Token(token.clone())]);
    let mut seen = HashSet::new();
    while let Some(key) = queue.pop_front() {
        if !seen.insert(key.clone()) {
            continue;
        }
        if let Some(version) = solution.get(&key) {
            match &key {
                PackageKey::Base(name) | PackageKey::Variant { name, .. } => {
                    memberships
                        .entry((name.clone(), version.to_string()))
                        .or_default()
                        .insert(token.clone());
                }
                _ => {}
            }
        }
        if let Some(edges) = adjacency.get(&key) {
            queue.extend(edges.iter().cloned());
        }
    }
}

fn token_kind_name(kind: &ResolutionRootTokenKind) -> &'static str {
    match kind {
        ResolutionRootTokenKind::DependencyGroup => "group",
        ResolutionRootTokenKind::Extra => "extra",
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{
        ArtifactFixture, FixtureRoot, PackageFixture, ResolverFixtureHarness,
    };
    use crate::{ArtifactKind, ResolutionRootTokenKind, ResolverError};

    #[tokio::test]
    async fn resolves_direct_dependency_from_local_fixture() {
        let harness = ResolverFixtureHarness::new().expect("fixture harness");
        harness
            .add_package(
                PackageFixture::new("alpha").with_artifact(ArtifactFixture::wheel("1.0.0")),
            )
            .expect("alpha fixture");

        let resolved = harness.resolve(&["alpha"]).await.expect("resolution");

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "alpha");
        assert_eq!(resolved[0].version, "1.0.0");
        assert!(resolved[0].dependencies.is_empty());
        assert_eq!(resolved[0].root_tokens.len(), 1);
        assert_eq!(resolved[0].artifacts[0].kind, ArtifactKind::Wheel);
    }

    #[tokio::test]
    async fn resolves_transitive_dependency_from_local_fixture() {
        let harness = ResolverFixtureHarness::new().expect("fixture harness");
        harness
            .add_package(
                PackageFixture::new("alpha")
                    .with_artifact(ArtifactFixture::wheel("1.0.0").with_dependency("beta>=2,<3")),
            )
            .expect("alpha fixture");
        harness
            .add_package(PackageFixture::new("beta").with_artifact(ArtifactFixture::wheel("2.1.0")))
            .expect("beta fixture");

        let resolved = harness.resolve(&["alpha"]).await.expect("resolution");
        let alpha = package(&resolved, "alpha");
        let beta = package(&resolved, "beta");

        assert_eq!(alpha.dependencies.len(), 1);
        assert_eq!(alpha.dependencies[0].name, "beta");
        assert_eq!(alpha.dependencies[0].version, beta.version);
    }

    #[tokio::test]
    async fn reports_conflicts_from_local_fixture_graph() {
        let harness = ResolverFixtureHarness::new().expect("fixture harness");
        harness
            .add_package(
                PackageFixture::new("alpha")
                    .with_artifact(ArtifactFixture::wheel("1.0.0").with_dependency("shared<2")),
            )
            .expect("alpha fixture");
        harness
            .add_package(
                PackageFixture::new("bravo")
                    .with_artifact(ArtifactFixture::wheel("1.0.0").with_dependency("shared>=2")),
            )
            .expect("bravo fixture");
        harness
            .add_package(
                PackageFixture::new("shared").with_artifact(ArtifactFixture::wheel("1.5.0")),
            )
            .expect("shared v1 fixture");
        harness
            .add_package(
                PackageFixture::new("shared").with_artifact(ArtifactFixture::wheel("2.0.0")),
            )
            .expect("shared v2 fixture");

        let error = harness
            .resolve(&["alpha", "bravo"])
            .await
            .expect_err("conflict");

        match error {
            ResolverError::Solve {
                conflict: Some(conflict),
                ..
            } => {
                assert_eq!(
                    conflict.summary,
                    "`alpha` requires `shared` `<2`, but `bravo` requires `shared` `>=2`."
                );
                assert!(conflict.report.contains("alpha"));
                assert!(conflict.report.contains("bravo"));
                assert!(conflict.report.contains("shared"));
            }
            other => panic!("unexpected resolver error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn reports_conflicts_between_group_and_extra_roots() {
        let harness = ResolverFixtureHarness::new().expect("fixture harness");
        harness
            .add_package(
                PackageFixture::new("shared").with_artifact(ArtifactFixture::wheel("1.5.0")),
            )
            .expect("shared v1 fixture");
        harness
            .add_package(
                PackageFixture::new("shared").with_artifact(ArtifactFixture::wheel("2.0.0")),
            )
            .expect("shared v2 fixture");

        let error = harness
            .resolve_roots(vec![
                FixtureRoot::new(
                    ResolutionRootTokenKind::DependencyGroup,
                    "pyra-default",
                    &["shared<2"],
                ),
                FixtureRoot::new(ResolutionRootTokenKind::Extra, "http2", &["shared>=2"]),
            ])
            .await
            .expect_err("conflict");

        match error {
            ResolverError::Solve {
                conflict: Some(conflict),
                ..
            } => assert_eq!(
                conflict.summary,
                "dependency group `pyra-default` requires `shared` `<2`, but extra `http2` requires `shared` `>=2`."
            ),
            other => panic!("unexpected resolver error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn reports_missing_core_metadata_from_local_fixture() {
        let harness = ResolverFixtureHarness::new().expect("fixture harness");
        harness
            .add_package(
                PackageFixture::new("alpha")
                    .with_artifact(ArtifactFixture::wheel("1.0.0").without_core_metadata()),
            )
            .expect("alpha fixture");

        let error = harness
            .resolve(&["alpha"])
            .await
            .expect_err("missing metadata");

        assert!(matches!(
            error,
            ResolverError::MissingCoreMetadata { package } if package == "alpha"
        ));
    }

    #[tokio::test]
    async fn reports_missing_installable_artifacts_from_local_fixture() {
        let harness = ResolverFixtureHarness::new().expect("fixture harness");
        harness
            .add_package(
                PackageFixture::new("alpha")
                    .with_artifact(ArtifactFixture::wheel_with_tags(
                        "1.0.0",
                        "cp313",
                        "cp313",
                        "macosx_11_0_arm64",
                    ))
                    .with_artifact(ArtifactFixture::sdist("1.0.0").yanked()),
            )
            .expect("alpha fixture");

        let error = harness
            .resolve(&["alpha"])
            .await
            .expect_err("no installable artifacts");

        assert!(matches!(
            error,
            ResolverError::NoInstallableArtifacts { package } if package == "alpha"
        ));
    }

    #[tokio::test]
    async fn prefers_wheels_and_falls_back_to_sdists_with_local_fixtures() {
        let harness = ResolverFixtureHarness::new().expect("fixture harness");
        harness
            .add_package(
                PackageFixture::new("preferred")
                    .with_artifact(ArtifactFixture::wheel("1.0.0"))
                    .with_artifact(ArtifactFixture::sdist("1.0.0")),
            )
            .expect("preferred fixture");
        harness
            .add_package(
                PackageFixture::new("fallback")
                    .with_artifact(ArtifactFixture::wheel_with_tags(
                        "1.0.0",
                        "cp313",
                        "cp313",
                        "macosx_11_0_arm64",
                    ))
                    .with_artifact(ArtifactFixture::sdist("1.0.0").with_requires_python(">=3.13")),
            )
            .expect("fallback fixture");

        let resolved = harness
            .resolve(&["preferred", "fallback"])
            .await
            .expect("resolution");
        let preferred = package(&resolved, "preferred");
        let fallback = package(&resolved, "fallback");

        assert!(
            preferred
                .artifacts
                .iter()
                .all(|artifact| artifact.kind == ArtifactKind::Wheel)
        );
        assert!(
            preferred
                .artifacts
                .iter()
                .all(|artifact| artifact.name.ends_with(".whl"))
        );
        assert!(
            fallback
                .artifacts
                .iter()
                .all(|artifact| artifact.kind == ArtifactKind::Sdist)
        );
        assert!(
            fallback
                .artifacts
                .iter()
                .all(|artifact| artifact.name.ends_with(".tar.gz"))
        );
    }

    #[tokio::test]
    async fn keeps_multiple_compatible_wheel_choices_for_one_version() {
        let harness = ResolverFixtureHarness::new().expect("fixture harness");
        harness
            .add_package(
                PackageFixture::new("multiwheel")
                    .with_artifact(ArtifactFixture::wheel("1.0.0"))
                    .with_artifact(ArtifactFixture::wheel_with_tags(
                        "1.0.0",
                        "cp313",
                        "abi3",
                        "manylinux_2_17_x86_64",
                    ))
                    .with_artifact(ArtifactFixture::sdist("1.0.0")),
            )
            .expect("multiwheel fixture");

        let resolved = harness.resolve(&["multiwheel"]).await.expect("resolution");
        let multiwheel = package(&resolved, "multiwheel");

        assert_eq!(multiwheel.artifacts.len(), 2);
        assert!(
            multiwheel
                .artifacts
                .iter()
                .all(|artifact| artifact.kind == ArtifactKind::Wheel)
        );
    }

    #[tokio::test]
    async fn resolves_sdist_only_package_from_local_fixture() {
        let harness = ResolverFixtureHarness::new().expect("fixture harness");
        harness
            .add_package(
                PackageFixture::new("sdistonly")
                    .with_artifact(ArtifactFixture::sdist("1.2.3").with_dependency("shared>=1")),
            )
            .expect("sdistonly fixture");
        harness
            .add_package(
                PackageFixture::new("shared").with_artifact(ArtifactFixture::wheel("1.5.0")),
            )
            .expect("shared fixture");

        let resolved = harness.resolve(&["sdistonly"]).await.expect("resolution");
        let package = package(&resolved, "sdistonly");

        assert!(
            package
                .artifacts
                .iter()
                .all(|artifact| artifact.kind == ArtifactKind::Sdist)
        );
        assert_eq!(package.dependencies[0].name, "shared");
    }

    #[tokio::test]
    async fn group_only_packages_receive_only_group_root_tokens() {
        let harness = ResolverFixtureHarness::new().expect("fixture harness");
        harness
            .add_package(
                PackageFixture::new("pytest").with_artifact(ArtifactFixture::wheel("8.3.0")),
            )
            .expect("pytest fixture");

        let resolved = harness
            .resolve_roots(vec![FixtureRoot::new(
                ResolutionRootTokenKind::DependencyGroup,
                "dev",
                &["pytest"],
            )])
            .await
            .expect("resolution");

        assert_root_tokens(package(&resolved, "pytest"), &["group:dev"]);
    }

    #[tokio::test]
    async fn extra_only_packages_receive_only_extra_root_tokens() {
        let harness = ResolverFixtureHarness::new().expect("fixture harness");
        harness
            .add_package(
                PackageFixture::new("httpx").with_artifact(ArtifactFixture::wheel("0.28.0")),
            )
            .expect("httpx fixture");

        let resolved = harness
            .resolve_roots(vec![FixtureRoot::new(
                ResolutionRootTokenKind::Extra,
                "http",
                &["httpx"],
            )])
            .await
            .expect("resolution");

        assert_root_tokens(package(&resolved, "httpx"), &["extra:http"]);
    }

    #[tokio::test]
    async fn shared_transitive_packages_preserve_all_applicable_root_memberships() {
        let harness = ResolverFixtureHarness::new().expect("fixture harness");
        harness
            .add_package(
                PackageFixture::new("alpha")
                    .with_artifact(ArtifactFixture::wheel("1.0.0").with_dependency("shared>=1")),
            )
            .expect("alpha fixture");
        harness
            .add_package(
                PackageFixture::new("pytest")
                    .with_artifact(ArtifactFixture::wheel("8.3.0").with_dependency("shared>=1")),
            )
            .expect("pytest fixture");
        harness
            .add_package(
                PackageFixture::new("shared").with_artifact(ArtifactFixture::wheel("1.5.0")),
            )
            .expect("shared fixture");

        let resolved = harness
            .resolve_roots(vec![
                FixtureRoot::new(
                    ResolutionRootTokenKind::DependencyGroup,
                    "pyra-default",
                    &["alpha"],
                ),
                FixtureRoot::new(ResolutionRootTokenKind::DependencyGroup, "dev", &["pytest"]),
            ])
            .await
            .expect("resolution");

        assert_root_tokens(package(&resolved, "alpha"), &["group:pyra-default"]);
        assert_root_tokens(package(&resolved, "pytest"), &["group:dev"]);
        assert_root_tokens(
            package(&resolved, "shared"),
            &["group:dev", "group:pyra-default"],
        );
    }

    #[tokio::test]
    async fn marker_filtered_requirements_are_excluded_when_environment_does_not_match() {
        let harness = ResolverFixtureHarness::new().expect("fixture harness");
        harness
            .add_package(PackageFixture::new("alpha").with_artifact(
                ArtifactFixture::wheel("1.0.0").with_dependency("beta>=1; sys_platform == 'win32'"),
            ))
            .expect("alpha fixture");

        let resolved = harness.resolve(&["alpha"]).await.expect("resolution");

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "alpha");
    }

    #[tokio::test]
    async fn duplicate_logical_declarations_across_scopes_preserve_each_root_membership_once() {
        let harness = ResolverFixtureHarness::new().expect("fixture harness");
        harness
            .add_package(
                PackageFixture::new("shared").with_artifact(ArtifactFixture::wheel("1.5.0")),
            )
            .expect("shared fixture");

        let resolved = harness
            .resolve_roots(vec![
                FixtureRoot::new(
                    ResolutionRootTokenKind::DependencyGroup,
                    "pyra-default",
                    &["shared>=1"],
                ),
                FixtureRoot::new(
                    ResolutionRootTokenKind::DependencyGroup,
                    "dev",
                    &["shared>=1"],
                ),
            ])
            .await
            .expect("resolution");

        assert_root_tokens(
            package(&resolved, "shared"),
            &["group:dev", "group:pyra-default"],
        );
    }

    fn package<'a>(
        packages: &'a [crate::ResolvedPackage],
        name: &str,
    ) -> &'a crate::ResolvedPackage {
        packages
            .iter()
            .find(|package| package.name == name)
            .expect("resolved package")
    }

    fn assert_root_tokens(package: &crate::ResolvedPackage, expected: &[&str]) {
        // Lock selection depends on these memberships staying precise per scope,
        // so compare a normalized label set rather than incidental insertion order.
        let actual = package
            .root_tokens
            .iter()
            .map(|token| match token.kind {
                ResolutionRootTokenKind::DependencyGroup => format!("group:{}", token.name),
                ResolutionRootTokenKind::Extra => format!("extra:{}", token.name),
            })
            .collect::<Vec<_>>();
        let expected = expected.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_eq!(
            actual, expected,
            "unexpected root tokens for {}",
            package.name
        );
    }
}
