# File and data boundaries

Treat models/metadata as untrusted data. Validate container lengths, index/accessor
ranges, integer overflow, finite transforms, texture expansion and extension
requirements before expensive allocations or document publication. Do not execute
scripts or open URLs from imported assets or attached reports. No silent external
file/network fetching; external resource support needs explicit scoped grants.

Original inputs are immutable. Export uses a separate destination with explicit
overwrite semantics, cancellation, independent reimport checks and atomic writes
where supported. Errors keep the current document/history and never claim save.
Logs omit model contents, secrets and full user paths by default. Crash/recovery
storage has bounded retention and a different application ID from Legacy.

All format features here are requirements; no importer currently exists.
