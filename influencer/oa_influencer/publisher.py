"""Social Media Manager: posts the next due, approved slot on Nova's account. Refuses without the
human gates, respects the daily cap and spacing, and records exactly what Instagram confirmed."""
import datetime as dt
from pathlib import Path

from . import config, planner, relay, state


class NotAllowed(RuntimeError):
    pass


def _gates() -> None:
    # No publishing gate (Eli, 2026-11-03): everything QC-approved gets posted on cadence.
    if not state.granted("account_created"):
        raise NotAllowed("waiting for approval: account_created")
    if not state.state().get("handle"):
        raise NotAllowed("no handle chosen yet (run `pipeline.py handles`)")


def _spacing_ok(kind: str) -> str | None:
    now = state.now()
    recent = [p for p in state.posts() if p["result"] == "posted"]
    today = [p for p in recent if p["at"][:10] == now.date().isoformat()]
    if len(today) >= config.FEED_POSTS_PER_DAY:
        return f"daily cap reached ({config.FEED_POSTS_PER_DAY} feed post/day)"
    if recent:
        last = dt.datetime.fromisoformat(max(p["at"] for p in recent))
        gap = now - last
        if gap < dt.timedelta(hours=config.MIN_FEED_GAP_HOURS):
            return f"last feed post {gap.seconds // 3600}h ago, minimum gap {config.MIN_FEED_GAP_HOURS}h"
    return None


def run(dry: bool = False) -> dict:
    _gates()
    cal = state.calendar()
    slot = planner.next_due_post(cal)
    if not slot:
        return {"result": "nothing due"}
    reason = _spacing_ok(slot["kind"])
    if reason:
        return {"result": "skipped", "slot": slot["id"], "reason": reason}

    handle = state.state()["handle"]
    approved = [a for a in slot["assets"] if a["approved"]]
    first = approved[0] if approved else None
    if not first:
        return {"result": "skipped", "slot": slot["id"], "reason": "no approved image"}
    media = Path(first["path"])
    # A full-size carousel when the shoot is ready: queue every approved frame (IG cap).
    extras = [Path(a["path"]) for a in approved[1:config.FEED_MAX_IMAGES]]
    if extras:
        state.log(f"publish: {slot['id']} is a {len(extras) + 1}-image carousel")
    else:
        state.log(f"publish: {slot['id']} is a carousel; posting the lead frame (only one frame approved)")
    if not media.exists() or any(not p.exists() for p in extras):
        slot["status"] = "rejected"
        slot["last_error"] = f"media missing: {media}"
        state.save_calendar(cal)
        return {"result": "failed", "slot": slot["id"], "reason": slot["last_error"]}

    if config.INSTAGRAM_VIA == "emulator":
        # Headless Pixel7 emulator (emulator-5554), Instagram app signed in as @nova_a_aa_ only.
        # Posts a single lead frame (no carousel) via the app; Snapwave (@snapwave) is never touched.
        from . import emu
        outcome = emu.post_image(media, slot["caption"], dry=dry, ai_label=True)
    else:
        with relay.studio_session(handle) as (_tr, ig):
            outcome = ig.share_media(media, slot["caption"], slot["id"], dry=dry, ai_label=True, extras=extras)

    entry = {"at": state.iso(), "slot": slot["id"], "kind": slot["kind"], "media": str(media),
             "handle": handle, "result": outcome, "dry": dry}
    state.record_post(entry)
    if outcome == "posted" and not dry:
        slot["status"] = "posted"
        slot["posted_at"] = entry["at"]
    elif outcome == "wrong-account":
        slot["last_error"] = f"@{handle} is not signed in on this Chrome"
        state.revoke("account_created")
    elif outcome not in ("dry", "pending"):
        slot["last_error"] = f"share returned {outcome}"
    state.save_calendar(cal)
    state.log(f"publish: {slot['id']} -> {outcome}{' (dry)' if dry else ''}")
    return {"result": outcome, "slot": slot["id"], "handle": handle}
