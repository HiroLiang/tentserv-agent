use tentgent_kernel::{
    features::session::{domain::SessionRefSelector, usecases::SessionStoreSelection},
    foundation::{error::KernelError, layout::LayoutResolveMode},
};

use crate::transport::rest::{error::RestError, state::RestState};

pub(in crate::handlers::rest) fn session_store_selection(
    state: &RestState,
) -> SessionStoreSelection {
    SessionStoreSelection::default_file(state.app().layout_input(LayoutResolveMode::ReadOnly))
}

pub(in crate::handlers::rest) fn parse_selector(
    reference: &str,
) -> Result<SessionRefSelector, RestError> {
    SessionRefSelector::parse(reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid session reference: {err}"))
    })
}

pub(in crate::handlers::rest) fn session_error(error: KernelError) -> RestError {
    match error {
        KernelError::SessionStoreUnavailable(message) => {
            RestError::store_lookup("session_read_failed", message)
        }
        other => RestError::kernel("session_read_failed", other),
    }
}

pub(in crate::handlers::rest) fn session_mutation_error(error: KernelError) -> RestError {
    match error {
        KernelError::SessionStoreUnavailable(message) if message.contains("lock") => {
            RestError::conflict("session_busy", message)
        }
        KernelError::SessionStoreUnavailable(message)
            if message.contains("compaction")
                || message.contains("would exceed")
                || message.contains("required") =>
        {
            RestError::conflict("session_compaction_required", message)
        }
        KernelError::SessionStoreUnavailable(message) => {
            RestError::store_lookup("session_mutation_failed", message)
        }
        other => RestError::kernel("session_mutation_failed", other),
    }
}
