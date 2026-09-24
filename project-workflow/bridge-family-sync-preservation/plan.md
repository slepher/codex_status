# Preserve device sync policy during family template publish

## Cause

The 1.54 family draft migrated with `sync_enabled:false` and has no family sync control. Template-tab `confirmFamilyPublish()` copies that value into the target device Profile before publishing. This can silently override the device's data-sync setting; it also copies the family's default full-sync interval.

## Decision

A family publish edits template selection and bindings for the chosen device. Preserve the target device Profile's existing `sync_enabled` and `full_sync_s` values when constructing the new Profile. For an unconfigured target, keep the platform defaults. The device page remains the explicit editor for data-sync policy. Do not change running Bridge data or start a publish.

## Check

Verify the browser script parses and run a focused publish-flow check for both sync on and sync off target devices. Review the diff and `git diff --check`. No Bridge rebuild/restart or device write in this task.
