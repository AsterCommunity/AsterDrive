# Authentication Flow State-Machine Contract

This contract defines the shared lifecycle boundary for primary login, second factors, account recovery, and sessions. MFA, Passkey, external-auth, contact-verification, invitation, and session modules continue to own their typed security payloads. The shared state machine owns only identity, state, expiry, attempt budgets, single-use behavior, and concurrency rules.

## Ownership

```text
HTTP route
  -> typed auth command
  -> auth domain transition guard
  -> owning service transaction
  -> repository conditional update / cache atomic take
  -> commit
  -> cookie, redirect, mail, and audit side effects
```

- `src/services/auth/flow/` defines `AuthFlowKind`, `AuthFlowState`, commands, snapshots, and transition rules.
- MFA, external-auth, local recovery, Passkey, and session services retain product guards, runtime policy checks, and side-effect ordering.
- Repositories only perform atomic conditional updates. `rows_affected == 0` or cache `take == None` is a no-result outcome that may represent conflict, replay, expiry, or cache eviction; services inspect authoritative fields and map the final state, while repositories do not choose UI behavior.
- Routes preserve the existing API envelope, cookie, and redirect contracts.

## Flow inventory

| Flow | Payload owner | Identity | Atomic advance | Terminal / cleanup |
| --- | --- | --- | --- | --- |
| Password primary | local auth service | request-local | password verification, then MFA/password-change/session result | request completion |
| MFA login | `mfa_login_flows` | `mfa-login:<id>` | transaction plus conditional attempt/consume | consumed/expired; runtime cleanup |
| Passkey login/registration | typed cache envelope | public flow UUID | cache atomic `take` | consumed/TTL eviction |
| External login | `external_auth_login_flows` | `external-login:<id>` | state plus browser-binding conditional consume | consumed/expired; runtime cleanup |
| External email recovery | `external_auth_email_verification_flows` | `external-recovery:<id>` | conditional email request/consume | consumed/expired; runtime cleanup |
| Registration/password reset/email change | `contact_verification_tokens` | `contact-verification:<id>` | transaction plus purpose-scoped single-use token | consumed/expired cleanup |
| Invitation | `user_invitations` | `invitation:<id>` | status-scoped conditional update | accepted/expired/revoked |
| Session | `auth_sessions` | session UUID | conditional refresh-JTI rotation | revoked/expired cleanup |

Identities use only database primary keys or public flow UUIDs. The request-local identity of a password primary is an explicit exception: it ends with the request and must not enter cross-request shared snapshots, logs, or errors. Raw tokens, provider state, browser bindings, verification codes, refresh JTIs, and their hashes never enter shared snapshots, logs, or errors.

## Transition rules

- Password primary may enter `SecondFactorPending`, `PasswordChangeRequired`, or `Authenticated`.
- Passkey and external primary flows advance through `FirstFactorPending -> Processing`, then MFA or authenticated. Local password policy does not block either factor.
- MFA advances only from `SecondFactorPending` to password-change or authenticated.
- Recovery advances through `RecoveryPending -> Processing -> Completed`.
- `Failed`, `Expired`, `Cancelled`, `Consumed`, `Completed`, and `Authenticated` are terminal.
- Revision conflicts are checked before terminal state. Except for explicit `Expire`, expiry is checked before cancel and ordinary transitions.
- Failure commands atomically increment attempts and enter `Failed` at the budget. Saturating arithmetic prevents counter wraparound.

## Policy and side effects

- Every cross-request advance rereads runtime auth policy. A UI snapshot captured at flow creation is not authorization evidence.
- Password-first MFA rechecks password-login policy during exchange; external-first MFA is unaffected by that switch.
- The session row is persisted in a transaction before the route emits cookies. Transaction failure emits no session cookie.
- Mail outbox, audit, and cache invalidation have an explicit pre-commit or post-commit position. Failures are not silently discarded.
- Frontend `AuthUiFlow` is the single frontend/UI projection of backend state. The URL adapter restores only an expiring flow reference and bounds TTL, methods, and local return paths.
- A generation-aware coordinator combines auth check and provider loading. An older generation, unmounted promise, or slower response never updates the active page.

## Test matrix

- Domain: every allowed transition, cross-context rejection, terminal replay, revision conflict, attempt boundaries, saturation, and expiry/cancel ordering.
- Repository/integration: single conditional consume, concurrent loser, no session on failure, runtime policy changes, and expired cleanup.
- Passkey cache: flow isolation, single `take`, TTL envelope, and registration/login kind isolation.
- Frontend: one top-level flow, URL recovery precedence, partial provider failure, stale generations, hostile return paths, TTL limits, unknown/duplicate methods, and query cleanup.

## Schema boundary

The typed tables already retain their security payload and authoritative lifecycle through `consumed_at`, `expires_at`, attempt counts, or status. Shared snapshots derive from those fields instead of creating a second `state` column. A typed table gains a revision only when a real multi-request CAS cannot be represented by its existing conditional fields. There is no universal auth JSON table.
