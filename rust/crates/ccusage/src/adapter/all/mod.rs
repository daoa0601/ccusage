mod loader;
mod report;
mod types;

use crate::{cli::AgentCommandArgs, print_json_or_jq, report_cache, wants_json, Result};

pub(crate) fn run(args: AgentCommandArgs) -> Result<()> {
    let kind = args.kind;
    let shared = args.shared;
    let parameters = report_cache::report_parameters("all", kind, &shared);
    let cached = report_cache::with_report_cache(
        "all",
        parameters,
        report_cache::all_report_sources(&shared),
        &shared,
        || loader::load_rows(kind, &shared).map(types::AllLoadResult::into_cache),
    )?;
    let result = cached.into_result();
    if wants_json(&shared) {
        return print_json_or_jq(
            report::report_json(&result.rows, kind),
            shared.jq.as_deref(),
        );
    }
    report::print_table(&result.rows, kind, &shared, &result.detected_agents)
}

#[cfg(test)]
use loader::{
    aggregate_rows, aggregate_rows_by_provider, codex_group_row, load_agent_rows_parallel,
};
#[cfg(test)]
use report::{all_report_title, all_table_columns, all_table_row, report_json};
#[cfg(test)]
use types::{AgentLoadSpec, AgentRows, AllRow};

#[cfg(test)]
mod tests;
