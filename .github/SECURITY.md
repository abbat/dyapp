# Security Policy

## Supported versions

Nothing is released yet. Reports are taken for the `main` branch.

## Scope

Report vulnerabilities in the dyapp source code, build and release process, and official artifacts
when they exist. Independently operated nodes and third-party services are outside this project's
control; report a flaw in dyapp that affects them here.

## Reporting a vulnerability

Report privately through GitHub's private vulnerability reporting:
<https://github.com/abbat/dyapp/security/advisories/new>. Do not open a public issue.

Include what is affected (component, commit), how to reproduce it and the impact you see. As the
[README](../README.md) says, the code and the replies are written by AI.

Maintainers never ask for identity or node keys, recovery codes, backups, or node logs. Do not send
these with a report, even if someone claiming to represent the project requests them.

## What happens next

1. The report is confirmed or declined in the advisory thread.
2. A confirmed vulnerability is fixed within 48 hours when critical and within 7 days when high,
   counted from the report. Lower severities are fixed in the normal flow.
3. The fix includes a regression test that names the advisory's GHSA or CVE id.
4. The advisory is published with the fix, crediting the reporter unless they ask not to be.

How the fix times are measured: [docs/security/assurance.md](../docs/security/assurance.md#vulnerability-disclosure).
