//! Framework-neutral configuration, validation, and transport primitives.
//!
//! Feature business rules do not belong in this crate.

pub mod config;
pub mod errors;
pub mod types;

pub use config::{
    config_key, ConfigEnvironment, ConfigKeySpec, ConfigValidation, SecretClass, CONFIG_KEYS,
};
pub use errors::{ApiError, ApiFailure, ErrorCode};
pub use types::{
    api_operation, ApiAuth, ApiMethod, ApiOperationSpec, ApiSuccess, Page, PageRequest,
    PaginationError, RequestId, API_OPERATIONS, MAX_PAGE_SIZE,
};
