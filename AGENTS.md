# Working agreements for automated contributors

This file applies to any automated coding agent working in this repository,
whatever vendor or tool it comes from. It is the place to look before generating
a document, and it takes precedence over an agent's own defaults.

Human contributors are welcome to follow it too; nothing here is agent-specific
except the audience.

## Documentation placement

**Agents MUST NOT create sprint reports, audits, certification reports, research
reports, compatibility reports, or implementation closeouts at the repository
root.**

Root Markdown is reserved for project entry points and governance. Today that
means exactly two files:

| File | Why it is at root |
| --- | --- |
| `README.md` | the project's entry point — what OmniBridge is, how to build and run it |
| `AGENTS.md` | this file — the conventions an agent needs before it writes anything |

`CONTRIBUTING.md`, `SECURITY.md` and `CODE_OF_CONDUCT.md` would also belong at
root if the project adopted them. Nothing else does.

**All generated project documentation must be placed under the appropriate
`docs/` category.** [`docs/README.md`](docs/README.md) carries the taxonomy and
a table that maps a document's kind to its directory. Classify by what the
document *is* — audit, certification, report, research, architecture, design,
decision, migration — not by the sprint that produced it and not by its
filename.

If no existing category fits, say so and propose one rather than defaulting to
the root.

## Historical documents are evidence

`docs/audits/`, `docs/certification/` and `docs/reports/` hold records of what
was measured, on which commit, on which hardware, on a given day. Do not edit
them into agreement with the present. Preserve verdicts, dates, commands,
command output, test counts, hardware details, conclusions, AnyFlow-era naming,
old GitHub URLs, and old paths quoted inside transcripts — including paths that
no longer resolve.

When a later change invalidates a claim in one of these documents, add a dated
superseding note that names the branch or date responsible, and leave the
original claim standing beneath it.

Moving a file is not rewriting it. Use `git mv` so the rename is recorded, and
verify the content is byte-identical afterwards. The only content change a move
may carry is the repair of a Markdown link whose target moved — and then only
the link target changes, never the link text.
