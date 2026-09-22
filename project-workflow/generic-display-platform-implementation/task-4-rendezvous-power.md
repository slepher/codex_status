# Task 4 — Rendezvous and PowerPlan (M4)

Device state machine (v2 §7):

```
DEEP --timer--> local clock/maintenance -> RENDEZVOUS: beacon, bounded wait
  |-- no response/timeout/NOOP/SLEEP -> radio off -> DEEP
  |-- small complete Data            -> apply, ACK -> radio off -> local display -> DEEP
  \-- formal PowerPlan(light)        -> BLE off -> bounded Wi-Fi -> WIFI_LIGHT

DEEP --BOOT--> provisional deadline t_boot+300s -> manual beacon -> bounded Wi-Fi -> WIFI_LIGHT
               (formal plan may shorten/keep/extend; keep sends remaining time)
WIFI_LIGHT --new formal PowerPlan--> update deadline/mode
WIFI_LIGHT --deadline/failure/low battery--> bounded teardown -> DEEP
```

- No short/long check classification, no device-side push/pull interpretation.
- Deep cannot be woken arbitrarily; first push latency bounded by rendezvous period.
- Only a new formal PowerPlan changes the light deadline. Data, reads, claim,
  owner renew and status queries never extend it.
- `plan_id` monotonic: same id+content idempotent (returns original result and
  remaining time), same id different content conflict, older id rejected.
- Deadlines use the device monotonic clock; time sync never moves them.
- Every exit path releases Wi-Fi, BLE and PM locks.
- BOOT provisional starts at the physical wake instant (BLE/Wi-Fi/time inside).
  Timer wake never gets 300 s. Bridge-unreachable → radio off by t_boot+300 s.

Bridge decision: NOOP/sleep, BLE small Data, request Wi-Fi light, send Bundle,
activate, full sync, hold/end light session. Verified by coordinator tests.
