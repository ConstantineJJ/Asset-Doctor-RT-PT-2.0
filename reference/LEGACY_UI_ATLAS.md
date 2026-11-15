# Legacy UI atlas

Legacy HEAD `17ca3549767aac78bf4ad51fa1c254c90dd15113`, branch `work/import-limit-20261115`, dirty `clean`.

Source observation only; no Legacy code/tests/GUI executed. All native scenarios UNVERIFIED.

## Source layout

Toolbar → scene tree / Doctor viewport / inspector → animation timeline → status bar. Application menu opens settings/shortcuts. Inspector: Health, Summary, Materials, Rig, Performance. Compare is a separate manually opened view with its own slots, camera/animation controls and settings. Rig removal, fitting, Heal preview/report and topology tests have separate UI surfaces.

197 lexical button/input/select/dialog source sites are indexed in LEGACY_UI_SOURCE_SITES.csv, including dynamic template sites. This is NOT a count of visible controls. Label/callback hints are approximate and require semantic mapping. All sites remain UNREVIEWED_SOURCE_SITE until mapped to a catalog ID or excluded with reason.

110 state candidates are indexed in LEGACY_SETTINGS_MATRIX.csv; transient state is deliberately not presented as a confirmed user setting. Complete ranges/persistence require review.

Full UI audit gate is OPEN. No claim that every UI action has been reviewed. Native interaction, DPI, focus, mouse ownership and screenshot atlas remain pending.
