"""Stories Strategist: rolling daily Stories calendar (3-5/day) for Nova Vexley.

Each day gets 3-5 photo slides spaced across waking hours. Stories carry cliffhanger-style
previews of exclusive Fanvue content (tease, never reveal the full premium set), interactive
stickers (polls, quizzes, Q&A, countdown), and tasteful "link in bio" CTAs pointing at the
Fanvue subscription page with UTM tracking.

Separate calendar from the Instagram feed calendar; shares the same character identity, themes,
and ComfyUI-generated assets pipeline. Fanvue surface stays separate from Instagram per the
two-platform rule (Eli, 2026-11-03).
"""
from __future__ import annotations

import datetime as dt
import random
import re
from pathlib import Path
from typing import Any

from . import character, comfy, config, generator, ig_to_fanvue, nim, planner, qc, relay, state

STORIES_STATE = config.DATA / "stories_calendar.json"
STORIES_PER_DAY = (3, 5)
STORY_HORIZON_DAYS = 30
# Spaced story slots per day (local time), stories land on or near these hours.
STORY_TIMES_LOCAL = (10, 14, 18, 21, 23)

STORY_TYPES = (
    "photo_slide",
    "poll",
    "quiz",
    "qa",
    "countdown",
    "link",
)

STORY_TEASERS = {
    "cosplay": "new costume drops on Fanvue Friday — subscribers see the full set first 🖤",
    "glam": "lingerie-adjacent set too spicy for the feed — full gallery on Fanvue",
    "altfashion": "platform boots + black lace preview, Fanvue-exclusive angles",
    "bts": "crafting desk + wig room, subscriber-only behind-the-scenes",
    "community": "subscriber poll: which look should I shoot next?",
    "famous_cosplays": "IP-safe parody teaser — the full homage set is Fanvue-only",
}

STORY_CTAS = {
    "photo_slide": "link in bio for the full set 🖤",
    "poll": "vote now — winners get early Fanvue access",
    "quiz": "guess the look, full reveal on Fanvue",
    "qa": "drop your questions, I answer on Fanvue",
    "countdown": "counting down to the Fanvue drop — link in bio",
    "link": "subscribe on Fanvue — link in bio",
}


class StoriesStrategist:
    """Builds and maintains the rolling 30-day Stories calendar."""

    def __init__(self) -> None:
        self.state = config.DATA / "stories_calendar.json"

    # -- persistence -----------------------------------------------------------

    def load(self) -> dict[str, Any]:
        return state.read(
            STORIES_STATE, {"slots": [], "generated_at": None, "weights": {}}
        )

    def save(self, cal: dict[str, Any]) -> None:
        state.write_atomic(STORIES_STATE, cal)

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
        story_type = rng.choice(STORY_TYPES)
        ip_safe = pillar_id == "famous_cosplays" or theme_id.startswith("fc:")
        hour24 = hour % 24
        scheduled = dt.datetime.combine(
            day, dt.time(hour24, minute), tzinfo=dt.datetime.now().astimezone().tzinfo
        )
        campaign_id = f"story-{day.isoformat()}-{index + 1}"
        return {
            "id": f"story-{day.isoformat()}-{hour24:02d}-{minute:02d}-{index + 1}",
            "platform": "instagram",
            "kind": "story",
            "story_type": story_type,
            "date": day.isoformat(),
            "scheduled_at": scheduled.isoformat(timespec="seconds"),
            "pillar": pillar_id,
            "theme": theme_id,
            "ip_safe": ip_safe,
            "teaser": STORY_TEASERS.get(pillar_id, STORY_TEASERS["cosplay"]),
            "cta": STORY_CTAS[story_type],
            "utm_campaign": campaign_id,
            "utm_medium": "story",
            "utm_content": f"{pillar_id}-{story_type}",
            "status": "planned",
            "assets": [],
            "caption": None,
            "hashtags": [],
            "sticker_config": self._sticker_config(story_type, pillar_id, rng),
            "attempts": 0,
        }

    def _weights(self, spec: dict) -> dict[str, int]:
        saved = state.state().get("story_pillar_weights")
        w = dict(saved) if saved else {p["id"]: p["weight"] for p in spec["pillars"]}
        w["famous_cosplays"] = max(w.get("famous_cosplays", 10), 10)
        return w

    def _pick_theme(self, spec: dict, pillar_id: str, rng: random.Random) -> str:
        ids = character.theme_ids(spec, pillar_id)
        return rng.choice(ids) if ids else pillar_id

    def _sticker_config(self, story_type: str, pillar_id: str, rng: random.Random) -> dict:
        if story_type == "poll":
            options = [
                "which look for Fanvue Friday?",
                "red or black next?",
            ]
            return {"type": "poll", "options": rng.sample(options, 2)}
        if story_type == "quiz":
            return {"type": "quiz", "question": "guess the Fanvue drop day", "options": ["Friday", "Saturday", "Sunday"], "correct": 0}
        if story_type == "qa":
            return {"type": "qa", "prompt": "ask me anything about the shoot 🖤"}
        if story_type == "countdown":
            return {"type": "countdown", "event": "Fanvue exclusive drop", "days": rng.randint(2, 5)}
        if story_type == "link":
            return {"type": "link", "label": "subscribe on Fanvue", "url": ""}
        return {"type": "photo_slide"}

    # -- calendar build ---------------------------------------------------------

    def build(self, today: dt.date | None = None) -> dict[str, Any]:
        """Rolling 30-day Stories calendar: 3-5 photo slides/day spaced across waking hours."""
        spec = character.spec()
        cal = self.load()
        slots = {s["id"]: s for s in cal["slots"]}
        today = today or state.now().date()
        horizon = today + dt.timedelta(days=STORY_HORIZON_DAYS)
        used: dict[str, int] = {}
        for s in cal["slots"]:
            used[s["pillar"]] = used.get(s["pillar"], 0) + 1

        added = 0
        for offset in range(STORY_HORIZON_DAYS):
            day = today + dt.timedelta(days=offset)
            day_rng = random.Random(f"nova-story:{day.isoformat()}")
            count = day_rng.randint(*STORIES_PER_DAY)
            times = day_rng.sample(STORY_TIMES_LOCAL, count)
            times.sort()
            for i, hour in enumerate(times):
                minute = day_rng.randint(0, 59)
                slot_id = f"story-{day.isoformat()}-{hour:02d}-{minute:02d}-{i + 1}"
                if slot_id in slots:
                    continue
                slot = self._base_slot(day, hour, minute, day_rng, spec, i)
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

    # -- due slots ---------------------------------------------------------------

    def due_for_generation(self, cal: dict[str, Any] | None = None) -> list[dict[str, Any]]:
        """Story slots inside the lead window that still need assets."""
        cal = cal or self.load()
        horizon = state.now() + dt.timedelta(days=config.GENERATION_LEAD_DAYS)
        out = []
        for s in cal["slots"]:
            if s["status"] not in ("planned", "rejected"):
                continue
            if s["status"] == "planned" and s.get("assets"):
                continue
            when = dt.datetime.fromisoformat(s["scheduled_at"])
            if when <= horizon and s["attempts"] < config.MAX_GENERATION_ATTEMPTS:
                out.append(s)
        return out

    def next_due_story(self, cal: dict[str, Any]) -> dict | None:
        now = state.now()
        ready = [
            s
            for s in cal["slots"]
            if s["status"] == "ready" and dt.datetime.fromisoformat(s["scheduled_at"]) <= now
        ]
        return ready[0] if ready else None


    # -- caption / hashtags ----------------------------------------------------

    def _caption(self, slot: dict, image: Path) -> str:
        """Short, cliffhanger Story caption that teases Fanvue-exclusive content."""
        spec = character.spec()
        v = spec["voice"]
        theme = character.theme(spec, slot["pillar"], slot["theme"])
        outfit, setting, mood = theme["outfit"], theme["setting"], theme["mood"]
        if slot.get("ip_safe"):
            outfit = character.strip_character_names(outfit, spec)
            setting = character.strip_character_names(setting, spec)
            mood = character.strip_character_names(mood, spec)
        system = (
            "You write Instagram Story captions for Nova Vexley, a fictional AI-generated "
            "24-year-old dark-glam cosplayer. Voice: {tone}. Never claim she is real. Never "
            "mention prices or ask to buy. No hashtags (added separately). 1-2 short lines. "
            "End with: {signoff} The caption must tease exclusive Fanvue content (cliffhanger: "
            "look, mood, 'coming soon') without revealing the full premium set, and end with a "
            "tasteful 'link in bio -> Fanvue' CTA."
        ).format(tone=v["tone"], signoff=v["signoff"])
        text = nim.chat(
            system,
            f"Pillar: {slot['pillar']}. Look: {outfit}. Setting: {setting}. Mood: {mood}. "
            f"Teaser: {slot['teaser']}. CTA: {slot['cta']}. Write the Story caption.",
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
        fanvue_url = ig_to_fanvue.build_fanvue_url(
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
        rng = random.Random(f"story-hashtags:{slot['id']}")
        core = rng.sample(pools["core"], min(5, len(pools["core"])))
        own = rng.sample(
            pools.get(slot["pillar"], []), min(3, len(pools.get(slot["pillar"], [])))
        )
        return core + own

    # -- generation ------------------------------------------------------------

    def generate_slot(
        self, session, slot: dict, refs: list[Path]
    ) -> dict:
        """Generate one QC-approved photo slide for the Story slot, then write a
        cliffhanger caption with a Fanvue CTA."""
        spec = character.spec()
        slot["attempts"] += 1

        theme = character.theme(spec, slot["pillar"], slot["theme"])
        outfit_brief = (
            __import__("re")
            .sub(r",?\s*(holding|with prop|carrying)\b.*$", "", theme["outfit"], flags=__import__("re").I)
            .strip(" ,")
        )
        out_dir = state.slot_dir(slot["id"])
        stem = f"{slot['id']}-{slot['attempts']}"
        pose_pool = [p for p in spec["poses"] if p.startswith("full body")]
        if not pose_pool:
            pose_pool = list(spec["poses"])
        frame_pose = pose_pool[slot["attempts"] % len(pose_pool)]
        prompt = character.image_prompt(
            spec, slot["pillar"], slot["theme"], frame_pose,
            glam=slot.get("glam", False),
            framing=generator.FULL_BODY_FRAMING
            if frame_pose.startswith("full body")
            else None,
            ip_safe=bool(slot.get("ip_safe")),
        )
        paths = session.images(
            prompt["prompt"], prompt["negative"],
            spec["aspect_ratios"]["story"], 1, out_dir, f"{stem}-1",
            references=refs,
        )
        path = paths[0]
        asset_id = path.stem
        verdict = qc.review(
            path, refs, spec["aspect_ratios"]["story"], outfit_brief, asset_id,
            require_full_body=frame_pose.startswith("full body"),
        )
        qc.thumbnail(path, out_dir / "thumbs" / f"{asset_id}.jpg")
        entry = {
            "id": asset_id, "path": str(path), "approved": verdict.approved,
            "reasons": verdict["reasons"], "verdict": verdict["verdict"],
            "prompt": prompt["prompt"], "pose": frame_pose,
        }
        already = [a for a in slot.get("assets", []) if a["id"] != entry["id"]]
        slot["assets"] = [entry] + already

        approved = [a for a in slot.get("assets", []) if a.get("approved")]
        if approved:
            try:
                slot["caption"] = slot["caption"] or self._caption(slot, Path(approved[0]["path"]))
                slot["hashtags"] = self._hashtags(spec, slot, slot.get("hashtags") or [])
            except nim.NimUnavailable as e:
                slot["status"] = "needs_caption"
                slot["last_error"] = str(e)
                state.log(f"story caption: {slot['id']} deferred: {e}")
                return slot
            slot["status"] = "ready"
            slot["ready_at"] = state.iso()
            state.log(f"story: {slot['id']} ready")
        else:
            slot["status"] = "rejected"
            slot["last_error"] = "; ".join(
                r for a in slot.get("assets", []) if not a.get("approved")
                for r in a.get("reasons", [])
            )[:300]
        return slot

    # -- publishing ------------------------------------------------------------

    def publish_slot(self, slot: dict, dry: bool = False) -> dict:
        """Post the Story photo slide via the relay (image -> Instagram Story).

        Requires slot["assets"][0]["path"] to point at a QC-approved image.
        """
        if not state.granted("account_created"):
            return {"result": "skipped", "slot": slot["id"], "reason": "waiting for approval: account_created"}

        approved = [a for a in slot.get("assets", []) if a.get("approved")]
        if not approved:
            return {
                "result": "skipped",
                "slot": slot["id"],
                "reason": "no approved story image; generate first",
            }

        handle = state.state()["handle"]
        media = Path(approved[0]["path"])
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
        state.log(f"story publish: {slot['id']} -> {outcome}{' (dry)' if dry else ''}")
        return {"result": outcome, "slot": slot["id"], "handle": handle}


# module-level helper functions so pipeline.py can import either class or functions
_strategist: StoriesStrategist | None = None


def _get_strategist() -> StoriesStrategist:
    global _strategist
    if _strategist is None:
        _strategist = StoriesStrategist()
    return _strategist


def build_calendar(today: dt.date | None = None) -> dict[str, Any]:
    return _get_strategist().build(today)


def load_calendar() -> dict[str, Any]:
    return _get_strategist().load()


def save_calendar(cal: dict[str, Any]) -> None:
    _get_strategist().save(cal)


def due_for_generation(cal: dict[str, Any] | None = None) -> list[dict[str, Any]]:
    return _get_strategist().due_for_generation(cal)


def next_due_story(cal: dict[str, Any]) -> dict | None:
    return _get_strategist().next_due_story(cal)


def generate_slot(session, slot: dict, refs: list[Path]) -> dict:
    return _get_strategist().generate_slot(session, slot, refs)


def publish_slot(slot: dict, dry: bool = False) -> dict:
    return _get_strategist().publish_slot(slot, dry)
