//! Find Usages beyond the language servers: providers that know of other references to a symbol
//! (Unity assets…) add them to the servers' answer, merged with it or
//! as a group of their own.

use crate::types::{ReferenceGroup, ReferencesIncomplete};
use crate::{DocSnap, LspManager};
use futures::future::BoxFuture;
use ide_core::{CancellationToken, IdeResult, LanguageId, Location, Pos};
use std::path::Path;

/// What is being looked for: the symbol at `pos` of a document.
pub struct ReferenceQuery<'a> {
    pub manager: &'a LspManager,
    pub(crate) snap: &'a DocSnap,
    pub pos: Pos,
    pub include_declaration: bool,
    /// A full Find Usages (not a cheap probe for code lenses or bulk analysis).
    pub project_wide: bool,
    pub cancel: &'a CancellationToken,
}

impl ReferenceQuery<'_> {
    pub fn path(&self) -> &Path {
        &self.snap.path
    }

    pub fn language(&self) -> &LanguageId {
        &self.snap.language
    }

    pub fn text(&self) -> &str {
        self.snap.text.as_str()
    }
}

/// What a provider found.
#[derive(Debug, Default)]
pub struct ProvidedReferences {
    /// Shown as its own group in Find Usages; `None` merges the locations with the servers'.
    pub title: Option<String>,
    pub locations: Vec<Location>,
    pub incomplete: Option<ReferencesIncomplete>,
    /// The whole answer: the servers aren't asked (a file they don't analyse).
    pub exclusive: bool,
}

pub trait ReferenceProvider: Send + Sync + 'static {
    /// `None` when the provider has nothing to say about this symbol.
    fn references<'a>(&'a self, query: &'a ReferenceQuery<'a>) -> BoxFuture<'a, IdeResult<Option<ProvidedReferences>>>;
}

/// Puts what providers found into a references answer.
pub(crate) fn merge(result: &mut crate::types::References, provided: ProvidedReferences) {
    result.incomplete = result.incomplete.or(provided.incomplete);
    match provided.title {
        None => result.locations.extend(provided.locations),
        Some(title) => result.groups.push(ReferenceGroup { title, locations: provided.locations }),
    }
}
