pub(crate) mod loader;
mod parser;
mod paths;
mod report;

#[cfg(test)]
pub(crate) use report::report_json;
pub(crate) use report::{agent_summary_json, first_column, summarize_entries, summary_period};

use crate::{
    cli::AgentCommandArgs, filter_loaded_entries_by_date, print_json_or_jq, print_usage_table,
    report_cache, sort_summaries, wants_json, CachedUsageSummary, Result,
};

pub(crate) fn run(args: AgentCommandArgs) -> Result<()> {
    let shared = args.shared;
    let parameters = report_cache::report_parameters("opencode", args.kind, &shared);
    let cached_rows = report_cache::with_report_cache(
        "opencode",
        parameters,
        report_cache::opencode_report_sources(),
        &shared,
        || {
            let mut entries = loader::load_entries(&shared)?;
            filter_loaded_entries_by_date(&mut entries, &shared);
            let mut rows = summarize_entries(&entries, args.kind)?;
            sort_summaries(&mut rows, &shared.order, |row| summary_period(row));
            Ok(rows
                .into_iter()
                .map(CachedUsageSummary::from)
                .collect::<Vec<_>>())
        },
    )?;
    let rows = cached_rows
        .into_iter()
        .map(CachedUsageSummary::into_summary)
        .collect::<Vec<_>>();
    if wants_json(&shared) {
        return print_json_or_jq(
            report::report_from_rows(&rows, args.kind),
            shared.jq.as_deref(),
        );
    }
    print_usage_table(
        "OpenCode Token Usage Report",
        first_column(args.kind),
        &rows,
        &shared,
        false,
        None,
    )?;
    Ok(())
}
