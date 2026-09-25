# task-2 — Larger stored quad template

## Objective and evidence
Replace fixed quad layout with JSON-controlled layout after one capability OTA, retain full/mini compatibility and wireless pairing. HEAD0a30c58, ROM0.4.2 battery wireless gate passed. Sol /root/delivery_plan planned this task. Current text fixedfont/x/y; TplEnv lacks battery; builtin findWindow scans allbuckets; WiFi signature skips arbitrary bindings and retaining unchanged latestusage. Existing preview uses inventedfont unrelated to firmware.

## Ownership
src/template_engine.cpp, src/template_engine.h, src/main.cpp; bridge/crates/core/src/template.rs and tests/template.rs; new tools/test-bridge/templates/quad.json; tools/generate-quad-preview.mjs; optional focused tools/test-quad-preview.mjs. Generated ignored artifacts/ previews allowed. No deletions unless removing newly obsolete local helper code insideownedfiles. No newdependencies, partitions, transport, Tauri, ACKredesign. full.json/mini.json exactbytes unchanged. Dispatcher ownsdocs/deployment/commit. Otherworker edits must be preserved.

## Accepted implementation
1. Schema1 backwards compatible optional text scale integer1..3, region=[x,y,w,h] fullyinside200x200, align=left/center/right onlywithregion. Region text verticallycentered; scale decreases tofit then truncates at1; ASCII-safe clippeddrawing fromexistingfonttables. Legacy absentfields sameappearance. Newfields type/rangevalidated equally infirmwaredryrun andRust.
2. device.battery binding and TplEnv int unknown=-1, existingADCsource; bounded epoch format default MM-DD HH:MM plus optional hhmm onlyfor epochbinds (server_time/reset timestamps), exactenumchosenanddocumented. No unboundedstrftime input.
3. quad.json idquad version1 min_fw0.5 (version comparator currently major/minor). Firmware0.5.0-bw. Diagonalblackrectangles [4,4,104,90] and [92,106,104,90], innerpadding; white hero digits explicit buckets[codex].weekly.remaining and buckets[codex].5h.remaining, absent -- neverinfinity. Fonts/scales chosen tofit 0,99,100,-- (f20scale3 iffits). Plan top-right; compact one-line reset MM-DD HH:MM; label/RC top-right; 5hreset/battery/sync bottom-left. Preserve readable spacing outsideblocks.
4. Builtinfallback fixedcodexbucket and -- absent5h. No newquad-specificrenderer; storedquad usesgenericengine.
5. Alwaysretain latestvalidusage, render onreceivedusage and bounded minute/battery check. Compare framebufferwithlastdisplayedbytes toskipidentical EPDwrites. Updatecomparisonfor everydisplaypath so pairingoverlay restoredproperly; preserve pairingoverlay precedence anddatafullrefresh. Useoneadditional5000bytebuffer, fail safely ifallocationfails. No stalehardcodedusageSig gate. Need straightforwardfunctionflow notnewframework.
6. Preview reads quadJSON and actualfirmwarefonttablebytes/metrics; deterministicfixture outputs normal/100/missing5h/longnames inartifacts/. No independenthardcodedquadlayout. Tests assert bounds,exactpalette,one-linedate,correctmissingbucket,fit,changedvisiblepixels vsidenticalpixels. Runtimefirmware refreshwiring sourceinspection pluspostOTArealtest; don't claimNodepixelmodel executesfirmware C++.

## Coding Self-Tests
Root: PlatformIO run via C:/Users/user/.platformio/penv/Scripts/platformio.exe (normaluser escalation allowedexistingcache only), node tools/generate-quad-preview.mjs, node tools/test-quad-preview.mjs ifcreated, git diff --check. bridge/: C:/Users/user/.cargo/bin/cargo.exe test --workspace --offline. Worker mustrunall directly; captureexitcountsandRAM/flash. Rusttests legacyhashes c1a2faaf/e6ba459e,validquad,invalidscale/region/align/timeformat incltypes/null/floats. Previewtests explicitcodexmissing5h whileSparkpresent; longplan/labels;100;boundaries. Nohardwareactions.

## Independent Verification
SeparateLunarunner repeats exactbuild/cargo/previewtests/diffchecks andinspectsparity,hashes,framecomparisonallpaths,latestusage retention,minute/battery trigger,overlay precedence. Root visuallyinspectsPNG. Solreview mandatoryafterbothlayers.

## Completion/stop
No materialreviewfinding; previewlargeblocksandmissing--correct; compile/tests pass. Commit Render larger quad layout from templates. DispatcherthenWiFiOTA0.5.0,rebuiltRustbridge,storedquadpush positiveACK anddeviceinfohash. DemonstrateJSONupdatewithunchangedROM. Stop andreportifnewdependency/protocol/schema orbroaderfilesneeded. Nevererase/resetbond orflashworker. Userauthorizednative roles despiteunavailableruntimemetadata. Nochilddelegation.

## Live observability addendum
Existing handleStatus diagnosticHTML may expose actual display update count (increment only on physical EPD write) and rendered active template id/hash. No newendpoint or protocol. Allows USB-disconnected verification of framebuffer skip; dispatcher authorized withinmain.cpp scope.

## User correction — 5h presence (authoritative)
User clarified the temporary weekly wording was a mistake: weekly always exists; this condition applies ONLY to Codex5h. If buckets[codex].5h exists, show its remaining number and 5h reset time. If absent, show actual infinity bitmap in lower-right hero and hide the 5h reset line entirely. Never substitute another bucket's5h. Weekly stays normal with reset time.
Add generic optional when:{bind:<knownbinding>,exists:<boolean>} on elements, validated identically in firmware/Rust and interpreted in preview. Omittedwhen unchanged. Presence examines actualselectedbucket/window/field, not formatted--or numerictruthiness. Guard5htext+reset exists:true; staticicon infinity usingexistingiconprimitive exists:false. Noquad-specificrenderer orINFtext. Testpresent5h,missingcodex5hwhileSpark5hexists,infinitypixels+noresetline,invalidwhenobject/bind/boolean andlegacyhashes. This overrides all earlier absent5h-- clauses and preliminaryweeklycorrection. Sameownedpaths.

## Clock inspection correction
Existing startNormalMode already calls configTzTime(CST-8,pool.ntp.org). Dispatcher initialsearchmissedconfigTzTime; no clockinitialization defect established. PreserveexistingNTP/UTC+8 setup, no newclock synchronization scope. Preview usesUTC+8. Minute/battery redrawcheckstillrequired.

## Latest user revision — absent5h displays100 (authoritative)
User replaces infinity with numeric100. Missing buckets[codex].5h => static text100 in the same hero region, guarded when.exists=false; hide5h reset time. Present5h => actualremaining andreset. Weekly unchanged. Remove infinityicon fromquadJSON; do not generatebitmap. Retain generic when semantics and updatepreview/tests toexpect100 andnoremainingresetline despiteSpark5h. This supersedes all infinity requirements above.

## Latest user revision — username label and zero reset credits (authoritative)
Bridge label is the Codex account username (app-server `account/read` email local part, ASCII-safe). No hostname fallback: when unavailable the label is null and the quad label row hides via `when.exists`; remove the `LABEL ` prefix. `availableCount<=0` omits the resetCredits object from the envelope; the quad RC row is guarded by `when.exists` and hidden at zero. Bridge/Rust/preview-only changes; delivered by template hash update after the 0.5.0 capability ROM, no firmware change.
