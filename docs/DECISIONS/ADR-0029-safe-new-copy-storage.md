# ADR-0029 — New-path Save Copy storage

Status: ACCEPTED, 2026-11-16, internal Windows filesystem API only.
No dialog, native Save button, dirty-clear or overwrite feature approved here.

Introduce ad-storage: std filesystem IO over ad-formats VerifiedGlb and a current
ad-document origin/revision stamp. No rendering/UI/history ownership. Destination
must be a new absolute .glb path in an existing directory. Never open/truncate an
existing destination, including an alias of an original asset.

Create an exclusive staging file in the canonical destination directory, bounded
unique-name attempts. Windows handle permits reads but denies other write/delete
opens while staging. Write in cancellable chunks, sync_all, seek/read every byte
back against verified memory. Publish through a same-volume hard link that fails
if destination exists. No fallback that overwrites/truncates/copies into a visible
partial destination. Unsupported hard links/filesystems produce an explicit IO
failure before destination publication.

Publication is the commit point: before it cancellation/failure leaves no new
destination and removes only the owned staging file. After successful publication
report saved; do not turn it into cancellation/error due to late cancellation or
staging cleanup failure. Return a cleanup warning if a temporary link remains.
This is atomic no-replace visibility, not a claim of crash/power-loss durability
for directory metadata. Keep the file handle through publication.

Request DELETE access on the owned staging handle. Cleanup marks that handle's
link for deletion with SetFileInformationByHandle(FILE_DISPOSITION_INFO) before
closing it; never close and then delete by pathname. Disarm cleanup after its
single attempt. A failed deletion may leave the owned temporary link and must be
reported; Drop cannot later delete a replacement at its former name. Non-Windows
storage is explicitly unsupported until equivalent ownership guarantees exist.

Verify the proof against the current document before writing and before publish.
The synchronous API borrows the document immutably. Future background/UI integration
must bind receipts to selected revision and never clear newer edits as saved.
Receipt retains the private origin stamp too, including reused public tags; its
is_current checks document association, not future external file changes.
No saved-state mutation exists in command sessions yet. Proof/receipt constructors
are private; a verified memory buffer is distinct from a saved-copy receipt.

Acceptance: successful byte-exact saved copy/reimport, existing/original path
unchanged, invalid/stale/foreign/cancel refusal, partial-write/readback/publication
failures with no destination, staging cleanup, concurrent no-replace publication,
and actual target-volume support. Test files stay in owned workspace artifacts.

Primary references: [Rust Windows OpenOptionsExt](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html),
[Microsoft hard-link API](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createhardlinkw),
[SetFileInformationByHandle and DELETE access](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle),
[FILE_DISPOSITION_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_disposition_info).
Uses already locked windows-sys0.52.0 Foundation/FileSystem bindings. Guarantees
cover owned staging and no-replace publication in a stable destination directory;
adversarial replacement of directory ancestors/junctions is not certified here.
