//! Lifecycle option intent, separate from the persisted server spec shape.

use super::domain::ServerCapability;
use crate::foundation::error::{KernelError, KernelResult};

/// Preserves explicit null as well as a supplied value until target validation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LifecycleInput<T> {
    #[default]
    Omitted,
    Provided(Option<T>),
}

impl<T: Copy> LifecycleInput<T> {
    pub fn value(self) -> Option<T> {
        match self {
            Self::Omitted => None,
            Self::Provided(value) => value,
        }
    }

    pub fn is_provided(self) -> bool {
        matches!(self, Self::Provided(_))
    }

    /// Combine aliases without losing the presence of an explicit null.
    pub fn with_presence(value: Option<T>, provided: bool) -> Self {
        if provided {
            Self::Provided(value)
        } else {
            Self::Omitted
        }
    }
}

impl<T> From<Option<T>> for LifecycleInput<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Provided(Some(value)),
            None => Self::Omitted,
        }
    }
}

/// Startup intent only; this does not change shared runtime idle ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadMode {
    Lazy,
    Eager,
}

impl LoadMode {
    pub fn from_lazy_load(lazy_load: bool) -> Self {
        if lazy_load {
            Self::Lazy
        } else {
            Self::Eager
        }
    }

    pub fn ensure_supported(self, capability: ServerCapability) -> KernelResult<()> {
        if capability == ServerCapability::ImageGeneration && self == Self::Eager {
            return Err(KernelError::UnsupportedTarget(
                "local image-generation requires --lazy-load (REST lazy_load:true); create a lazy server spec with the same model and desired settings; workflow-aware eager loading is not supported".to_string(),
            ));
        }
        Ok(())
    }
}
