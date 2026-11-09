//! The AI module. Specification: `docs/specs/ai.md`.
//!
//! The CAD core does not know this crate. What reaches an external provider is
//! decided here and checked by [`consent::ConsentStore`]; what comes back is only
//! ever text and a [`proposal`], which changes nothing until the user approves it.
//! Check results are computed by the ERC and DRC crates: no type in this crate
//! can set them.

pub mod analysis;
pub mod consent;
pub mod context;
pub mod proposal;
pub mod provider;

pub use analysis::{analyze, Finding};
pub use consent::{Consent, ConsentStore};
pub use context::{build_request, CheckCounts, Sections, Task};
pub use proposal::{apply_changes, extract_block, parse_changes, preview, Change, Preview};
pub use provider::{
    classify, AiError, AiProvider, ChatMessage, ChatOutcome, ChatRequest, HttpProvider, Locality,
    ProviderConfig, ProviderKind, Role, StopReason,
};
