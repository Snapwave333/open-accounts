"""Fanvue compliance: the research agent and the hard gate that keeps the account alive.

The in-app agents (NVIDIA NIM via ``nim.py``) research Fanvue's published policies — Community
Guidelines, Acceptable Use Policy, Creator Terms, Marketing Policy, API Access & Usage Policy,
Content Moderation & Protection Policy, Creator Earnings & Payouts — kept as raw snapshots in
``character/fanvue/raw/`` (legal.fanvue.com serves agent-friendly ``.md`` versions), and distill
them into the machine-enforceable ruleset in ``character/fanvue/policy.json``. Every caption,
message, bio, prompt and public-media asset Nova would put on Fanvue is audited against that
ruleset BEFORE anything is generated or published. The account survives by never testing
Fanvue's moderators.

The weekly ``Nova fanvue policy`` autonomy job (daily cadence, 7-day internal staleness gate)
re-fetches the raw documents, re-runs the research pass through NIM, merges verified changes and
writes a human-readable ``review.md``. Everything fails closed: a fetch or NIM failure keeps the
existing policy and reports the error; a missing or broken policy file makes every audit BLOCK
rather than allow.
"""
import datetime as dt
import json
import re
import urllib.error
import urllib.request
from pathlib import Path

from . import character, config, nim, state

POLICY_DIR = config.ROOT / "character" / "fanvue"
RAW_DIR = POLICY_DIR / "raw"
POLICY_JSON = POLICY_DIR / "policy.json"
REVIEW_MD = POLICY_DIR / "review.md"
SOURCES_JSON = RAW_DIR / "sources.json"

FANVUE_LEGAL = "https://legal.fanvue.com"
# legal.fanvue.com serves clean markdown of any page at "<page>.md" and an index at /llms.txt.
DOCS = {
    "community-guidelines": f"{FANVUE_LEGAL}/community-guidelines.md",
    "acceptable-use-policy": f"{FANVUE_LEGAL}/acceptable-use-policy.md",
    "creator-terms": f"{FANVUE_LEGAL}/creator-terms.md",
    "marketing-policy": f"{FANVUE_LEGAL}/marketing-policy.md",
    "api-policy": f"{FANVUE_LEGAL}/api-policy.md",
    "content-moderation": f"{FANVUE_LEGAL}/content-moderation.md",
    "creator-earnings-payouts": f"{FANVUE_LEGAL}/creator-earnings-payouts.md",
}
DOC_MARK = "Fanvue"  # every fetched page must at least mention the platform (catches HTML error pages)

SURFACES = ("caption", "message", "bio", "comment", "promo", "prompt")
# Rule ids that must always exist in a merged policy: the agent may reword them, never drop them.
CORE_RULE_IDS = (
    "minors_absolutely", "age_play_banned", "no_real_person_likeness", "no_ai_denial",
    "external_payments", "platform_migration", "in_person_solicitation", "extreme_themes",
    "explicit_text_house_rule", "weapons_drugs", "hate_violence_selfharm", "misleading_commerce", "pii_leak",
)

# ---- loading -----------------------------------------------------------------

def load() -> dict:
    """The distilled ruleset. Raises FileNotFoundError when missing (callers fail closed)."""
    return json.loads(POLICY_JSON.read_text(encoding="utf8"))


def _missing_policy_violation() -> list[str]:
    return [
        "fanvue policy file missing or unreadable - run `py pipeline.py fanvue-policy` first; "
        "nothing may be published to Fanvue until the compliance rules are distilled and audited"
    ]


def documents() -> list[dict]:
    """The raw-corpus manifest: one entry per policy document with URL and fetch date."""
    return state.read(SOURCES_JSON, {"documents": {}}).get("documents", {})


def _age_days(iso_str: str | None) -> float | None:
    if not iso_str:
        return None
    try:
        parsed = dt.datetime.fromisoformat(iso_str)
    except ValueError:
        return None
    if parsed.tzinfo is None:  # older baselines were written naive-local
        parsed = parsed.astimezone()
    return (state.now() - parsed).total_seconds() / 86400


def status() -> dict:
    """For `pipeline.py status`: policy freshness, corpus state, last review."""
    try:
        pol = load()
    except (OSError, json.JSONDecodeError):
        return {"policy": "MISSING (fail closed - run `py pipeline.py fanvue-policy`)", "attention": needs_attention()}
    docs = documents()
    return {
        "policy": "loaded",
        "rules": len(pol.get("text_rules", [])),
        "reviewed_at": pol.get("meta", {}).get("reviewed_at"),
        "age_days": _age_days(pol.get("meta", {}).get("reviewed_at")),
        "raw_docs": {name: (m.get("updated") or f"{m.get('bytes', 0)} bytes @ {m.get('fetched_at', '?')}") for name, m in sorted(docs.items())},
        "review": REVIEW_MD.name if REVIEW_MD.is_file() else None,
        "attention": needs_attention(),
    }


# ---- deterministic audit engine ----------------------------------------------

def _rule_hits(rule: dict, text: str) -> list[str]:
    hits: list[str] = []
    lowered = text.lower()
    for term in rule.get("match", []):
        if term and term.lower() in lowered:
            hits.append(term)
    for pattern in rule.get("regex", []):
        try:
            if re.search(pattern, text, flags=re.IGNORECASE):
                hits.append(f"/{pattern}/")
        except re.error:
            continue  # a broken agent-supplied pattern never blocks, the rule id still reports
    return hits


def audit_text(text: str, kind: str = "caption") -> list[str]:
    """Deterministic violations for one piece of Fanvue text (kind: caption, message, bio,
    comment, promo). Fail closed: missing/unreadable policy blocks everything."""
    if not text or not text.strip():
        return []
    try:
        pol = load()
        rules = pol.get("text_rules") or []
        if not rules:
            return _missing_policy_violation()
    except (OSError, json.JSONDecodeError):
        return _missing_policy_violation()
    violations: list[str] = []
    for rule in rules:
        surfaces = rule.get("surfaces", [])
        if kind not in surfaces:
            continue
        hits = _rule_hits(rule, text)
        if hits:
            violations.append(
                f"{rule.get('id', 'unnamed')}: {hits[0]}{' (+' + str(len(hits) - 1) + ' more)' if len(hits) > 1 else ''} "
                f"[{rule.get('reason', '')[:160]}] (source: {rule.get('source', '?')})"
            )
    name_hits = _fan_cosplay_hits(text)
    if name_hits:
        violations.append(
            f"ip_safe_cosplay: fan-cosplay name '{name_hits[0]}'"
            f"{' (+' + str(len(name_hits) - 1) + ' more)' if len(name_hits) > 1 else ''} - Fanvue is a paid "
            "surface: cosplay must be an IP-safe parody that suggests the look without naming the character "
            "or franchise [Fanvue CG A.1.2 (IP); Eli 2026-11-03]"
        )
    return violations


def prompt_gate(prompt: str) -> list[str]:
    """Fanvue-side prompt checks (complements character.check_prompt's house rules)."""
    return audit_text(prompt, kind="prompt")


# ---- built-in dynamic rules (code, not policy.json) ---------------------------
#
# IP-safe parody: cosplay goes to Fanvue, but the character's name and franchise
# must never appear there (Eli, 2026-11-03). The banned-name list is derived from
# character.json's fan-cosplay pool so `addcosplay` extends it automatically.

def _fan_cosplay_hits(text: str) -> list[str]:
    try:
        patterns = character.fan_cosplay_name_patterns()
    except Exception:  # spec unreadable: skip the dynamic rule, static rules still apply
        return []
    lowered = text.lower()
    return [p for p in patterns if re.search(rf"\b{re.escape(p)}\b", lowered)]


# Fanvue's "Intimate" definition includes wearing only lingerie/underwear/see-through
# clothing (CG Definitions). Over-labeling is harmless (an extra 18+ marker); under-
# labeling is a violation - so the keyword net is intentionally wide.
INTIMATE_OUTFIT_WORDS = ("lingerie", "lace slip", "bralette", "underwear", "see-through",
                         "sheer", "bodysuit", "swimsuit", "bikini", "swimwear", "lace bodysuit",
                         "silk robe over a")


def content_class_for_text(text: str) -> str:
    """'intimate' when the styling wording falls under Fanvue's Intimate definition."""
    lowered = (text or "").lower()
    return "intimate" if any(w in lowered for w in INTIMATE_OUTFIT_WORDS) else "sfw"


def check_caption(caption: str, content_class: str = "sfw") -> list[str]:
    """Full caption gate: banned text plus the REQUIRED disclosures.

    content_class: "sfw" (public-safe styling) or "intimate" (Fanvue's definition covers
    lingerie/underwear/see-through - such posts must carry an 18+ marker on top of the
    AI disclosure)."""
    violations = audit_text(caption, kind="caption")
    try:
        required = load().get("required", {})
    except (OSError, json.JSONDecodeError):
        return _missing_policy_violation()
    terms = required.get("ai_disclosure_terms", ["AI-generated"])
    if not any(t.lower() in caption.lower() for t in terms):
        violations.append("missing_ai_disclosure: every AI-generated post needs a clear AI disclosure "
                          "[CG D.1.1, AUP 2.3.1.4, API 4.3.1]")
    if content_class == "intimate":
        markers = required.get("intimate_18plus_marker", ["18+"])
        if not any(m.lower() in caption.lower() for m in markers):
            violations.append("missing_18plus_marker: Fanvue counts lingerie/underwear/see-through as "
                              "Intimate content, which must be clearly marked 18+ [CG C.6.1, CG Definitions]")
    return violations


def check_bio(bio: str) -> list[str]:
    violations = audit_text(bio, kind="bio")
    try:
        required = load().get("required", {})
    except (OSError, json.JSONDecodeError):
        return _missing_policy_violation()
    terms = required.get("ai_disclosure_terms", ["AI-generated"])
    if not any(t.lower() in bio.lower() for t in terms):
        violations.append("bio_missing_ai_disclosure: the bio must state the account is AI-generated "
                          "[CG D.1.1, AUP 2.3.1.4]")
    return violations


def public_media_gate(kind: str, text: str = "") -> list[str]:
    """Profile picture / banner / intro video / public bio are pre-subscribe surfaces with the
    strictest standard (SFW, no implied nudity, AI disclosure). Text gates the bio/banner copy;
    the visual side is judged by QC with the `public_sfw` mode before upload."""
    pol = load()  # raises when missing: fail closed
    del pol
    violations = []
    if kind not in ("profile_picture", "banner", "intro_video", "public_bio"):
        violations.append(f"unknown public-media kind: {kind}")
    if text:
        violations += audit_text(text, kind="bio")
    return violations


def prop_blocked(prop: str) -> str | None:
    """A theme prop word that Fanvue's rules disallow (weapons). Returns the matched pattern."""
    try:
        patterns = load().get("theme_rules", {}).get("strip_props_matching", [])
    except (OSError, json.JSONDecodeError):
        patterns = []
    for pattern in patterns:
        try:
            if re.search(pattern, prop, flags=re.IGNORECASE):
                return pattern
        except re.error:
            continue
    return None


def adapt_prompt_for_fanvue(prompt: str, prop: str | None = None) -> tuple[str, list[str]]:
    """Returns (prompt, removed_notes) with weapon-prop clauses stripped for the Fanvue surface.
    The prompt text itself is NOT modified beyond prop removal - banned words make the prompt
    fail the gate instead of being silently rewritten."""
    notes: list[str] = []
    out = prompt
    if prop:
        pattern = prop_blocked(prop)
        if pattern:
            # the clause character.image_prompt appends: "wearing the outfit above, holding or with prop: ..."
            out = re.sub(r",?\s*(holding|with prop|carrying)[^,]*", "", out, flags=re.IGNORECASE).strip(" ,")
            notes.append(f"prop stripped for Fanvue (matched /{pattern}/): {prop[:80]}")
    return out, notes


def gate_slot(slot: dict, caption: str, content_class: str | None = None) -> tuple[bool, list[str]]:
    """The one pre-publish check a Fanvue publisher performs on a calendar slot:
    caption + required disclosures + per-asset prompt review. Blocking means DO NOT POST.
    The content class defaults to the theme's own outfit wording (Fanvue counts lingerie/
    underwear/see-through as Intimate, which must carry an 18+ marker)."""
    if content_class is None:
        try:
            spec_ = character.spec()
            theme = character.theme(spec_, slot.get("pillar", ""), slot.get("theme", ""))
            content_class = content_class_for_text(f"{theme.get('outfit', '')} {caption}")
        except Exception:
            content_class = content_class_for_text(caption)
    violations = list(check_caption(caption, content_class))
    for asset in slot.get("assets", []):
        violations += [f"asset {asset.get('id', '?')} prompt: {v}" for v in prompt_gate(asset.get("prompt", ""))]
    return (not violations), violations


# ---- raw corpus --------------------------------------------------------------

def _fetch(url: str, timeout: int = 30) -> str:
    req = urllib.request.Request(url, headers={"User-Agent": "OpenAccounts-Nova/1.0 (compliance agent)"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        text = r.read().decode("utf8", "replace")
    if DOC_MARK not in text or len(text) < 2_000:
        raise ValueError(f"{url} did not return the expected policy document ({len(text)} bytes)")
    return text


def _doc_updated(text: str) -> str | None:
    m = re.search(r"Updated:\s*([^\n*]+)", text)
    return m.group(1).strip() if m else None


def fetch_raw() -> dict:
    """Downloads every policy document into raw/ and updates sources.json. Raises on failure;
    existing snapshots stay untouched when a fetch fails midway. The .md export carries no
    'Updated:' line, so change detection uses content hashes."""
    import hashlib

    manifest = documents()
    fetched = {}
    for name, url in DOCS.items():
        text = _fetch(url)
        dest = RAW_DIR / f"{name}.md"
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(text, encoding="utf8")
        digest = hashlib.sha256(text.encode("utf8")).hexdigest()
        changed = name in manifest and manifest[name].get("sha256") not in (None, digest)
        fetched[name] = {"url": url, "fetched_at": state.iso(), "updated": _doc_updated(text),
                         "bytes": len(text), "sha256": digest, "changed_since_last": changed}
        state.log(f"fanvue-policy: fetched {name} ({len(text)} bytes"
                  f"{', CONTENT CHANGED' if changed else ''})")
    manifest.update(fetched)
    state.write_atomic(SOURCES_JSON, {"documents": manifest})
    return manifest


def _corpus_docs() -> list[tuple[str, str]]:
    """The research corpus as (doc_name, text) pairs. Raises when snapshots are missing."""
    docs = []
    have = documents()
    for name, url in DOCS.items():
        path = RAW_DIR / f"{name}.md"
        if not path.is_file():
            raise FileNotFoundError(f"raw policy document missing: {path} (run with network access to fetch)")
        docs.append((name, path.read_text(encoding="utf8")))
    return docs


# ---- the agent research pass --------------------------------------------------

RESEARCH_SYSTEM = """You are the compliance researcher for Nova Vexley, a fully fictional AI-generated adult
cosplay persona (age 24, always disclosed as AI) operated on Fanvue by a human account owner through
Fanvue's official API. You are given ONE current Fanvue policy document and the current machine rules
derived from it.

Your job: verify every existing rule assigned to this document against it, add rules the document
requires that are missing, and drop nothing that is real. Cite the document section for every rule
(e.g. "CG C.6.3", "AUP 2.6"). Rules must be enforceable by keyword or regular expression where possible.
surfaces values: caption, message, bio, comment, promo, prompt.
Never soften the minors/zero-tolerance rules, the AI-disclosure rules, or the off-platform-payment rules.
Be terse: think briefly, then reply with the JSON only - at most the 12 most important rules for this
document, keywords only (no essays in reasons). Also list required_actions: concrete things the
operator must do or change to stay compliant (max 4).

Reply with ONE JSON object with keys: text_rules (array, same schema as the current rules: id, match,
regex, reason, source, surfaces - include ONLY rules grounded in THIS document), required_actions
(array of strings), notes (string). Do not invent policies that are not in the document."""

DOC_CITATIONS = {
    "community-guidelines": "CG",
    "acceptable-use-policy": "AUP",
    "creator-terms": "CT",
    "marketing-policy": "MP",
    "api-policy": "API",
    "content-moderation": "CM",
    "creator-earnings-payouts": "CEP",
}
DOC_PROMPT_CAP = 28000
HOUSE_RULE_IDS = {"explicit_text_house_rule"}  # grounded in identity.md, not in any Fanvue doc


def _rules_for_doc(rules: list[dict], doc: str) -> list[dict]:
    """Current rules whose source cites this document, plus fixed house rules."""
    cite = DOC_CITATIONS.get(doc)
    out = [r for r in rules if r.get("id") in HOUSE_RULE_IDS]
    if cite:
        out += [r for r in rules if f" {cite} " in f" {r.get('source', '')} "
                or r.get("source", "").startswith(f"{cite} ")
                or r.get("source", "").endswith(f" {cite}")]
    # first distillation cites several docs; when nothing matches, send nothing and let the
    # agent extract fresh rules for this document
    return out


def _doc_findings(doc: str, text: str, rules_subset: list[dict]) -> dict:
    """One NIM research pass over a single policy document. Raises NimUnavailable on failure."""
    user = (
        "CURRENT MACHINE RULES FOR THIS DOCUMENT (may be a first-draft subset):\n" +
        json.dumps(rules_subset, ensure_ascii=False) +
        f"\n\nPOLICY DOCUMENT: {doc}\n\n" + text[:DOC_PROMPT_CAP]
    )
    out = nim.chat_json(RESEARCH_SYSTEM, user, max_tokens=16000, temperature=0.2)
    if not isinstance(out, dict) or "text_rules" not in out:
        raise ValueError(f"research reply for {doc} had unexpected shape: {json.dumps(out)[:200]}")
    return out


def _research_via_nim(docs: list[tuple[str, str]], current: dict) -> dict:
    """Per-document NIM passes verify rules against the real documents; the merge itself is
    deterministic in code (see _merge) so the cycle can never fail on a final LLM call."""
    findings = []
    failures = []
    for doc, text in docs:
        subset = _rules_for_doc(current.get("text_rules", []), doc)
        try:
            result = _doc_findings(doc, text, subset)
        except nim.NimUnavailable as e:
            failures.append(f"{doc}: {e}")
            state.log(f"fanvue-policy: research pass for {doc} failed, skipped: {e}")
            continue
        findings.append({"doc": doc, "text_rules": result.get("text_rules", []),
                         "required_actions": result.get("required_actions", []),
                         "notes": result.get("notes", "")})
        state.log(f"fanvue-policy: research pass for {doc}: "
                  f"{len(result.get('text_rules', []))} rule(s), "
                  f"{len(result.get('required_actions', []))} action(s)")
    if len(findings) < 3:
        raise nim.NimUnavailable(f"only {len(findings)}/{len(docs)} document passes succeeded; failures: "
                                 + "; ".join(failures)[:400])
    # deterministic combine: first occurrence of each rule id wins (per-doc passes were
    # scoped to their own document, so id collisions are rare and benign)
    combined: list[dict] = []
    seen: set[str] = set()
    for f in findings:
        for rule in f["text_rules"]:
            rid = str(rule.get("id") or "")
            if rid and rid not in seen:
                seen.add(rid)
                combined.append(rule)
    actions: list[str] = []
    for f in findings:
        for action in f["required_actions"]:
            if action and action not in actions:
                actions.append(action)
    notes = "\n\n".join(f"[{f['doc']}] {f['notes']}" for f in findings if f.get("notes"))
    return {"text_rules": combined, "required_actions": actions[:8], "review_notes": notes}


def _validate_rule(rule: dict, i: int) -> dict | None:
    """Keeps only well-formed rules; a malformed agent rule is dropped (fail closed per rule)."""
    if not isinstance(rule, dict) or not rule.get("id"):
        return None
    match_raw = rule.get("match") or []
    regex_raw = rule.get("regex") or []
    surfaces_raw = rule.get("surfaces") or []
    cleaned = {
        "id": str(rule["id"]),
        "match": [str(m) for m in match_raw if str(m).strip()],
        "regex": [str(r) for r in regex_raw if str(r).strip()],
        "reason": str(rule.get("reason") or "")[:400],
        "source": str(rule.get("source") or "")[:200],
        "surfaces": [s for s in surfaces_raw if s in SURFACES] or list(SURFACES),
    }
    # A rule with no matchers and no regex blocks nothing; keep it only if it cites something.
    if not cleaned["match"] and not cleaned["regex"]:
        return None
    for pattern in list(cleaned["regex"]):
        try:
            re.compile(pattern)
        except re.error:
            cleaned["regex"].remove(pattern)
    if not cleaned["match"] and not cleaned["regex"]:
        return None
    _ = i
    return cleaned


def _merge(current: dict, research: dict) -> tuple[dict, list[str]]:
    """Validated merge of the research output into the policy. Fail-closed: core rules that the
    agent dropped are restored from the current policy; malformed rules are skipped."""
    changes: list[str] = []
    fresh: dict[str, dict] = {}
    for i, rule in enumerate(research.get("text_rules", [])):
        cleaned = _validate_rule(rule, i)
        if cleaned is None:
            continue
        fresh[cleaned["id"]] = cleaned
    kept: dict[str, dict] = {}
    for rule in current.get("text_rules", []):
        if isinstance(rule, dict) and rule.get("id"):
            kept[rule["id"]] = rule
    merged: dict[str, dict] = {}
    for rid in kept:
        if rid in fresh:
            merged[rid] = fresh[rid]
            if json.dumps(merged[rid], sort_keys=True) != json.dumps(kept[rid], sort_keys=True):
                changes.append(f"updated rule {rid}")
        else:
            merged[rid] = kept[rid]
            if rid in CORE_RULE_IDS:
                changes.append(f"agent dropped core rule {rid}; restored from previous policy")
            else:
                changes.append(f"kept previous rule {rid} (not in research output)")
    for rid, rule in fresh.items():
        if rid not in merged:
            merged[rid] = rule
            changes.append(f"added rule {rid} (source: {rule['source']})")

    pol = json.loads(json.dumps(current))  # deep copy
    pol["text_rules"] = [merged[rid] for rid in sorted(merged)]
    actions = [str(a)[:300] for a in research.get("required_actions", []) if str(a).strip()]
    notes = str(research.get("review_notes", "")).strip()
    pol["meta"]["reviewed_at"] = state.iso()
    pol["meta"]["reviewed_by"] = f"NIM research agent ({config.NIM_TEXT_MODEL})"
    pol["meta"]["last_required_actions"] = actions
    pol["meta"]["last_review_notes"] = notes[:2000]
    if actions:
        changes.append(f"{len(actions)} required action(s) recorded")
    return pol, changes


def _write_review(research: dict, changes: list[str], fetched: list[str]) -> None:
    lines = [
        "# Fanvue policy review", "",
        f"* Reviewed: {state.iso()} by the Nova fanvue policy agent (NIM)",
        f"* Documents re-fetched this run: {', '.join(fetched) if fetched else 'none (used existing snapshots)'}", "",
        "## What changed in the machine rules", "",
    ]
    lines += [f"- {c}" for c in changes] or ["- nothing (all rules verified against the current documents)"]
    actions = research.get("required_actions") or []
    lines += ["", "## Required actions for Eli", ""]
    lines += [f"- [ ] {a}" for a in actions] or ["- none"]
    lines += ["", "## Agent notes", "", str(research.get("review_notes", "")).strip() or "(none)"]
    REVIEW_MD.write_text("\n".join(lines) + "\n", encoding="utf8")


def needs_attention() -> list[str]:
    """Required actions from the latest review, for the app's Needs-You tab."""
    try:
        return list(load().get("meta", {}).get("last_required_actions", []))
    except (OSError, json.JSONDecodeError):
        return ["Fanvue compliance policy missing - run `py pipeline.py fanvue-policy` before any Fanvue work"]


def is_fresh() -> bool:
    try:
        age = _age_days(load().get("meta", {}).get("reviewed_at"))
    except (OSError, json.JSONDecodeError):
        return False
    if age is None:
        return False
    try:
        cadence = float(load().get("meta", {}).get("review_cadence_days", 7))
    except (OSError, json.JSONDecodeError):
        cadence = 7.0
    return age < cadence


def refresh(force: bool = False, online: bool = True) -> dict:
    """One full research cycle. Fails closed: on any error the existing policy and snapshots
    stand unchanged and the error is reported (the autonomy job retries next cadence)."""
    out: dict = {"fetched": [], "changed": False, "changes": [], "errors": [], "fresh": False}
    if not force and is_fresh():
        out["fresh"] = True
        return out
    if online:
        try:
            manifest = fetch_raw()
            out["fetched"] = sorted(manifest)
        except (urllib.error.URLError, OSError, ValueError) as e:
            out["errors"].append(f"fetch failed (keeping previous snapshots): {e}")
    try:
        docs = _corpus_docs()
        current = load()
    except (OSError, json.JSONDecodeError, FileNotFoundError) as e:
        out["errors"].append(f"corpus/policy unavailable: {e}")
        return out
    try:
        research = _research_via_nim(docs, current)
    except nim.NimUnavailable as e:
        out["errors"].append(f"research agent unavailable (policy unchanged): {e}")
        return out
    merged, changes = _merge(current, research)
    state.write_atomic(POLICY_JSON, merged)
    out["changed"] = bool(changes)
    out["changes"] = changes
    _write_review(research, changes, out["fetched"])
    state.log(f"fanvue-policy: review complete, {len(changes)} change(s), "
              f"{len(research.get('required_actions', []))} required action(s)")
    return out
