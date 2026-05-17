//! Session use case boundaries.

mod common;
pub mod port;
mod standard;

#[cfg(test)]
mod tests;

pub use port::{
    AppendSessionChatAssistantRequest, AppendSessionChatAssistantResult,
    AppendSessionMessagesRequest, AppendSessionMessagesResult, ApplySessionAppendCompactionRequest,
    ApplySessionAppendCompactionResult, ApplySessionChatSummaryRequest,
    ApplySessionChatSummaryResult, ApplySessionCompactionRequest, ApplySessionCompactionResult,
    CreateSessionRequest, CreateSessionResult, PrepareSessionChatTurnRequest,
    PrepareSessionChatTurnResult, PrepareSessionCompactionRequest, PrepareSessionCompactionResult,
    RemoveSessionRequest, RemoveSessionResult, ResolvedSessionStore, SessionCatalogReadUseCase,
    SessionChatContextUseCase, SessionChatSummaryScope, SessionCompactionUseCase,
    SessionInspectRequest, SessionInspectResult, SessionListRequest, SessionListResult,
    SessionMessagesRequest, SessionMessagesResult, SessionMutationUseCase,
    SessionStoreResolutionUseCase, SessionStoreSelection, SessionSummaryRequirement,
    SessionSummaryUseCase, SessionSummaryUseCaseRequest, SessionSummaryUseCaseResult,
    SessionUseCaseFuture, UpdateSessionRequest, UpdateSessionResult,
};
pub use standard::StdSessionUseCase;
