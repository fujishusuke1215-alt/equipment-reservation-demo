# ADR 0002: Demo authentication, authorization, and reservation lifecycle

- Status: accepted
- Date: 2026-11-15

## Context

The portfolio should be easy to run locally while showing server-enforced access control and a readable reservation workflow. It must not imply that a selectable demo role is production authentication.

## Decision

- Local demo login selects one of two seeded accounts and creates an unpredictable, opaque server-side session. The browser receives a `HttpOnly`, `SameSite=Lax` cookie; reservation and administrator routes resolve the session against PostgreSQL.
- The login route is enabled only when `APP_ENV=development` and `DEMO_AUTH_ENABLED=true`. The production container defaults to `APP_ENV=production` with demo login disabled.
- Every protected handler checks the user role in the backend. Administrator-only catalog management, all-reservation listing, audit access, and lifecycle transitions are not trusted to frontend visibility.
- Reservation lifecycle is explicit: `pending → approved → checked_out → returned`, with `pending → rejected` and `pending|approved → cancelled` as terminal paths. Each transition locks the reservation row, validates the current state, updates its version, and records actor, timestamp, old state, and new state in one transaction.
- Members can cancel only their own unstarted reservation; administrators can cancel any pending or approved reservation. Only administrators can approve, reject, check out, or record a return.
- Errors use one `application/problem+json` envelope with a stable application error code.

## Consequences and limits

This is a development login, not identity proof: anyone with access to the local demo can choose either seeded role. A deployed system would need an identity provider, session rotation and revocation policy, secure cookies over HTTPS, CSRF defenses appropriate to its deployment, rate limiting, and operational account lifecycle controls. The demo's 12-hour database session is intentionally simple and is not a production session policy.

## Alternatives considered

- `x-demo-role` or a role stored only in browser state was rejected because a caller could forge administrator access.
- Hard-coded production passwords were rejected; this demo has no password flow.
- A generic status update endpoint was rejected because named transitions make business rules and invalid requests easier to inspect and test.
