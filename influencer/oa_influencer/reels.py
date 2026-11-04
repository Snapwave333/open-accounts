"""Reels Strategist: rolling daily Reel calendar (1/day) for Nova Vexley.

Each day gets one photo-slideshow Reel (~15-30s) built from QC-approved still frames
with text overlays and music. Reels carry cliffhanger-style previews of exclusive
Fanvue content (tease, never reveal the full premium set) and tasteful "link in
bio" CTAs pointing at the Fanvue subscription page with UTM tracking.

Separate calendar from the Instagram feed and Stories calendars; shares the same
character identity, themes, and ComfyUI-generated assets pipeline.

Publishing a Reel needs a rendered slideshow video (frames -> video -> relay
share_media, which Instagram turns into a Reel). Without a video renderer the
publish command marks slots "needs_render" and returns instructions instead of
posting.
"""
from __future__ import annotations

import datetime as dt
import random
from pathlib import Path
from typing import Any

from . import character, config, generator, nim, planner, qc, relay, state

REELS_STATE = config.DATA / "reels_calendar.json"
REELS_PER_DAY = 1
REEL_HORIZON_DAYS = 30
# One Reel per day at 12:00 local (midday), separate from the 19:00 feed post.
REEL_TIME_LOCAL = 12

REEL_TEASERS = {
    "cosplay": "full costume set drops on Fanvue Friday — subscribers see it first 🖤",
    "glam": "lingerie-adjacent set too spicy for the feed — full gallery on Fanvue",
    "altfashion": "platform boots + black lace preview, Fanvue-exclusive angles",
    "bts": "crafting desk + wig room, subscriber-only behind-the-scenes",
    "community": "subscriber pick: which look should I shoot next?",
    "famous_cosplays": "IP-safe parody homage — the full set is Fanvue-only",
}

REEL_CTAS = {
    "photo_slide": "link in bio for the full set 🖤",
    "poll": "vote now — winners get early Fanvue access",
    "quiz": "guess the look, full reveal on Fanvue",
    "qa": "drop your questions, I answer on Fanvue",
    "countdown": "counting down to the Fanvue drop — link in bio",
    "link": "subscribe on Fanvue — link in bio",
}

# How many frames from the day's feed carousel a Reel slideshow uses.
REEL_FRAME_TARGET = 8


class ReelsStrategist:
    """Builds and maintains the rolling 30-day Reel calendar."""

    def __init__(self) -> None:
        self.state_path = REELS_STATE

    # -- persistence -----------------------------------------------------------

    def load(self) -> dict[str, Any]:
        return state.read(
            self.state_path, {"slots": [], "generated_at": None, "weights": {}}
        )

    def save(self, cal: dict[str, Any]) -> None:
        state.write_atomic(self.state_path, cal)

    # -- slot construction -----------------------------------------------------

    def _base_slot(
        self,
        day: dt.date,
        hour: int,
        minute: int,
        rng: random.Random,
        spec: dict,
        index: int,
    ) -> dict:
        pillar_id = planner._pick_pillar(rng, self._weights(spec))
        theme_id = self._pick_theme(spec, pillar_id, rng)
        ip_safe = pillar_id == "famous_cosplays" or theme_id.startswith("fc:")
        scheduled = dt.datetime.combine(
            day, dt.time(hour, minute), tzinfo=dt.datetime.now().astimezone().tzinfo
        )
        campaign_id = f"reel-{day.isoformat()}-{index + 1}"
        return {
            "id": f"reel-{day.isoformat()}-{hour:02d}-{minute:02d}",
            "platform": "instagram",
            "kind": "reel",
            "date": day.isoformat(),
            "scheduled_at": scheduled.isoformat(timespec="seconds"),
            "pillar": pillar_id,
            "theme": theme_id,
            "ip_safe": ip_safe,
            "source_slot": None,  # feed slot id whose frames become the slideshow
            "teaser": REEL_TEASERS.get(pillar_id, REEL_TEASERS["cosplay"]),
            "cta": REEL_CTAS["photo_slide"],
            "utm_campaign": campaign_id,
            "utm_medium": "reel",
            "utm_content": f"{pillar_id}-reel",
            "status": "planned",
            "assets": [],
            "caption": None,
            "hashtags": [],
            "rendered": None,
            "attempts": 0,
        }

    def _weights(self, spec: dict) -> dict[str, int]:
        saved = state.state().get("reel_pillar_weights")
        w = dict(saved) if saved else {p["id"]: p["weight"] for p in spec["pillars"]}
        w["famous_cosplays"] = max(w.get("famous_cosplays", 10), 10)
        return w

    def _pick_theme(self, spec: dict, pillar_id: str, rng: random.Random) -> str:
        ids = character.theme_ids(spec, pillar_id)
        return rng.choice(ids) if ids else pillar_id

    # -- calendar build ---------------------------------------------------------

    def build(self, today: dt.date | None = None) -> dict[str, Any]:
        """Rolling 30-day Reel calendar: 1 Reel/day at midday local."""
        spec = character.spec()
        cal = self.load()
        slots = {s["id"]: s for s in cal["slots"]}
        today = today or state.now().date()
        horizon = today + dt.timedelta(days=REEL_HORIZON_DAYS)
        used: dict[str, int] = {}
        for s in cal["slots"]:
            used[s["pillar"]] = used.get(s["pillar"], 0) + 1

        added = 0
        for offset in range(REEL_HORIZON_DAYS):
            day = today + dt.timedelta(days=offset)
            day_rng = random.Random(f"nova-reel:{day.isoformat()}")
            slot_id = f"reel-{day.isoformat()}-{REEL_TIME_LOCAL:02d}-00"
            if slot_id in slots:
                continue
            slot = self._base_slot(day, REEL_TIME_LOCAL, 0, day_rng, spec, 0)
            slot["id"] = slot_id
            # round-robin theme within the pillar so looks don't repeat before others ran
            pillar_used = used.get(pillar_id := slot["pillar"], 0)
            theme_ids = character.theme_ids(spec, pillar_id)
            if theme_ids:
                slot["theme"] = theme_ids[pillar_used % len(theme_ids)]
                used[pillar_id] = pillar_used + 1
            slots[slot_id] = slot
            added += 1

        cal["slots"] = sorted(slots.values(), key=lambda s: s["scheduled_at"])
        cal["generated_at"] = state.iso()
        cal["weights"] = self._weights(spec)
        self.save(cal)
        return {"added": added, "total": len(cal["slots"]), "weights": cal["weights"]}

    # -- due slots --------------------------------------------------------------

    def due_for_generation(self, cal: dict[str, Any] | None = None) -> list[dict[str, Any]]:
        """Reel slots inside the lead window that still need frames + caption."""
        cal = cal or self.load()
        horizon = state.now() + dt.timedelta(days=config.GENERATION_LEAD_DAYS)
        out = []
        for s in cal["slots"]:
            if s["status"] not in ("planned", "rejected"):
                continue
            if s["status"] == "planned" and s.get("assets") and s.get("caption"):
                continue
            when = dt.datetime.fromisoformat(s["scheduled_at"])
            if when <= horizon and s["attempts"] < config.MAX_GENERATION_ATTEMPTS:
                out.append(s)
        return out

    def next_due_reel(self, cal: dict[str, Any]) -> dict | None:
        now = state.now()
        ready = [
            s
            for s in cal["slots"]
            if s["status"] == "ready" and dt.datetime.fromisoformat(s["scheduled_at"]) <= now
        ]
        return ready[0] if ready else None

    # -- generation --------------------------------------------------------------

    def generate_slot(
        self, session, slot: dict, refs: list[Path], feed_cal: dict[str, Any] | None = None
    ) -> dict:
        """Select frames from the same day's feed carousel (or generate dedicated
        frames), then write a cliffhanger caption with a Fanvue CTA."""
        spec = character.spec()
        slot["attempts"] += 1

        # 1. Find frames: prefer the same day's feed carousel approved assets.
        source_slot = None
        feed_cal = feed_cal or state.calendar()
        for fs in feed_cal.get("slots", []):
            if fs.get("date") == slot["date"] and fs.get("platform") == "instagram":
                approved = [a for a in fs.get("assets", []) if a.get("approved")]
                if len(approved) >= 3:
                    source_slot = fs
                    break

        if source_slot:
            slot["source_slot"] = source_slot["id"]
            chosen = sorted(
                source_slot["assets"],
                key=lambda a: a.get("id", ""),
            )[:REEL_FRAME_TARGET]
            slot["assets"] = [
                {
                    "id": a["id"],
                    "path": a["path"],
                    "approved": True,
                    "reasons": a.get("reasons", []),
                    "verdict": a.get("verdict", "feed-carousel"),
                    "prompt": a.get("prompt", ""),
                    "pose": a.get("pose", ""),
                }
                for a in chosen
            ]
            slot["images"] = len(slot["assets"])
        else:
            # No feed slot ready yet: shoot dedicated frames for the Reel.
            slot["kind"] = "reel"
            slot["images"] = REEL_FRAME_TARGET
            already = [a for a in slot.get("assets", []) if a.get("approved")]
            remaining = REEL_FRAME_TARGET - len(already)
            if remaining > 0:
                pose_pool = [p for p in spec["poses"] if p.startswith("full body")]
                if not pose_pool:
                    pose_pool = list(spec["poses"])
                out_dir = state.slot_dir(slot["id"])
                stem = f"{slot['id']}-{slot['attempts']}"
                approved, rejected = [], already
                budget = min(remaining, config.GEN_FRAMES_PER_RUN)
                for i in range(budget):
                    frame_pose = pose_pool[i % len(pose_pool)]
                    prompt = character.image_prompt(
                        spec, slot["pillar"], slot["theme"], frame_pose,
                        glam=slot["glam"],
                        framing=generator.FULL_BODY_FRAMING
                        if frame_pose.startswith("full body")
                        else None,
                        ip_safe=bool(slot.get("ip_safe")),
                    )
                    paths = session.images(
                        prompt["prompt"], prompt["negative"], slot["aspect_ratio"], 1,
                        out_dir, f"{stem}-{i + 1}", references=refs,
                    )
                    path = paths[0]
                    asset_id = path.stem
                    outfit_brief = (
                        __import__("re")
                        .sub(r",?\s*(holding|with prop|carrying)\b.*$", "", character.theme(spec, slot["pillar"], slot["theme"])["outfit"], flags=__import__("re").I)
                        .strip(" ,")
                    )
                    verdict = qc.review(
                        path, refs, slot["aspect_ratio"], outfit_brief, asset_id,
                        require_full_body=frame_pose.startswith("full body"),
                    )
                    qc.thumbnail(path, out_dir / "thumbs" / f"{asset_id}.jpg")
                    entry = {
                        "id": asset_id, "path": str(path), "approved": verdict.approved,
                        "reasons": verdict["reasons"], "verdict": verdict["verdict"],
                        "prompt": prompt["prompt"], "pose": frame_pose,
                    }
                    (approved if verdict.approved else rejected).append(entry)
                slot["assets"] = slot.get("assets", []) + [
                    a for a in approved if a["id"] not in {x["id"] for x in already}
                ] + [a for a in rejected if a["id"] not in {x["id"] for x in already}]

        approved = [a for a in slot.get("assets", []) if a.get("approved")]
        if len(approved) >= slot.get("images", REEL_FRAME_TARGET):
            try:
                slot["caption"] = slot["caption"] or self._caption(slot, Path(approved[0]["path"]))
                slot["hashtags"] = self._hashtags(spec, slot, slot.get("hashtags") or [])
            except nim.NimUnavailable as e:
                slot["status"] = "needs_caption"
                slot["last_error"] = str(e)
                state.log(f"reel caption: {slot['id']} deferred: {e}")
                return slot
            slot["status"] = "ready"
            slot["ready_at"] = state.iso()
            state.log(f"reel: {slot['id']} ready with {len(approved)} frames")
        elif approved:
            slot["last_error"] = f"{len(approved)}/{slot.get('images', REEL_FRAME_TARGET)} frames approved so far (continues next run)"
        else:
            slot["status"] = "rejected"
            slot["last_error"] = "; ".join(
                r for a in slot.get("assets", []) if not a.get("approved")
                for r in a.get("reasons", [])
            )[:300]
        return slot

    def _caption(self, slot: dict, image: Path) -> str:
        spec = character.spec()
        v = spec["voice"]
        theme = character.theme(spec, slot["pillar"], slot["theme"])
        outfit, setting, mood = theme["outfit"], theme["setting"], theme["mood"]
        if slot.get("ip_safe"):
            outfit = character.strip_character_names(outfit, spec)
            setting = character.strip_character_names(setting, spec)
            mood = character.strip_character_names(mood, spec)
        system = (
            "You write Instagram Reel captions for Nova Vexley, a fictional AI-generated 24-year-old "
            "dark-glam cosplayer. Voice: {tone}. Never claim she is real. Never mention prices or ask to buy. "
            "No hashtags (added separately). 1-3 short lines. End with: {signoff} "
            "The caption must tease exclusive Fanvue content (cliffhanger: look, mood, 'coming soon') "
            "without revealing the full premium set, and end with a tasteful 'link in bio -> Fanvue' CTA."
        ).format(tone=v["tone"], signoff=v["signoff"])
        text = nim.chat(
            system,
            f"Pillar: {slot['pillar']}. Look: {outfit}. Setting: {setting}. Mood: {mood}. "
            f"Teaser: {slot['teaser']}. CTA: {slot['cta']}. Write the Reel caption for the attached photo.",
            images=[image],
            temperature=0.8,
            max_tokens=config.NIM_CAPTION_TOKENS,
        )
        text = text.strip().strip('"')
        if slot.get("ip_safe"):
            text = character.strip_character_names(text, spec)
        if v["signoff"] not in text:
            text = f"{text}\n{v['signoff']}"
        disclosure = v.get("ai_disclosure", "")
        fanvue_url = build_fanvue_url(
            slot["utm_campaign"], slot["utm_medium"], slot["utm_content"]
        )
        return (
            f"{text}\n\n{slot['teaser']}\n{slot['cta']} {fanvue_url}\n\n{disclosure}\n\n"
            f"{' '.join(slot.get('hashtags') or [])}"
        )

    def _hashtags(self, spec: dict, slot: dict, existing: list[str]) -> list[str]:
        if existing:
            return existing
        pools = spec["hashtag_pools"]
        import random as _r
        rng = _r.Random(f"reel-hashtags:{slot['id']}")
        core = rng.sample(pools["core"], min(5, len(pools["core"])))
        own = rng.sample(
            pools.get(slot["pillar"], []), min(3, len(pools.get(slot["pillar"], [])))
        )
        return core + own

    # -- publishing -------------------------------------------------------------

    def publish_slot(self, slot: dict, dry: bool = False) -> dict:
        """Post the rendered Reel video via the relay (video -> Instagram Reel).

        Requires slot["rendered"] to point at a slideshow video file. Without a
        video renderer, callers must render frames -> video first and set
        slot["rendered"] before publishing.
        """
        if not state.granted("account_created"):
            return {"result": "skipped", "slot": slot["id"], "reason": "waiting for approval: account_created"}

        rendered = slot.get("rendered")
        if not rendered or not Path(rendered).exists():
            return {
                "result": "needs_render",
                "slot": slot["id"],
                "reason": "no rendered slideshow video; render frames -> video then set slot['rendered']",
                "frames": [a["path"] for a in slot.get("assets", []) if a.get("approved")],
            }

        handle = state.state()["handle"]
        media = Path(rendered)
        if config.INSTAGRAM_VIA == "emulator":
            from . import emu
            outcome = emu.post_image(media, slot["caption"], dry=dry, ai_label=True)
        else:
            with relay.studio_session(handle) as (_tr, ig):
                outcome = ig.share_media(media, slot["caption"], slot["id"], dry=dry, ai_label=True)

        entry = {
            "at": state.iso(), "slot": slot["id"], "kind": slot["kind"],
            "media": str(media), "handle": handle, "result": outcome, "dry": dry,
        }
        state.record_post(entry)
        if outcome == "posted" and not dry:
            slot["status"] = "posted"
            slot["posted_at"] = entry["at"]
        elif outcome == "wrong-account":
            slot["last_error"] = f"@{handle} is not signed in on this Chrome"
            state.revoke("account_created")
        elif outcome not in ("dry", "pending"):
            slot["last_error"] = f"share returned {outcome}"
        state.log(f"reel publish: {slot['id']} -> {outcome}{' (dry)' if dry else ''}")
        return {"result": outcome, "slot": slot["id"], "handle": handle}


# module-level helpers
_strategist: ReelsStrategist | None = None


def _get_strategist() -> ReelsStrategist:
    global _strategist
    if _strategist is None:
        _strategist = ReelsStrategist()
    return _strategist


def build_calendar(today: dt.date | None = None) -> dict[str, Any]:
    return _get_strategist().build(today)


def load_calendar() -> dict[str, Any]:
    return _get_strategist().load()


def save_calendar(cal: dict[str, Any]) -> None:
    _get_strategist().save(cal)


def due_for_generation(cal: dict[str, Any] | None = None) -> list[dict[str, Any]]:
    return _get_strategist().due_for_generation(cal)


def next_due_reel(cal: dict[str, Any]) -> dict | None:
    return _get_strategist().next_due_reel(cal)


def generate_slot(session, slot: dict, refs: list[Path],
                  feed_cal: dict[str, Any] | None = None) -> dict:
    return _get_strategist().generate_slot(session, slot, refs, feed_cal)


def publish_slot(slot: dict, dry: bool = False) -> dict:
    return _get_strategist().publish_slot(slot, dry)


def build_fanvue_url(campaign_id: str, medium: str, content: str) -> str:
    """UTM-tagged Fanvue subscription URL for a given campaign."""
    from urllib.parse import urlencode, urlparse, parse_qs, urlunparse

    base = "https://fanvue.com/nova_a_aa_"
    params = {
        "utm_source": "instagram",
        "utm_medium": medium,
        "utm_campaign": campaign_id,
        "utm_content": content,
    }
    parts = urlparse(base)
    qs = parse_qs(parts.query)
    qs.update(params)
    return urlunparse((
        parts.scheme, parts.netloc, parts.path, parts.params,
        urlencode(qs, doseq=True), parts.fragment,
    ))
