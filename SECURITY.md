# Security Policy

## Supported Versions

| Version | Supported |
| ------- | --------- |
| latest  | Yes       |
| older   | No        |

Only the latest published release receives security updates.

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Report privately via [GitHub Security Advisories](https://github.com/Sigilweaver/SpecLance/security/advisories/new).

Include:

- A description of the vulnerability and its potential impact.
- Steps to reproduce or a proof of concept (a small input file is
  ideal).
- The affected crate (`speclance-core`, `speclance-ms`, `speclance-cli`,
  or `speclance-py`).
- The OS, Rust toolchain, and crate version you were running.

Expect an initial acknowledgment within 7 days.

## Scope

In scope:

- **Parser / writer correctness on malicious input.** SpecLance
  ingests vendor formats (via `openmassspec-io`) and mzML, and writes
  Lance datasets. Crashes (panics, OOB reads, infinite loops),
  arbitrary file writes, or memory corruption triggered by a crafted
  input file are in scope.
- **Path-traversal or arbitrary-file-write bugs** in `speclance-cli`
  and `speclance-py`.
- **Supply-chain integrity** of published artifacts on crates.io and
  PyPI: tampered manifests, missing provenance, unsigned releases.

Out of scope:

- Denial of service via legitimately oversized vendor files. Mass-spec
  acquisitions can be hundreds of GB by design.
- Vulnerabilities in third-party crates with no demonstrated exploit
  path through this stack. Forward those upstream.
- Issues in vendor parsers themselves - those should be filed against
  the upstream repo. SpecLance delegates all vendor ingest to
  `openmassspec-io`, so parser bugs typically belong in
  [OpenMassSpec](https://github.com/Sigilweaver/OpenMassSpec) or the
  vendor-specific reader.

## Disclosure

We follow coordinated disclosure. Reporters are credited in the
release notes unless they prefer to remain anonymous. We aim to ship
a fix within 30 days of confirming a high or critical issue.
