"""Content Strategist: keeps a rolling 30-day calendar. Deterministic per date so re-runs never
reshuffle slots that already exist; weights come from the analyst's latest adjustments."""
import datetime as dt
import random

from . import character, config, state

HORIZON_DAYS = 30
STORY_IDEAS = {
    "cosplay": ["poll: which costume next?", "close-up of the costume detail with a 'how I made it' line"],
    "glam": ["fit-check mirror photo", "'guess the fabric' quiz sticker"],
    "altfashion": ["outfit poll A/B", "boot shelf still (label your favourite)"],
    "bts": ["wig desk still + 'which wig?' sticker", "stream tonight? countdown sticker"],
    "community": ["Q&A sticker", "shout-out to the best comment of the week"],
    "famous_cosplays": ["which famous character next? poll", "gentle-goth twist detail close-up + franchise shout-out (fan cosplay)"],
}


def weights(spec: dict) -> dict[str, int]:
    saved = state.state().get("pillar_weights")
    w = dict(saved) if saved else {p["id"]: p["weight"] for p in spec["pillars"]}
    # keep the curated character pillar in rotation (analyst-adjusted weights can never zero it)
    w["famous_cosplays"] = max(w.get("famous_cosplays", config.FAMOUS_WEIGHT_FLOOR), config.FAMOUS_WEIGHT_FLOOR)
    return w


def _pick_pillar(rng: random.Random, w: dict[str, int]) -> str:
    ids, ws = zip(*w.items())
    return rng.choices(ids, weights=ws, k=1)[0]


def _theme_for(spec: dict, pillar_id: str, used: dict[str, int]) -> str:
    """Round-robin through a pillar's themes so no look repeats before the others ran."""
    themes = character.theme_ids(spec, pillar_id)
    i = used.get(pillar_id, 0) % len(themes)
    used[pillar_id] = i + 1
    return themes[i]
    return themes[i]["id"]


def build(today: dt.date | None = None) -> dict:
    spec = character.spec()
    cal = state.calendar()
    slots = {s["id"]: s for s in cal["slots"]}
    today = today or state.now().date()
    w = weights(spec)
    used: dict[str, int] = {}
    for s in cal["slots"]:
        used[s["pillar"]] = used.get(s["pillar"], 0) + 1

    added = 0
    for offset in range(HORIZON_DAYS):
        day = today + dt.timedelta(days=offset)
        slot_id = f"{day.isoformat()}-feed"
        if slot_id in slots:
            continue
        rng = random.Random(f"{spec['slug']}:{day.isoformat()}")
        pillar_id = _pick_pillar(rng, w)
        prev = slots.get(f"{(day - dt.timedelta(days=1)).isoformat()}-feed")
        week_ago = [slots.get(f"{(day - dt.timedelta(days=k)).isoformat()}-feed") for k in range(1, 7)]
        for _ in range(8):  # never the same pillar two days running; community at most once a week
            same_as_yesterday = prev and prev["pillar"] == pillar_id
            community_this_week = pillar_id == "community" and any(s and s["pillar"] == "community" for s in week_ago)
            if not (same_as_yesterday or community_this_week):
                break
            pillar_id = _pick_pillar(rng, w)
        theme_id = _theme_for(spec, pillar_id, used)
        # Every post is a FULL-size IG carousel: max images per post (20).
        kind = "carousel"
        post_at = dt.datetime.combine(day, dt.time(config.POST_HOUR_LOCAL)).astimezone()
        slot = {
            "id": slot_id,
            "date": day.isoformat(),
            "scheduled_at": post_at.isoformat(timespec="seconds"),
            "kind": kind,
            "images": int(config.FEED_MAX_IMAGES),
            "pillar": pillar_id,
            "theme": theme_id,
            "pose": character.pick_pose(spec, rng),
            "glam": pillar_id == "glam" or rng.random() < 0.25,
            "aspect_ratio": spec["aspect_ratios"]["feed"],
            "story": rng.choice(STORY_IDEAS[pillar_id]),
            "status": "planned",
            "attempts": 0,
            "assets": [],
            "caption": None,
            "hashtags": character.hashtags(spec, pillar_id, rng),
        }
        slots[slot_id] = slot
        added += 1

    cal["slots"] = sorted(slots.values(), key=lambda s: s["scheduled_at"])
    cal["generated_at"] = state.iso()
    cal["weights"] = w
    state.save_calendar(cal)
    return {"added": added, "total": len(cal["slots"]), "weights": w}


def due_for_generation(cal: dict) -> list[dict]:
    """Slots inside the lead window that still need assets and have attempts left."""
    horizon = state.now() + dt.timedelta(days=config.GENERATION_LEAD_DAYS)
    out = []
    for s in cal["slots"]:
        when = dt.datetime.fromisoformat(s["scheduled_at"])
        if s["status"] in ("planned", "rejected") and when <= horizon and s["attempts"] < config.MAX_GENERATION_ATTEMPTS:
            out.append(s)
    return out


def next_due_post(cal: dict) -> dict | None:
    now = state.now()
    ready = [s for s in cal["slots"] if s["status"] == "ready" and dt.datetime.fromisoformat(s["scheduled_at"]) <= now]
    return ready[0] if ready else None
