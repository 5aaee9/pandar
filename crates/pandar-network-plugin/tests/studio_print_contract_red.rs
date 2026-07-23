#![cfg(any(unix, windows))]

#[path = "studio_print_contract_red/field_cases.rs"]
mod field_cases;
#[path = "studio_print_contract_red/field_contract.rs"]
mod field_contract;
#[path = "studio_print_contract_red/harness.rs"]
mod harness;
#[path = "studio_print_contract_red/lifecycle.rs"]
mod lifecycle;
#[path = "studio_print_contract_red/lifecycle_edges.rs"]
mod lifecycle_edges;
#[path = "studio_print_contract_red/pinned.rs"]
mod pinned;
#[allow(dead_code)]
mod support;
#[path = "studio_print_contract_red/tasks.rs"]
mod tasks;
