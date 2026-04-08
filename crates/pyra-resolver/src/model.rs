//! Public resolver request and result models.

use reqwest::Client;

use crate::{ResolverEnvironment, error::ResolverError, provider::resolve_request};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ResolutionRootTokenKind {
    DependencyGroup,
    Extra,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ResolutionRootToken {
    pub kind: ResolutionRootTokenKind,
    pub name: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ResolutionRoot {
    pub token: ResolutionRootToken,
    pub requirements: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ResolutionRequest {
    pub environment: ResolverEnvironment,
    pub roots: Vec<ResolutionRoot>,
    pub index_url: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ArtifactKind {
    Wheel,
    Sdist,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ArtifactRecord {
    pub kind: ArtifactKind,
    pub name: String,
    pub url: String,
    pub size: Option<u64>,
    pub upload_time: Option<String>,
    pub sha256: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PackageDependencyRecord {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ResolvedPackage {
    pub name: String,
    pub version: String,
    pub requires_python: Option<String>,
    pub dependencies: Vec<PackageDependencyRecord>,
    pub artifacts: Vec<ArtifactRecord>,
    pub root_tokens: Vec<ResolutionRootToken>,
}

#[derive(Debug, Clone)]
pub struct Resolver {
    client: Client,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub async fn resolve(
        &self,
        request: ResolutionRequest,
    ) -> Result<Vec<ResolvedPackage>, ResolverError> {
        resolve_request(&self.client, request).await
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}
