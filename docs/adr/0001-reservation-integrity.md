# ADR 0001: Reservation integrity and repeatable requests

- Status: accepted
- Date: 2026-11-15

## Context

A reservation request can arrive at the same time as another request for the same individually tracked item. Checking availability in application code before writing is not sufficient: both requests can pass the check before either one commits. Clients can also retry a request after a timeout without knowing whether the first attempt committed.

## Decision

- Store reservation ranges as `TIMESTAMPTZ` and treat them as half-open intervals, `[starts_at, ends_at)`. Adjacent bookings are therefore valid.
- Add a PostgreSQL GiST exclusion constraint over `equipment_id` and `tstzrange(...)`, scoped to `pending`, `approved`, and `checked_out` rows. PostgreSQL arbitrates concurrent inserts; the API maps exclusion violation `23P01` to HTTP 409.
- Serialize reservation creation with equipment status changes by taking a shared lock on the equipment row. The administrator path takes an update lock and checks for open reservations before pausing or retiring an item.
- Require an `Idempotency-Key` per user. The reservation and its key record are written in one transaction. Reusing a key with the same request fingerprint returns the existing reservation; reusing it with different input returns 409.
- Write the initial audit event in the same transaction as the reservation and idempotency record.

## Consequences

The database, rather than an in-memory lock or a preflight query, remains the final authority under concurrency. PostgreSQL's `btree_gist` extension is required. This demo models one physical item per equipment row; quantity-based inventory would need a different allocation model and corresponding invariant. Idempotency records are retained for this demo and would need a documented retention policy at scale.

## Alternatives considered

- An application-level availability check alone was rejected because parallel requests can race.
- A per-equipment advisory lock could serialize requests, but the exclusion constraint is a declarative backstop that also protects writes from future code paths.
- A queue would smooth bursts, but it would not replace the database invariant and would add operational components without improving this small demo.
