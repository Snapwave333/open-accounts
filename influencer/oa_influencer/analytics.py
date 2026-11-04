"""Growth Analyst: reads the public profile numbers, keeps a history, and once a week shifts the
planner's pillar weights toward what grew the account."""
import datetime as dt

from . import character, config, relay, state

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
    # per-post engagement (public read only): the signal the weekly review and dashboard need,
    # beyond plain follower count — never a write action.
    try:
        snap["recent_posts"] = relay.post_metrics(handle, limit=3)
        for p in snap["recent_posts"]:
            state.log(f"analytics: post {p['path']} likes={p.get('likes')} comments={p.get('comments')}")
    except Exception as e:
        state.log(f"analytics: per-post metrics skipped ({str(e)[:120]})")
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
    # the curated character pillar always exists and keeps its floor: a weekly review must never
    # silently take the fan-cosplay list out of rotation (it is the feed's content source).
    weights = {p["id"]: p["weight"] for p in spec["pillars"]}
    weights.update(st.get("pillar_weights") or {})
    weights.setdefault("famous_cosplays", config.FAMOUS_WEIGHT_FLOOR)
    weights["famous_cosplays"] = max(weights["famous_cosplays"], config.FAMOUS_WEIGHT_FLOOR)
    perf = pillar_performance()
    note = {"at": state.iso(), "performance": perf, "before": dict(weights)}
    # latest engagement read, even before follower-delta data exists: what the feed actually did
    # (likes/comments on the newest posts) — steers future tuning and the app's campaign view.
    history = state.read(state.ANALYTICS, {"history": []}).get("history", [])
    if history:
        recent = history[-1].get("recent_posts") or []
        likes = [p.get("likes") for p in recent if p.get("likes")]
        comments = [p.get("comments") for p in recent if p.get("comments")]
        if likes or comments:
            note["recent_engagement"] = {"posts": len(recent), "likes": likes, "comments": comments}
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
