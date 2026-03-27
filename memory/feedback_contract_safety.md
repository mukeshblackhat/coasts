---
name: contract-safety
description: Public API contracts must stay unchanged unless the ticket explicitly requires it — always verify all callers
type: feedback
---

Never change a public function's contract (signature, return type, behavior) unless the ticket explicitly asks for it. When a ticket does require a contract change (like replacing positional args with a struct), verify every caller:

1. Grep for all usages of the changed function across the entire codebase
2. Update every caller — don't miss any
3. Don't add or remove behavior at call sites — the caller should do the same thing as before, just with the new API
4. After updating, run `make lint` and `make test` to catch anything missed

**Why:** Changing contracts without checking all consumers breaks other services silently. User explicitly wants to avoid introducing regressions at call sites.

**How to apply:** Before implementing any contract change, grep for all usages first. After implementing, grep again to confirm zero remaining old-style calls.
