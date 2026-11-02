"""Growth Analyst: reads the public profile numbers, keeps a history, and once a week shifts the
planner's pillar weights toward what grew the account."""
import datetime as dt

from . import character, relay, state

MILESTONES = {"Instagram Subscriptions": 10_000, "Creator Marketplace": 1_000, "first affiliate push": 500}
WEIGHT_MIN, WEIGHT_MAX, STEP = 5, 50, 5


def snapshot() -> dict:
    handle = state.state().get("handle")
    if not handle or not state.granted("account_created"):
        raise RuntimeError("no approved account yet")
    data = state.read(state.ANALYTICS, {"history": [], "milestones": {}})
    meta = relay.profile_meta(handle)
    if not meta["exists"]:
        raise RuntimeError(f"@{handle} profile not readable (logged out, private, or page changed)")
    prev = data["history"][-1] if data["history"] else None
    snap = {**meta, "delta_followers": (meta["followers"] - prev["followers"]) if prev and prev.get("followers") is not None else None}
    data["history"] = (data["history"] + [snap])[-400:]
    for name, need in MILESTONES.items():
        if meta["followers"] >= need and name not in data["milestones"]:
            data["milestones"][name] = state.iso()
            state.log(f"MONETIZATION: @{handle} reached {need:,} followers: {name} is now possible (Eli's call)")
    state.write_atomic(state.ANALYTICS, data)
    state.log(f"analytics: @{handle} {meta['followers']} followers, {meta['posts']} posts")
    return snap


def pillar_performance() -> dict[str, float]:
    """Average follower gain on the day after each pillar's post (the only signal the public profile
    gives without an API). Enough to steer weights; not a substitute for Insights."""
    data = state.read(state.ANALYTICS, {"history": []})
    by_day: dict[str, int] = {}
    for snap in data["history"]:
        if snap.get("followers") is not None:
            by_day[snap["checked_at"][:10]] = snap["followers"]
    gains: dict[str, list[int]] = {}
    for slot in state.calendar()["slots"]:
        if slot["status"] != "posted":
            continue
        day = slot["posted_at"][:10]
        nxt = (dt.date.fromisoformat(day) + dt.timedelta(days=1)).isoformat()
        if day in by_day and nxt in by_day:
            gains.setdefault(slot["pillar"], []).append(by_day[nxt] - by_day[day])
    return {p: sum(v) / len(v) for p, v in gains.items() if v}


def weekly_review() -> dict:
    """Moves weight from the weakest pillar to the strongest, by one step, and writes the note."""
    spec = character.spec()
    st = state.state()
    weights = dict(st.get("pillar_weights") or {p["id"]: p["weight"] for p in spec["pillars"]})
    perf = pillar_performance()
    note = {"at": state.iso(), "performance": perf, "before": dict(weights)}
    if len(perf) >= 2:
        best = max(perf, key=perf.get)
        worst = min(perf, key=perf.get)
        if best != worst and weights[worst] - STEP >= WEIGHT_MIN and weights[best] + STEP <= WEIGHT_MAX:
            weights[worst] -= STEP
            weights[best] += STEP
            note["change"] = f"+{STEP} {best}, -{STEP} {worst}"
        else:
            note["change"] = "none (bounds)"
    else:
        note["change"] = "none (not enough posted days with follower data)"
    st["pillar_weights"] = weights
    st["last_weekly_review"] = note["at"]
    state.save_state(st)
    note["after"] = weights
    week = state.now().strftime("%G-W%V")
    out = state.ANALYTICS.parent / "analytics" / f"weekly-{week}.md"
    out.parent.mkdir(parents=True, exist_ok=True)
    lines = [f"# Weekly review {week}", "", f"Change: {note['change']}", "", "| pillar | avg next-day follower gain | weight |", "|---|---|---|"]
    for p in spec["pillars"]:
        lines.append(f"| {p['id']} | {perf.get(p['id'], '—')} | {weights[p['id']]} |")
    out.write_text("\n".join(lines) + "\n", encoding="utf8")
    state.log(f"analytics: weekly review -> {note['change']}")
    return note


def weekly_due() -> bool:
    last = state.state().get("last_weekly_review")
    return not last or state.now() - dt.datetime.fromisoformat(last) >= dt.timedelta(days=7)
