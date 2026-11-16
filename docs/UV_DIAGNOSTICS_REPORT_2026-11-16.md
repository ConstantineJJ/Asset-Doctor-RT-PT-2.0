# Source UV diagnostics — 2026-11-16

Code checkpoint: `ff355f91acc4244a023feeecf7c66d33de6fb58f`.
Accepted contract: ADR-0025. Read-only behavior reference: SLICE_12_UV_DIAGNOSTICS_AUDIT.
This report covers independent source UV diagnostics, not UV editing or repairs.

## Behavior

Algorithm5 adds G009missing sourceUV0 information/count1per source primitive,
G010exact zero UV area warning per channel, G011outside closed0..1information per
channel. All admitted TEXCOORD_n channels are examined, including unused channels
and unreferenced vertices. Selected material UV2 cannot masquerade as sourceUV0.
Each primitive, including inactive data, is checked once; instances do not multiply
counts. No coordinate wrapping/normalization/repair; original source stays intact.

G010 uses existing robust adaptive orient2d on decoded f32, no epsilon. Repeated-
index faces are excluded/count retained once per primitive; distinct-index
collinear geometry can still have either valid or collapsed UV. Tiny nonzero and
mirrored maps remain nonzero. G011 is information because wrapping is allowed.

Fixed8channel stack summaries, no per-vertex/triangle UV working allocation.
Each finding keeps up to16typed source indices and the actual channel ID.
Multichannel output is admitted/built/appended atomically per primitive/rule;
allocation/finding refusal retains earlier data and marks omitted coverage.
Later clean or not-applicable primitives still complete. No channels gives
G010/G011N-A, distinct from checked/no findings. Report completeness requires
checked+N-A equals source scope, with no omissions. Cancel discards results;
cooperative checks inside scanning loops prevent partial tally publication.

## Verification

- 113 workspace tests PASS:35analysis/23desktop/16document/27formats/12renderer.
  First missingUV0/selectedUV2 test failed before implementation then passed.
  Tests cover all channels, inactive/instance scope, unused outside vertices,
  tiny/large/mirrored/collinear/collapsed maps, repeated indices, bounds0/1/-0,
  lowest16samples, exact8channel output/refusal, allocation refusal, later clean/
  N-A scopes and deterministic cancellation after partial vertex/triangle tallies.
- fmt check, all-target clippy warnings denied, Windows release build and
  cargo-deny advisories/bans/licenses/sources PASS. No dependency changes.
- UV_Checker:3source primitives/4triangles/9findings,6display triangles. Includes
  collapsedUV0, validUV2 with unused outside vertex3, missingUV0, noUV and instance.
  G009checked3/count2; G010checked2+N-A1/count1/excluded1; G011checked2+N-A1/count1.
  Bad_Unused_UV has NaN in an untextured channel and explicitly refuses replacement.
  Both2608byte numeric fixtures are project authored. All20manifest hashes/sizes
  match and generator reproduces exactly; no external/user assets vendored.
- Native UV,2Medge-limit,normal/emission,components and empty regressions PASS.
  Applicable runs check invalid replacement keeps CPU/GPU document,15mesh presents,
  resize/minimize/restore/native close;0validationerrors.2Mpartial retains4findings,
  edge checks omitted, G009checked, G010/G011N-A. Sync validation requested by child
  environment variable; effective layer setting not independently queried.
- Computer Use observed final compact full/partial UI: all11rule coverage rows,
  checked/N-A/omitted distinctions, G010TEXCOORD_0triangle[0], G011TEXCOORD_2vertex[3],
  missingUV0 and always-visible scope/footer. Coverage scroll expanded to panel
  width after a visual usability correction. Separate coverage/findings scroll IDs.
  Final full/partial windows closed normally with0validationerrors; capture kernel
  reset. Window-close logs/process checks confirm no test viewer remains.
- Real Tire:702558triangles,11rules complete147ms/2findings, no UV findings.
  Vanguard:1559826triangles,11rules complete338ms/3findings including32zero-UV-area
  triangles. Both originalSHA256unchanged; PBR/bad-unused-UV replacement/lifecycle
  smoke0validationerrors. Single observations, not benchmarks or peak-memory proof.
- Independent source-only review found no concrete blocker. Its mid-scan cancel
  coverage gap was closed with deterministic tests. Reviewer did not independently
  rerun the native/visual checks.

Local evidence remains ignored under artifacts/uv-*, release-import-smoke*uv* and
*edge-limit*. Source models, Legacy, Arc-XRay and screenshots are not redistributed.

## Remaining scope

No UV overlaps/islands/density, orientation/welding/full manifoldness, material/
skeleton/animation/profile parity, focus/filters/export-report/rescan/edit/repair
claimed. Current checks are exact over decoded source data, not quality scores.
Existing sorts are not internally interruptible; allocation admission does not
bound whole-process/VRAM peaks. User acceptance of earlier material examples is
not acceptance of every new diagnostic. Full plan: MASTER_PLAN_AND_STATUS_RU.md.

Next milestone: audit/contracts and fixtures for revision-checked commands,
Undo/Redo, static Save Copy/roundtrip and the first safe Heal vertical slice.
