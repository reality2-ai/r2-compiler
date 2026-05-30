# SPEC-APIARY-AMENDMENT-PROPOSAL: broaden R2-APIARY to encompass TG-bound multi-hive deployments

**Version:** 0.1 Draft
**Date:** 2026-05-31
**Status:** Proposal — for ratification + upstream merge into r2-specifications
**Author:** r2-compiler design session 02, Roy Davies
**Target spec:** `r2-specifications/specs/r2-core/R2-APIARY.md` (currently v0.3 Draft 2026-05-06)

---

## 1. Motivation

R2-APIARY's current scope (v0.3, R2-APIARY §1.1) defines an **apiary** as:

> "a single R2 hive whose protocol responsibilities are implemented by two or more cooperating processors physically bound together in a single device, sharing one identity, one trust group binding, and one set of key material."

R2-APIARY §1.1 itself acknowledges this is a **"deliberate slight stretch"** of the dictionary meaning. In ordinary usage, an apiary is a place that contains **one or more separate beehives**, each with its own queen and colony. R2 borrowed the term for the OPPOSITE concept: one colony spread across multiple boxes (Langstroth supers — one queen, multiple boxes).

r2-compiler's design has surfaced an unmet vocabulary need: the **TG-bound set of cooperating hives that together deliver one deployment**. The r2-workshop rocker rig is the canonical example — one TG, four role-ensembles (sensor + controller + viewer + keyholder), many hive instances. Currently this lacks a name in the R2 spec suite (workshop calls it informally "the deployment").

The proposal: **broaden R2-APIARY's scope** so an apiary is the broader TG-bound-set-of-stacks concept, with the existing multi-processor-single-identity case becoming a labeled specialisation ("tightly-bound apiary"). This:

- Returns the term to its dictionary meaning (a yard with many hives).
- Provides one canonical R2 term for the deployment-level concept r2-compiler needs.
- Preserves all current R2-APIARY reasoning under a more specific name.

## 2. Proposed scope

### 2.1 Top-level definition (replaces R2-APIARY §1.1)

> An **apiary** is a set of cooperating R2 protocol-stack instances bound together by shared trust group membership and a shared deployment purpose. The instances may be:
>
> - **Multi-device**: separate physical hives, each with its own external identity + key material, sharing only a TG binding + a class string. Each is a complete hive; the apiary is the cooperative whole.
> - **Tightly-bound** (the special case formerly called "apiary"): two or more processors physically bound in one enclosure, sharing one external identity, one TG binding, and one set of key material. Externally, a tightly-bound apiary IS one hive; internally, it is composed of *components*.
>
> Both cases share the defining property: a set of R2 stacks that **belong together** by shared TG membership. The packaging — separate devices vs co-located processors — varies; the cooperative-set abstraction is the same.

### 2.2 Apiary specialisations

| Specialisation | Physical binding | Identities | Key material | TG | Section |
|---|---|---|---|---|---|
| **Multi-device apiary** | None (separate enclosures) | One per hive | Per-hive keypair | One shared TG | new R2-APIARY §3 |
| **Tightly-bound apiary** | Physical (shared enclosure + power) | One (collapsed at the boundary) | One shared set | One TG binding | renamed R2-APIARY §2 (was §§1–11) |

A tightly-bound apiary is a specialisation of the broader concept: every tightly-bound apiary IS an apiary, but not every apiary is tightly-bound.

### 2.3 Why this generalisation works

R2-APIARY §1.1.1 argues for the single-external-identity property of the tightly-bound case on five grounds (trust groups already provide grouping; components share fate; components share keys; capability variation handled by R2-CAP; routing simplifies). **All five arguments remain valid for tightly-bound apiaries** — they are PROPERTIES of the specialisation, not of the definition.

The multi-device apiary case has different properties:

- **Distinct identities are appropriate** — separate physical devices have separate fates, separate failures, separate replacements.
- **Per-hive keys are appropriate** — devices may be acquired, enrolled, revoked, and retired independently.
- **One shared TG ties them together** — trust group membership is the cooperative binding; the apiary is the named abstraction over the cooperation.
- **One shared class string** is the protocol-visible signal that hives belong to the same deployment.

The reason the original R2-APIARY argued AGAINST per-component identity was specifically about physically co-located components — "Issuing N device identities for one device just adds correlated entries everywhere — N CAP entries to maintain, N beacon entries to deduplicate, N keys to revoke jointly." That reasoning does NOT extend to multi-device cooperation, where each device legitimately has its own fate.

## 3. Required spec changes

### 3.1 `r2-specifications/specs/r2-core/R2-APIARY.md`

1. **Header version bump**: 0.3 → 0.4. New date.
2. **§1.1 — replace with the broader definition** (§2.1 of this proposal). Move current §1.1 wording into a new §2 (Tightly-bound apiaries) without changing the technical content.
3. **§1.1.1** — retitle to "§2.1 Why a tightly-bound apiary has a single external identity". Content unchanged; just relocated.
4. **New §1.2** — "Apiary specialisations" — introduce the multi-device vs tightly-bound distinction (§2.2 of this proposal).
5. **New §3** — "Multi-device apiaries". Defines the cooperative-set semantics: how hives in a multi-device apiary discover each other (via TG membership), how they declare shared deployment purpose (via shared class string), how capability advertisement works at the apiary level (an apiary's aggregate capabilities = union of its members' capabilities, gated by which members are reachable).
6. **§1.4 (Terminology)** — add "Multi-device apiary" + "Tightly-bound apiary" definitions. Keep existing "Apiary / Component / Cohort / Bridge / Sentinel / MC / SBC" definitions; clarify that Component / Cohort / Bridge / Sentinel / MC / SBC apply specifically to tightly-bound apiaries (existing §5 + §6 + §7).
7. **§5 (Bridges) and §7 (Power states) and §11 (Security)** — clarify they describe properties of tightly-bound apiaries specifically.
8. **R2-HW §10 cross-reference** — R2-HW's multi-processor framing flows into tightly-bound apiaries unchanged.

### 3.2 Other r2-specifications files

- **`r2-specifications/specs/r2-core/R2-ENSEMBLE.md`** — gains a §"Apiary as a container for ensembles" subsection: an apiary contains 1+ role-ensembles, sharing the apiary's class string at the protocol level.
- **`r2-specifications/specs/r2-core/R2-INTRO.md`** — vocabulary list amended.
- **`r2-specifications/specs/r2-core/README.md`** — no change (file just indexes).

### 3.3 No spec changes required in

- R2-WIRE, R2-FNV, R2-CBOR, R2-BEACON, R2-TRUST, R2-DEF, R2-COMPILE, R2-PLUGIN — all unchanged.
- The protocol surface (events, frames, TG semantics) doesn't change. The amendment is purely vocabulary + organising principle.

## 4. r2-compiler usage post-amendment

Once the upstream amendment lands:

- r2-compiler uses the term **"apiary"** for the operator's deployment unit (replaces the placeholder "project" terminology used so far).
- Directory layout: `apiaries/<name>/` per [`SPEC-APIARY-LAYOUT.md`](SPEC-APIARY-LAYOUT.md) §2.
- `apiary.toml` schema per `SPEC-APIARY-LAYOUT.md` §3.
- Event vocabulary `r2.compiler.apiary.*` per `SPEC-APIARY-LAYOUT.md` §7.
- Webapp UI: project pill → "Apiary: rocker-rig".

The amendment does NOT block r2-compiler's adoption of the term — `SPEC-APIARY-LAYOUT.md` already uses it. If Roy decides against the upstream amendment, r2-compiler would need to either:

- Use a different word locally (project / deployment / federation / …), OR
- Continue using "apiary" with a noted scope-divergence from R2-APIARY (cleaner if amendment is just slow, awkward if rejected).

## 5. Open questions for the amendment

1. **What's the minimal protocol-visible signal that hives belong to the same multi-device apiary?** Candidate: shared class string in their R2-BEACON AD payloads (already supported). Stronger: an explicit "apiary id" field. The simpler answer (shared class string) probably suffices — operators already use class strings for trust-group-bound deployments.

2. **Apiary-level capability advertisement.** When a hive observes multiple peers in the same TG with the same class string, can it infer apiary membership? Should the apiary expose AGGREGATE capabilities (the union of its members') to non-member peers? Or is this an internal-to-the-TG concern only?

3. **Apiary as a parameter to `r2.dash.*` commands.** Today `r2.dash.cmd.*` commands are scoped per-device. Should there be an `r2.apiary.cmd.*` family that broadcasts to all members of one role-ensemble within an apiary?

These are spec-amendment design questions, not r2-compiler blockers. r2-compiler can use "apiary" today and the spec answers shape the protocol-level integration over time.

## 6. Ratification path

1. Roy reviews this proposal.
2. Apply the §3.1 amendments to `r2-specifications/specs/r2-core/R2-APIARY.md` (version bump 0.3 → 0.4).
3. Update R2-ENSEMBLE + R2-INTRO per §3.2.
4. Notify r2-compiler that the upstream amendment has landed (just a sync run will pick it up if any vendored crates reference the spec; otherwise r2-compiler's local specs already use the term).
5. Sweep r2-workshop's documentation to use "apiary" where it currently says "deployment" or "the four role-ensembles".

## 7. Backwards compatibility

The amendment does NOT change the protocol surface — it only broadens vocabulary. Existing tightly-bound apiaries (per R2-APIARY v0.3) remain valid; they're now identified as the specialisation. Existing r2-workshop documentation that uses "deployment" informally remains accurate; the formal term "apiary" can be adopted incrementally.

## 8. Change log

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1 | Initial draft of the amendment proposal. Authored in r2-compiler design session 02 alongside `SPEC-APIARY-LAYOUT.md`. Awaits Roy's ratification + upstream merge. |
