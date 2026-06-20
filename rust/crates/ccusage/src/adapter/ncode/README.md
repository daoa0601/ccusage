# NCode Source

Default data directory:

- `~/.ncode/projects/`

`NCODE_CONFIG_DIR` can specify one path or comma-separated multiple paths. Each path can be an NCode config directory containing `projects/`, or the `projects/` directory itself.

NCode writes Claude-compatible JSONL transcripts, so this adapter uses the shared Claude parser and keeps NCode as its own source in unified reports.
