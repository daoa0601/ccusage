use crate::{
    adapter::{amp, claude, opencode},
    cli::{AgentCommandArgs, SharedArgs},
    filter_loaded_entries_by_date, print_json_or_jq, sort_summaries, wants_json, LoadedEntry,
    Result, UsageSummary,
};

pub(crate) fn load_entries(
    shared: &SharedArgs,
    project_filter: Option<&str>,
) -> Result<Vec<LoadedEntry>> {
    claude::load_ncode_entries(shared, project_filter)
}

pub(crate) fn load_daily_summaries(
    shared: &SharedArgs,
    project_filter: Option<&str>,
    group_by_project: bool,
) -> Result<Vec<UsageSummary>> {
    claude::load_ncode_daily_summaries(shared, project_filter, group_by_project)
}

pub(crate) fn run(args: AgentCommandArgs) -> Result<()> {
    let shared = args.shared;
    let mut entries = load_entries(&shared, None)?;
    filter_loaded_entries_by_date(&mut entries, &shared);
    let mut rows = opencode::summarize_entries(&entries, args.kind)?;
    sort_summaries(&mut rows, &shared.order, opencode::summary_period);
    if wants_json(&shared) {
        return print_json_or_jq(
            amp::report_from_rows(&rows, args.kind),
            shared.jq.as_deref(),
        );
    }
    amp::print_table_for_agent("NCode", args.kind, &rows, &shared)
}
