# Core emission verification — 2026-11-15

PASS: core emissiveFactor linear RGB defaults to0, embedded PNG/JPEG emissive RGB
samples SRGB with independent UV. Texture alpha ignored; factor-only uses white.
Source resources admitted in every view even factor0. Missing/bad resources fail
without publishing a replacement. Source bytes retained. Optional strength
extension is noted unsupported; required extensions remain refused.

PASS: Vulkan contribution adds before exposure/Reinhard in PBR independently of
key/fill/AO/base sampling. Base MASK controls coverage. Independent emission
switch survives lighting reset. Resource layout remains2sets,frame96/push64;
vertex streams96bytes; tuple4096/image256/side8192/full-mip256MiB caps unchanged.

PASS:64workspace tests; fmt, all-target clippy warnings denied, Windows release
workspace build, dependency advisory/bans/licenses/sources checks. Source-only
CPU/GPU reviews found no concrete blocker. New numeric fixtures cover SRGB sharing,
UV2, factor-only/factor0, MASK and malformed emission-only PNG.

PASS: native emission on/off/default smoke presents15meshframes each, emission15/0/0,
2unique material binding tuples,1CPU image/2sampled formats/40mipbytes. Rejected
replacement preserves CPU/GPU model. Resize/minimize/restore/native close, plus
AO/normal/colorMASK/empty regressions:0validationerrors. Import runs requested child
VK_LAYER_VALIDATE_SYNC=1; effective setting not separately queried.

PASS: local heavy Vanguard probe852516vertices/1559826triangles,4images,
223696208mipbytes,15emissive frames and0validationerrors; source hash unchanged.
The added emission image fits existing cap. No user assets copied into repository.

PASS (qualitative): native window at key0/fill0 shows textured and factor-only
emission; factor0 stays dark. Switch off darkens all, switch on restores. Turning
base texture sampling off retains emission and removes texture-driven MASK holes.
App closed normally,0validationerrors. Tool-displayed captures only, no committed
screenshots. Logs under ignored artifacts: emission-ui-visual.*, vanguard-emission-
smoke.*, release-import-smoke*emission*.*.

WARN/unverified: calibrated pixel/channel/LOD equivalence, peak RAM/VRAM,
allocation/device-loss injection, full material extensions. No bloom/GI, scene
lighting from emission, HDR environment/shadows, BLEND, rig/animation, diagnostics,
editing/export, RT/PT. Compilation and smoke are separate from visual evidence.

No dependency changes, license-policy exceptions, remote publication or Legacy/
supplied Arc-XRay report modifications.
