//! Operation-specific resource guard validators.

mod delete_adapter;
mod delete_cluster;
mod delete_dataset;
mod delete_model;
mod delete_server_spec;
mod delete_train_plan;
mod rebind_adapter;
mod remove_model_capability;
mod replace_cluster;
mod replace_model_capabilities;

#[cfg(test)]
mod tests;

pub(crate) use delete_adapter::validate as validate_delete_adapter;
pub(crate) use delete_cluster::validate as validate_delete_cluster;
pub(crate) use delete_dataset::validate as validate_delete_dataset;
pub(crate) use delete_model::validate as validate_delete_model;
pub(crate) use delete_server_spec::validate as validate_delete_server_spec;
pub(crate) use delete_train_plan::validate as validate_delete_train_plan;
pub(crate) use rebind_adapter::validate as validate_rebind_adapter;
pub(crate) use remove_model_capability::validate as validate_remove_model_capability;
pub(crate) use replace_cluster::validate as validate_replace_cluster;
pub(crate) use replace_model_capabilities::validate as validate_replace_model_capabilities;
