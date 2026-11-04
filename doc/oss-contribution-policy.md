# OSS development and contribution boundary

## Development direction

Kitu is an open-source framework. Prioritize adopting it in actual game
development and improving it from observed integration needs. Expanding or
polishing a showcase demo is not a prerequisite for adoption. Keep the public
reference demo useful for reproducible examples and compatibility checks;
schedule further demo enhancements only when they serve a concrete framework
need or are explicitly prioritized.

Closed-source game development and Kitu contributions are separate workstreams.
Application rules, content, presentation, service integration, and delivery
belong in the application's own private repository. Kitu receives reusable
contracts, fixes, tools, documentation, and independently reproducible tests.
Do not make framework builds, tests, or public CI depend on a private checkout,
private package, credential, service, or dataset.

## Information that must stay outside Kitu

Never put identifying or confidential information about a private consuming
application into this repository or its collaboration surfaces. This includes:

- Application names, codenames, identifiers, customer/team details, repository
  URLs, private issue links, internal hostnames, and local paths that identify it.
- Unreleased specifications, game rules, content, balance tables, schemas,
  source code, assets, screenshots, recordings, save files, and production data.
- Credentials, environment files, configuration, crash dumps, traces, replay
  captures, logs, build artifacts, and generated reports containing such details.

The rule covers code and comments, documentation, examples, fixtures, file and
branch names, commit messages and history, issues and PRs, review comments,
attachments, release notes, packages, CI logs/artifacts, and AI-generated output.
Do not retain private material even in ignored files inside the Kitu checkout.
Keep private workspaces, prompts, notes, and raw evidence outside it. An ignore
rule prevents some accidental staging; it does not remove tracked files or
sanitize content. Never store a list of private names here to detect leakage.

Existing intentionally public reference-demo material may remain public. That
does not authorize disclosure of any private consuming application. Do not add
new application identities or details to Kitu; use synthetic examples instead.

## Contributing an improvement discovered in a private application

1. In the private workspace, identify the reusable framework requirement and
   separate it from application rules and confidential implementation details.
2. Produce a minimal, independently authored synthetic reproduction using
   generic names, invented values, and only public dependencies. Renaming the
   application in a copied proprietary fixture is not sufficient. Confirm the
   right to contribute any code or assets; sanitizing names does not establish it.
3. Describe the public contract, trigger, expected behavior, and acceptance
   checks without identifying the consuming application or linking private
   evidence. Keep any private-to-public tracking link on the private side only.
4. Implement and verify the generic change in Kitu. Validate actual application
   integration separately in its private environment. Do not upload raw private
   evidence to Kitu to substantiate the public change.
5. Before staging or sharing, review the full diff, staged/untracked files,
   filenames, commit/branch metadata, issue/PR text, and any generated output or
   attachments. Use only reviewed synthetic/public data in public CI. Verify
   package file lists when packaging is affected.

AI assistants and delegated workers must receive only the public, sanitized
task context needed for Kitu work. Do not copy private application context into
Kitu task files, worker logs, public model prompts, or generated assets. When
the separation or contribution rights are unclear, keep the material outside
Kitu and resolve the uncertainty privately before including it.

## Review and incident handling

The contributor and reviewer must check this boundary before committing,
opening or updating an issue/PR, uploading evidence, or distributing a package.
Templates provide reminders; tests, secret scanners, and ignore rules cannot
prove the absence of confidential application information. Keep synthetic
regression tests with the framework fix so public validation stands on its own.

If private material is found, stop sharing it and report the affected location
privately to the maintainer without repeating the material in public. Remove
uncommitted material from the Kitu checkout. For material already committed or
uploaded, coordinate cleanup of history, logs, artifacts, caches, and packages
as applicable; deleting the current file alone is insufficient. Rotate exposed
credentials when necessary. Do not rewrite shared history without explicit
authorization.
